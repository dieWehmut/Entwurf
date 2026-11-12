using System.Security.Cryptography;
using System.Text.Json;

namespace Entwurf.Bridge;

/// <summary>Durable submission intent and original-image staging, independent of window selectors and WebSocket lifetimes.</summary>
public sealed class ImageAutomationSession : IAsyncDisposable
{
    public const int MaximumImageBytes = 32 * 1024 * 1024;
    readonly string directory;
    readonly IImageAutomationSurface surface;
    readonly Func<JsonElement, CancellationToken, Task> send;
    readonly SemaphoreSlim gate = new(1, 1);
    readonly SemaphoreSlim reports = new(1, 1);
    readonly FileStream ownership;
    readonly CancellationTokenSource lifetime = new();
    Dictionary<string, ImageRunRecord> records;
    const int MaximumResumeRequests = 64;
    sealed record RunningControl(string Id, CancellationTokenSource Cancellation);
    RunningControl? runningControl;
    Task running = Task.CompletedTask;

    public ImageAutomationSession(string stateDirectory, IImageAutomationSurface surface,
        Func<JsonElement, CancellationToken, Task> send)
    {
        directory = Path.GetFullPath(stateDirectory);
        Directory.CreateDirectory(directory);
        EnsureOrdinaryPath(directory);
        ownership = new FileStream(Path.Combine(directory, "adapter.lock"), FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.None);
        this.surface = surface;
        this.send = send;
        try
        {
            var database = Path.Combine(directory, "runs.json");
            records = File.Exists(database)
                ? JsonSerializer.Deserialize<Dictionary<string, ImageRunRecord>>(File.ReadAllBytes(database), ImageAutomationJson.Options) ?? throw new InvalidDataException()
                : new(StringComparer.Ordinal);
            if (records.Count > 10_000 || records.Any(x => !ImageAutomationRun.ValidId(x.Key) || x.Key != x.Value.RunId))
                throw new InvalidDataException("invalid_state");
            var requests = records.Values.SelectMany(record => record.ResumeRequestIds ?? []).ToArray();
            if (records.Values.Any(record => record.ResumeRequestIds is { Length: > MaximumResumeRequests })
                || requests.Any(id => !Guid.TryParseExact(id, "D", out var parsed) || id != parsed.ToString("D"))
                || requests.Distinct(StringComparer.Ordinal).Count() != requests.Length)
                throw new InvalidDataException("invalid_resume_history");
            foreach (var (id, record) in records.ToArray())
            {
                if (record.Phase == "awaiting_import")
                {
                    if (!ArtifactMatches(record)) records[id] = record with { Phase = "paused", Reason = "artifact_invalid" };
                }
                else if (!record.Terminal) records[id] = record with { Phase = "paused", Reason = "adapter_restarted" };
            }
            Persist(records);
        }
        catch { ownership.Dispose(); throw; }
    }

    public async Task ConnectedAsync(CancellationToken cancellation)
    {
        await ProbeAsync(cancellation);
        ImageRunRecord[] snapshot;
        await gate.WaitAsync(cancellation);
        try { snapshot = records.Values.ToArray(); }
        finally { gate.Release(); }
        foreach (var record in snapshot.Where(x => !x.Terminal)) await ReportAsync(record, cancellation);
    }

    public async Task HandleAsync(JsonElement message, CancellationToken cancellation)
    {
        var type = message.TryGetProperty("type", out var t) ? t.GetString() : null;
        if (type == "probe") { await ProbeAsync(cancellation); return; }
        if (type == "resume_pre_submit")
        {
            var run = message.GetProperty("run").Deserialize<ImageAutomationRun>(ImageAutomationJson.Options) ?? throw new ImageAutomationException("invalid_run");
            run.Validate();
            var requestId = message.GetProperty("request_id").GetString();
            if (!Guid.TryParseExact(requestId, "D", out _)) throw new ImageAutomationException("invalid_request_id");
            await ResumePreSubmitAsync(run, Guid.Parse(requestId!).ToString("D"), cancellation);
            return;
        }
        if (type == "start")
        {
            var run = message.GetProperty("run").Deserialize<ImageAutomationRun>(ImageAutomationJson.Options) ?? throw new ImageAutomationException("invalid_run");
            run.Validate();
            await StartAsync(run, cancellation);
            return;
        }
        if (!message.TryGetProperty("run_id", out var idValue) || !ImageAutomationRun.ValidId(idValue.GetString())) return;
        var id = idValue.GetString()!;
        if (type == "cancel")
        {
            // Cancellation is observable before waiting for any durable transition or UI call.
            var active = Volatile.Read(ref runningControl);
            if (active?.Id == id)
                try { active.Cancellation.Cancel(); } catch (ObjectDisposedException) { /* A completed run was replaced concurrently. */ }
        }
        ImageRunRecord? report = null;
        TaskCompletionSource? resume = null;
        await gate.WaitAsync(cancellation);
        try
        {
            if (!records.TryGetValue(id, out var record)) return;
            if (type == "cancel" && !record.Terminal)
            {
                record = record with { Phase = "cancelled", Reason = "cancelled" };
                Save(record);
            }
            else if (type == "artifact_accepted" && record.Phase == "awaiting_import")
            {
                if (!ArtifactMatches(record)) record = record with { Phase = "paused", Reason = "artifact_invalid" };
                else record = record with { Phase = "completed", Reason = null };
                Save(record); // Never delete bytes or advertise completion before durable application ACK.
                if (record.Phase == "completed") DeleteOwnedArtifact(record);
            }
            else if (type == "status" && record.Phase == "paused" && record.SubmitIntent is true && record.SurfaceBinding is not null && running.IsCompleted)
            {
                // Status can resume only observation/capture of an already submitted, durably bound turn.
                // The recovery path contains no Prepare, prompt-writing, or Submit calls.
                var control = new RunningControl(id, CancellationTokenSource.CreateLinkedTokenSource(lifetime.Token));
                Interlocked.Exchange(ref runningControl, control)?.Cancellation.Dispose();
                resume = new(TaskCreationOptions.RunContinuationsAsynchronously);
                var begin = resume.Task;
                running = Task.Run(async () => { await begin; await RecoverAsync(record, control.Cancellation.Token); });
                _ = ReportAfterWorkerAsync(running, id);
            }
            report = record;
        }
        finally { gate.Release(); }
        try { if (report is not null) await ReportAsync(report, cancellation); }
        finally { resume?.TrySetResult(); }
    }

    bool PreSubmitRetryable(ImageRunRecord record)
    {
        if (record.Phase != "paused" || record.SubmitIntent is not false || record.SurfaceBinding is not null
            || record.Sha256 is not null || record.MimeType is not null || !running.IsCompleted
            || (record.ResumeRequestIds?.Length ?? 0) >= MaximumResumeRequests
            || records.Values.Any(other => other.RunId != record.RunId && !other.Terminal)) return false;
        try
        {
            var artifact = ArtifactPath(record.RunId);
            EnsureOrdinaryPath(artifact);
            if (File.Exists(artifact) || Directory.Exists(artifact)) return false;
            var downloads = Path.Combine(directory, "downloads", record.RunId);
            EnsureOrdinaryPath(downloads);
            return !File.Exists(downloads) && (!Directory.Exists(downloads) || !Directory.EnumerateFileSystemEntries(downloads).Any());
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException or ImageAutomationException) { return false; }
    }

    async Task ResumePreSubmitAsync(ImageAutomationRun run, string requestId, CancellationToken cancellation)
    {
        ImageRunRecord? report = null;
        string? rejection = null;
        TaskCompletionSource? start = null;
        await gate.WaitAsync(cancellation);
        try
        {
            if (!records.TryGetValue(run.RunId, out report) || !report.Matches(run)) rejection = "run_identity_conflict";
            else if (report.ResumeRequestIds?.Contains(requestId, StringComparer.Ordinal) is true) { /* Durable duplicate: report only. */ }
            else if (records.Values.Any(record => record.ResumeRequestIds?.Contains(requestId, StringComparer.Ordinal) is true)) rejection = "run_identity_conflict";
            else if ((report.ResumeRequestIds?.Length ?? 0) >= MaximumResumeRequests) rejection = "resume_limit_reached";
            else if (!PreSubmitRetryable(report)) rejection = "resume_not_allowed";
            else
            {
                report = report with { Phase = "preparing", Reason = null, Stage = "prepare", ExceptionType = null,
                    ExceptionTypes = null, FailureMethod = null, ResumeRequestIds = [.. report.ResumeRequestIds ?? [], requestId] };
                Save(report); // Consume the explicit request before any UI operation. Never replay it after a restart.
                var control = new RunningControl(run.RunId, CancellationTokenSource.CreateLinkedTokenSource(lifetime.Token));
                Interlocked.Exchange(ref runningControl, control)?.Cancellation.Dispose();
                start = new(TaskCreationOptions.RunContinuationsAsynchronously);
                var begin = start.Task;
                running = Task.Run(async () => { await begin; await ExecuteAsync(run, control.Cancellation.Token); });
                _ = ReportAfterWorkerAsync(running, run.RunId);
            }
        }
        finally { gate.Release(); }
        try
        {
            if (rejection is not null) await send(ImageAutomationJson.Element(new { type = "resume_rejected", run_id = run.RunId, request_id = requestId, reason = rejection }), cancellation);
            if (report is not null) await ReportAsync(report, cancellation);
        }
        finally { start?.TrySetResult(); }
    }

    async Task ReportAfterWorkerAsync(Task worker, string id)
    {
        try
        {
            await worker;
            if (lifetime.IsCancellationRequested) return;
            ImageRunRecord? record;
            await gate.WaitAsync(lifetime.Token);
            try { records.TryGetValue(id, out record); }
            finally { gate.Release(); }
            if (record is { Phase: "paused", SubmitIntent: false }) await ReportAsync(record, lifetime.Token);
        }
        catch { /* A failed report never schedules work or changes the durable record. */ }
    }

    async Task StartAsync(ImageAutomationRun run, CancellationToken cancellation)
    {
        ImageRunRecord report;
        TaskCompletionSource? start = null;
        await gate.WaitAsync(cancellation);
        try
        {
            if (records.TryGetValue(run.RunId, out var existing))
            {
                if (!existing.Matches(run)) throw new ImageAutomationException("run_identity_conflict");
                report = existing; // Includes preparing/submitting/paused: repeated start is never executable.
            }
            else if (records.Values.Any(x => !x.Terminal) || !running.IsCompleted)
            {
                report = new(run.RunId, run.TaskId, run.BaseVersionId, run.PromptSha256, "failed", "adapter_busy");
                Save(report);
            }
            else
            {
                report = new(run.RunId, run.TaskId, run.BaseVersionId, run.PromptSha256, "preparing", SubmitIntent: false, Stage: "prepare");
                Save(report);
                var control = new RunningControl(run.RunId, CancellationTokenSource.CreateLinkedTokenSource(lifetime.Token));
                Interlocked.Exchange(ref runningControl, control)?.Cancellation.Dispose();
                start = new(TaskCreationOptions.RunContinuationsAsynchronously);
                var begin = start.Task;
                running = Task.Run(async () => { await begin; await ExecuteAsync(run, control.Cancellation.Token); });
                _ = ReportAfterWorkerAsync(running, run.RunId);
            }
        }
        finally { gate.Release(); }
        try { await ReportAsync(report, cancellation); }
        finally { start?.TrySetResult(); }
    }

    async Task ExecuteAsync(ImageAutomationRun run, CancellationToken cancellation)
    {
        PreparedImageConversation? prepared = null;
        var intentWritten = false;
        try
        {
            cancellation.ThrowIfCancellationRequested();
            await SetStageAsync(run.RunId, "prepare", cancellation);
            prepared = await ObserveMethodAsync("PrepareAsync", () => surface.PrepareAsync(run, cancellation));
            await SetStageAsync(run.RunId, "write", cancellation);
            await ObserveMethodAsync("WriteAndVerifyPromptAsync", () => surface.WriteAndVerifyPromptAsync(prepared, run.Prompt, cancellation));
            await SetStageAsync(run.RunId, "submit", cancellation);
            await ChangeAsync(run.RunId, "submitting", null, cancellation, prepared);
            intentWritten = true;
            await gate.WaitAsync(cancellation);
            try
            {
                cancellation.ThrowIfCancellationRequested();
                if (records[run.RunId].Terminal) throw new OperationCanceledException(cancellation);
                // Submission and the durable cancel transition have one ordering. Cancel never completes and then allows a click.
                await ObserveMethodAsync("SubmitOnceAsync", () => surface.SubmitOnceAsync(prepared, cancellation));
            }
            finally { gate.Release(); }
            await ChangeAsync(run.RunId, "generating", null, cancellation, prepared);
            await CaptureAsync(run.RunId, prepared, cancellation);
        }
        catch (Exception error)
        {
            var reason = error is ImageAutomationException specific ? specific.Reason
                : error is OperationCanceledException ? (lifetime.IsCancellationRequested ? "adapter_restarted" : "cancelled")
                : intentWritten ? "submission_uncertain" : "automation_unavailable";
            await FailSafelyAsync(run.RunId, reason, prepared, error);
        }
        finally
        {
            if (prepared is not null)
            {
                using var cleanup = new CancellationTokenSource(TimeSpan.FromSeconds(6));
                try { await surface.ClearOwnedDraftAsync(prepared, run.Prompt, cleanup.Token); } catch { }
                // A failed pre-submit write already retains its owned binding and
                // cannot be retried. Preserve the final cleanup acknowledgement too;
                // an empty early snapshot must not erase evidence of an in-flight write.
                await gate.WaitAsync();
                try
                {
                    if (records.TryGetValue(run.RunId, out var record) && record.SubmitIntent is false
                        && surface.ExportBinding(prepared) is { } binding)
                        Save(record with { SurfaceBinding = binding });
                }
                finally { gate.Release(); }
            }
        }
    }

    async Task RecoverAsync(ImageRunRecord run, CancellationToken cancellation)
    {
        PreparedImageConversation? prepared = null;
        try
        {
            prepared = await surface.RestoreSubmittedAsync(run, run.SurfaceBinding!.Value, cancellation)
                ?? throw new ImageAutomationException("submission_uncertain");
            await ChangeAsync(run.RunId, "generating", null, cancellation, prepared);
            await CaptureAsync(run.RunId, prepared, cancellation);
        }
        catch (Exception error)
        {
            var reason = error is ImageAutomationException known ? known.Reason
                : error is OperationCanceledException ? (lifetime.IsCancellationRequested ? "adapter_restarted" : "cancelled")
                : "download_failed";
            await FailSafelyAsync(run.RunId, reason, prepared, error);
        }
    }

    async Task CaptureAsync(string id, PreparedImageConversation prepared, CancellationToken cancellation)
    {
        await SetStageAsync(id, "wait", cancellation);
        await ObserveMethodAsync("WaitForGeneratedImageAsync", () => surface.WaitForGeneratedImageAsync(prepared, cancellation));
        await ChangeAsync(id, "downloading", null, cancellation, prepared);
        var downloadDirectory = Path.Combine(directory, "downloads", id);
        Directory.CreateDirectory(downloadDirectory);
        EnsureOrdinaryPath(downloadDirectory);
        await SetStageAsync(id, "download", cancellation);
        var download = await ObserveMethodAsync("DownloadOriginalAsync", () => surface.DownloadOriginalAsync(prepared, downloadDirectory, cancellation));
        var full = Path.GetFullPath(download);
        if (!full.StartsWith(downloadDirectory + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase))
            throw new ImageAutomationException("artifact_invalid");
        EnsureOrdinaryPath(full);
        var info = new FileInfo(full);
        if (info.Length is <= 0 or > MaximumImageBytes) throw new ImageAutomationException("artifact_invalid");
        var bytes = await ReadBoundedImageAsync(full, cancellation);
        var mime = MimeType(bytes) ?? throw new ImageAutomationException("artifact_invalid");
        var hash = Convert.ToHexStringLower(SHA256.HashData(bytes));
        ImageRunRecord staged;
        await gate.WaitAsync(cancellation);
        try
        {
            cancellation.ThrowIfCancellationRequested();
            staged = records[id];
            if (staged.Terminal) return;
            AtomicWrite(ArtifactPath(id), bytes);
            staged = staged with { Phase = "awaiting_import", Reason = null, Sha256 = hash, MimeType = mime, Stage = "report" };
            Save(staged);
        }
        finally { gate.Release(); }
        try { File.Delete(full); }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException) { /* Durable staged bytes already exist. */ }
        await ReportAsync(staged, lifetime.Token);
    }

    async Task ChangeAsync(string id, string phase, string? reason, CancellationToken cancellation, PreparedImageConversation? prepared = null)
    {
        ImageRunRecord record;
        await gate.WaitAsync(cancellation);
        try
        {
            cancellation.ThrowIfCancellationRequested();
            record = records[id];
            if (record.Terminal) throw new OperationCanceledException(cancellation);
            record = record with { Phase = phase, Reason = reason, SubmitIntent = phase == "submitting" ? true : record.SubmitIntent,
                SurfaceBinding = prepared is null ? record.SurfaceBinding : surface.ExportBinding(prepared) ?? record.SurfaceBinding };
            Save(record);
        }
        finally { gate.Release(); }
        await ReportAsync(record, cancellation);
    }

    async Task SetStageAsync(string id, string stage, CancellationToken cancellation)
    {
        await gate.WaitAsync(cancellation);
        try
        {
            cancellation.ThrowIfCancellationRequested();
            var record = records[id];
            if (record.Terminal) throw new OperationCanceledException(cancellation);
            Save(record with { Stage = stage });
        }
        finally { gate.Release(); }
    }

    static string[] ExceptionTypes(Exception error)
    {
        var types = new List<string>();
        for (Exception? current = error; current is not null && types.Count < 3; current = current.InnerException)
        {
            var name = current.GetType().Name;
            types.Add(name.Length <= 80 && name.All(c => char.IsAsciiLetterOrDigit(c) || c == '_') ? name : "Exception");
        }
        return types.ToArray();
    }

    static async Task<T> ObserveMethodAsync<T>(string method, Func<Task<T>> action)
    {
        try { return await action(); }
        catch (Exception error) { error.Data["EntwurfFailureMethod"] = method; throw; }
    }
    static async Task ObserveMethodAsync(string method, Func<Task> action)
    {
        try { await action(); }
        catch (Exception error) { error.Data["EntwurfFailureMethod"] = method; throw; }
    }
    static string? FailureMethod(Exception error) => (error.Data["EntwurfFailureMethod"] as string) switch
    {
        "PrepareAsync" => "PrepareAsync", "WriteAndVerifyPromptAsync" => "WriteAndVerifyPromptAsync",
        "SubmitOnceAsync" => "SubmitOnceAsync", "WaitForGeneratedImageAsync" => "WaitForGeneratedImageAsync",
        "DownloadOriginalAsync" => "DownloadOriginalAsync", "ReportAsync" => "ReportAsync", "Persist" => "Persist", _ => null
    };

    async Task FailSafelyAsync(string id, string reason, PreparedImageConversation? prepared, Exception error)
    {
        ImageRunRecord? report = null;
        await gate.WaitAsync();
        try
        {
            if (records.TryGetValue(id, out var record) && !record.Terminal)
            {
                var stage = error.Data["EntwurfStage"] as string == "report" ? "report" : record.Stage ?? "prepare";
                var types = ExceptionTypes(error);
                var diagnostics = record with { Stage = stage, ExceptionType = types[0], ExceptionTypes = types, FailureMethod = FailureMethod(error) };
                // Reporting/cleanup failure cannot turn a verified staged original back into a UI retry.
                report = record.Phase == "awaiting_import" && ArtifactMatches(record) ? diagnostics
                    : diagnostics with { Phase = reason == "cancelled" ? "cancelled" : "paused", Reason = reason,
                        SurfaceBinding = prepared is null ? record.SurfaceBinding : surface.ExportBinding(prepared) ?? record.SurfaceBinding };
                try { Save(report); }
                catch (Exception storageError) when (storageError is IOException or UnauthorizedAccessException) { /* Existing durable intent still prevents replay. */ }
            }
        }
        finally { gate.Release(); }
        if (report is not null) try { await ReportAsync(report, CancellationToken.None); } catch { }
    }

    async Task ProbeAsync(CancellationToken cancellation)
    {
        var probe = await surface.ProbeAsync(cancellation);
        await send(ImageAutomationJson.Element(new { type = "probe", adapter = "chatgpt-windows", available = probe.Available, reason = probe.Reason }), cancellation);
    }

    async Task ReportAsync(ImageRunRecord record, CancellationToken cancellation)
    {
        await reports.WaitAsync(cancellation);
        try
        {
            await gate.WaitAsync(cancellation);
            bool retryable;
            try { record = records[record.RunId]; retryable = PreSubmitRetryable(record); }
            finally { gate.Release(); }
            await ReportCurrentAsync(record, retryable, cancellation);
        }
        catch (Exception error) { error.Data["EntwurfStage"] = "report"; error.Data["EntwurfFailureMethod"] = "ReportAsync"; throw; }
        finally { reports.Release(); }
    }
    async Task ReportCurrentAsync(ImageRunRecord record, bool retryable, CancellationToken cancellation)
    {
        await send(ImageAutomationJson.Element(new { type = "status", run_id = record.RunId, phase = record.Phase, detail = record.Reason,
            pre_submit_retryable = retryable, resume_request_id = record.ResumeRequestIds?.LastOrDefault(), stage = record.Stage,
            exception_type = record.ExceptionType, exception_types = record.ExceptionTypes, failure_method = record.FailureMethod }), cancellation);
        if (record.Phase != "awaiting_import") return;
        byte[] bytes;
        await gate.WaitAsync(cancellation);
        try
        {
            if (!records.TryGetValue(record.RunId, out var current) || current.Phase != "awaiting_import") return;
            if (!ArtifactMatches(current)) throw new ImageAutomationException("artifact_invalid");
            bytes = await ReadBoundedImageAsync(ArtifactPath(record.RunId), cancellation);
        }
        finally { gate.Release(); }
        await send(ImageAutomationJson.Element(new
        {
            type = "artifact", run_id = record.RunId, task_id = record.TaskId, base_version_id = record.BaseVersionId,
            mime_type = record.MimeType, bytes_base64 = Convert.ToBase64String(bytes), sha256 = record.Sha256
        }), cancellation);
    }

    void Save(ImageRunRecord record)
    {
        var next = new Dictionary<string, ImageRunRecord>(records, StringComparer.Ordinal) { [record.RunId] = record };
        try { Persist(next); }
        catch (Exception error) { error.Data["EntwurfFailureMethod"] = "Persist"; throw; }
        records = next;
    }
    void Persist(Dictionary<string, ImageRunRecord> next) => AtomicWrite(Path.Combine(directory, "runs.json"), JsonSerializer.SerializeToUtf8Bytes(next, ImageAutomationJson.Options));
    string ArtifactPath(string id) => Path.Combine(directory, id + ".image");
    bool ArtifactMatches(ImageRunRecord record)
    {
        try
        {
            var path = ArtifactPath(record.RunId);
            EnsureOrdinaryPath(path);
            var length = new FileInfo(path).Length;
            if (length is <= 0 or > MaximumImageBytes) return false;
            using var file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read);
            if (file.Length is <= 0 or > MaximumImageBytes) return false;
            var bytes = new byte[(int)file.Length];
            file.ReadExactly(bytes);
            if (file.ReadByte() != -1) return false;
            return record.MimeType == MimeType(bytes) && record.Sha256 == Convert.ToHexStringLower(SHA256.HashData(bytes));
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException) { return false; }
    }
    void DeleteOwnedArtifact(ImageRunRecord record)
    {
        try { File.Delete(ArtifactPath(record.RunId)); } catch (IOException) { }
    }
    internal static string? MimeType(ReadOnlySpan<byte> bytes) => bytes.StartsWith(new byte[] { 137, 80, 78, 71, 13, 10, 26, 10 }) ? "image/png"
        : bytes.Length >= 3 && bytes[0] == 255 && bytes[1] == 216 && bytes[2] == 255 ? "image/jpeg"
        : bytes.Length >= 12 && bytes[..4].SequenceEqual("RIFF"u8) && bytes.Slice(8, 4).SequenceEqual("WEBP"u8) ? "image/webp" : null;
    internal static void EnsureOrdinaryPath(string path)
    {
        for (var current = Path.GetFullPath(path); !string.IsNullOrEmpty(current); current = Path.GetDirectoryName(current))
            if (File.Exists(current) || Directory.Exists(current))
                if ((File.GetAttributes(current) & FileAttributes.ReparsePoint) != 0) throw new IOException("reparse_path");
    }
    static async Task<byte[]> ReadBoundedImageAsync(string path, CancellationToken cancellation)
    {
        using var file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read);
        if (file.Length is <= 0 or > MaximumImageBytes) throw new ImageAutomationException("artifact_invalid");
        var bytes = new byte[(int)file.Length];
        await file.ReadExactlyAsync(bytes, cancellation);
        if (file.ReadByte() != -1) throw new ImageAutomationException("artifact_invalid");
        return bytes;
    }
    static void AtomicWrite(string path, byte[] bytes)
    {
        EnsureOrdinaryPath(path);
        var temporary = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            using (var file = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None)) { file.Write(bytes); file.Flush(true); }
            File.Move(temporary, path, true);
        }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
    }

    public async ValueTask DisposeAsync()
    {
        lifetime.Cancel();
        runningControl?.Cancellation.Cancel();
        try { await running.WaitAsync(TimeSpan.FromSeconds(7)); } catch (TimeoutException) { }
        // A stalled UIA provider can retain the surface lock even after cancellation. Keep disposal
        // under that lock, but let this dedicated adapter process exit and reclaim it if cleanup stalls.
        try { await Task.Run(surface.Dispose).WaitAsync(TimeSpan.FromSeconds(1)); }
        catch (TimeoutException) { }
        runningControl?.Cancellation.Dispose();
        lifetime.Dispose();
        ownership.Dispose();
    }
}
