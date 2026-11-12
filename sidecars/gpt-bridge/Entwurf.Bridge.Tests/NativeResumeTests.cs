using System.Collections.Concurrent;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativeResumeTests
{
    const string PrivateMessage = "never-retain-private-prompt-or-UI-content";
    static readonly byte[] Png = Convert.FromBase64String("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6kVwAAAAASUVORK5CYII=");
    static ImageAutomationRun Run(string id = "same-run") => new(id, "task", "version", "public fixture image", Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes("public fixture image"))));
    static JsonElement Start(ImageAutomationRun run) => ImageAutomationJson.Element(new { type = "start", run });
    static JsonElement Resume(ImageAutomationRun run, string request) => ImageAutomationJson.Element(new { type = "resume_pre_submit", request_id = request, run });
    static JsonElement Status(string id) => ImageAutomationJson.Element(new { type = "status", run_id = id });

    [Theory]
    [InlineData("prepare")]
    [InlineData("write")]
    [InlineData("submit")]
    [InlineData("wait")]
    [InlineData("download")]
    [InlineData("report")]
    public async Task Failure_retains_only_fixed_stage_and_exception_types(string stage)
    {
        using var fixture = new Fixture();
        var surface = new Surface { Fail = stage };
        if (stage == "report") fixture.FailArtifactReport = true;
        await using var session = fixture.Session(surface);
        await session.HandleAsync(Start(Run()), default);
        await fixture.Until(() => fixture.Messages.Any(x => x.TryGetProperty("exception_type", out _)));
        var record = fixture.Record();
        Assert.Equal(stage, record.GetProperty("stage").GetString());
        Assert.Equal("InvalidOperationException", record.GetProperty("exception_type").GetString());
        Assert.Equal(new[] { "InvalidOperationException", "IOException" }, record.GetProperty("exception_types").EnumerateArray().Select(x => x.GetString()));
        Assert.DoesNotContain(PrivateMessage, File.ReadAllText(fixture.Database));
        Assert.DoesNotContain(Run().Prompt, File.ReadAllText(fixture.Database));
        if (stage == "report") { Assert.Equal("awaiting_import", record.GetProperty("phase").GetString()); Assert.Equal(Png, await File.ReadAllBytesAsync(fixture.Artifact)); }
    }

    [Fact]
    public async Task Explicit_resume_consumes_request_before_prepare_and_submits_the_existing_run_once()
    {
        using var fixture = new Fixture(); fixture.Seed();
        var request = Guid.NewGuid().ToString();
        var surface = new Surface { BeforePrepare = () => Assert.Contains(request, fixture.Record().GetProperty("resume_request_ids").EnumerateArray().Select(x => x.GetString())) };
        await using var session = fixture.Session(surface);
        await session.HandleAsync(Resume(Run(), request), default);
        await fixture.Until(() => fixture.Messages.Any(x => x.TryGetProperty("phase", out var phase) && phase.GetString() == "awaiting_import"));
        await session.HandleAsync(Resume(Run(), request.ToUpperInvariant()), default);
        Assert.Equal(1, surface.Prepares); Assert.Equal(1, surface.Submissions);
        Assert.Equal(request, fixture.Messages.Last(x => x.GetProperty("type").GetString() == "status").GetProperty("resume_request_id").GetString());
        Assert.Equal("same-run", fixture.Record().GetProperty("run_id").GetString());
    }

    [Theory]
    [InlineData("write_unconfirmed")]
    [InlineData("clear_unconfirmed")]
    [InlineData("cleared")]
    public async Task A_failed_write_preserves_final_cleanup_binding_and_cannot_replay_after_restart(string cleanupState)
    {
        using var fixture = new Fixture();
        var surface = new Surface
        {
            Fail = "write",
            Binding = ImageAutomationJson.Element(new { window = 20, document_id = "owned", draft_cleanup_state = "write_unconfirmed" }),
            BeforeCleanup = () => { },
            Cleanup = cleanupState == "cleared" ? Task.CompletedTask : Task.FromException(new ImageAutomationException("image_surface_unavailable"))
        };
        surface.BeforeCleanup = () => surface.Binding = ImageAutomationJson.Element(new { window = 20, document_id = "owned", draft_cleanup_state = cleanupState });
        await using (var session = fixture.Session(surface))
        {
            await session.HandleAsync(Start(Run()), default);
            await fixture.Until(() => fixture.Messages.Any(x => x.TryGetProperty("exception_type", out _)));
        }
        var saved = fixture.Record();
        Assert.False(saved.GetProperty("submit_intent").GetBoolean());
        Assert.Equal(cleanupState, saved.GetProperty("surface_binding").GetProperty("draft_cleanup_state").GetString());
        Assert.DoesNotContain(Run().Prompt, File.ReadAllText(fixture.Database));
        var restartedSurface = new Surface();
        await using var restarted = fixture.Session(restartedSurface);
        await restarted.HandleAsync(Status("same-run"), default);
        Assert.False(fixture.Messages.Last(x => x.GetProperty("type").GetString() == "status").GetProperty("pre_submit_retryable").GetBoolean());
        await restarted.HandleAsync(Resume(Run(), Guid.NewGuid().ToString()), default);
        Assert.Equal(0, restartedSurface.Prepares);
        Assert.Equal(0, restartedSurface.Submissions);
    }

    [Fact]
    public async Task Consumed_resume_is_not_executable_after_failure_restart_start_status_or_reconnect()
    {
        using var fixture = new Fixture(); fixture.Seed(); var request = Guid.NewGuid().ToString();
        await using (var first = fixture.Session(new Surface { Fail = "prepare" }))
        {
            await first.HandleAsync(Resume(Run(), request), default);
            await fixture.Until(() => fixture.Messages.Any(x => x.TryGetProperty("pre_submit_retryable", out var retryable) && retryable.GetBoolean()));
        }
        var surface = new Surface(); await using var restarted = fixture.Session(surface);
        await restarted.ConnectedAsync(default); await restarted.HandleAsync(Start(Run()), default);
        await restarted.HandleAsync(Status("same-run"), default); await restarted.HandleAsync(Resume(Run(), request), default);
        Assert.Equal(0, surface.Prepares); Assert.Equal(0, surface.Submissions);
        await restarted.HandleAsync(Resume(Run(), Guid.NewGuid().ToString()), default);
        await fixture.Until(() => fixture.Messages.Any(x => x.TryGetProperty("phase", out var phase) && phase.GetString() == "awaiting_import"));
        Assert.Equal(1, surface.Submissions);
    }

    [Theory]
    [InlineData("missing_intent")]
    [InlineData("null_intent")]
    [InlineData("submitted")]
    [InlineData("binding")]
    [InlineData("sha")]
    [InlineData("mime")]
    [InlineData("artifact")]
    [InlineData("download")]
    [InlineData("terminal")]
    [InlineData("other_run")]
    [InlineData("limit")]
    public async Task Ambiguous_or_ineligible_state_never_prepares_or_consumes_a_request(string condition)
    {
        using var fixture = new Fixture(); fixture.Seed(condition);
        var surface = new Surface();
        await using var session = fixture.Session(surface);
        var persisted = File.ReadAllBytes(fixture.Database);
        await session.HandleAsync(Resume(Run(), Guid.NewGuid().ToString()), default);
        Assert.Equal(0, surface.Prepares); Assert.Equal(0, surface.Submissions);
        Assert.Contains(fixture.Messages, x => x.GetProperty("type").GetString() == "resume_rejected");
        Assert.Equal(persisted, File.ReadAllBytes(fixture.Database));
        await session.HandleAsync(Status("same-run"), default);
        Assert.False(fixture.Messages.Last(x => x.GetProperty("type").GetString() == "status").GetProperty("pre_submit_retryable").GetBoolean());
    }

    [Fact]
    public async Task Identity_mismatch_and_unknown_run_never_modify_the_saved_run()
    {
        using var fixture = new Fixture(); fixture.Seed(); var surface = new Surface(); await using var session = fixture.Session(surface);
        var before = File.ReadAllBytes(fixture.Database);
        await session.HandleAsync(Resume(Run() with { TaskId = "different-task" }, Guid.NewGuid().ToString()), default);
        await session.HandleAsync(Resume(Run("unknown"), Guid.NewGuid().ToString()), default);
        Assert.Equal(0, surface.Prepares); Assert.Equal(before, File.ReadAllBytes(fixture.Database));
        Assert.Equal(2, fixture.Messages.Count(x => x.GetProperty("type").GetString() == "resume_rejected"));
    }

    [Fact]
    public async Task A_worker_that_has_reported_failure_but_not_finished_cleanup_is_not_retryable()
    {
        using var fixture = new Fixture(); fixture.Seed();
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var surface = new Surface { Fail = "write", Cleanup = release.Task };
        await using var session = fixture.Session(surface);
        await session.HandleAsync(Resume(Run(), Guid.NewGuid().ToString()), default);
        await fixture.Until(() => fixture.Messages.Any(x => x.TryGetProperty("phase", out var phase) && phase.GetString() == "paused" && x.TryGetProperty("exception_type", out _)));
        try
        {
            await session.HandleAsync(Status("same-run"), default);
            Assert.False(fixture.Messages.Last(x => x.GetProperty("type").GetString() == "status").GetProperty("pre_submit_retryable").GetBoolean());
            await session.HandleAsync(Resume(Run(), Guid.NewGuid().ToString()), default);
            Assert.Equal(1, surface.Prepares);
        }
        finally { release.TrySetResult(); }
        await fixture.Until(() => fixture.Messages.Any(x => x.TryGetProperty("pre_submit_retryable", out var value) && value.GetBoolean()));
    }

    [Fact]
    public async Task Failed_durable_receipt_does_not_execute_or_consume_the_request()
    {
        using var fixture = new Fixture(); fixture.Seed(); var surface = new Surface();
        await using var session = fixture.Session(surface);
        var before = File.ReadAllBytes(fixture.Database); var request = Guid.NewGuid().ToString();
        File.Move(fixture.Database, fixture.Database + ".preserved");
        Directory.CreateDirectory(fixture.Database);
        try
        {
            var error = await Record.ExceptionAsync(() => session.HandleAsync(Resume(Run(), request), default));
            Assert.True(error is IOException or UnauthorizedAccessException);
            Assert.Equal(0, surface.Prepares); Assert.Equal(0, surface.Submissions);
        }
        finally { Directory.Delete(fixture.Database); File.Move(fixture.Database + ".preserved", fixture.Database); }
        Assert.Equal(before, File.ReadAllBytes(fixture.Database));
        await session.HandleAsync(Resume(Run(), request), default);
        await fixture.Until(() => fixture.Messages.Any(x => x.TryGetProperty("phase", out var phase) && phase.GetString() == "awaiting_import"));
        Assert.Equal(1, surface.Submissions);
    }

    [Fact]
    public async Task Invalid_or_cross_run_request_ids_never_execute_or_modify_history()
    {
        const string request = "11111111-1111-4111-8111-111111111111";
        using var fixture = new Fixture(); fixture.Seed("request_elsewhere"); var surface = new Surface();
        await using var session = fixture.Session(surface); var before = File.ReadAllBytes(fixture.Database);
        await Assert.ThrowsAsync<ImageAutomationException>(() => session.HandleAsync(Resume(Run(), "not-a-uuid"), default));
        await session.HandleAsync(Resume(Run(), request), default);
        Assert.Equal("run_identity_conflict", fixture.Messages.First(x => x.GetProperty("type").GetString() == "resume_rejected").GetProperty("reason").GetString());
        Assert.Equal(0, surface.Prepares); Assert.Equal(before, File.ReadAllBytes(fixture.Database));
    }

    [Fact]
    public async Task Cancellation_during_the_accepted_receipt_report_prevents_any_resumed_UI_operation()
    {
        using var fixture = new Fixture(); fixture.Seed(); var surface = new Surface();
        var reported = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        fixture.OnSend = async message =>
        {
            if (message.TryGetProperty("phase", out var phase) && phase.GetString() == "preparing")
            { reported.TrySetResult(); await release.Task; }
        };
        var session = fixture.Session(surface);
        try
        {
            var resume = session.HandleAsync(Resume(Run(), Guid.NewGuid().ToString()), default);
            await reported.Task.WaitAsync(TimeSpan.FromSeconds(2));
            var cancel = session.HandleAsync(ImageAutomationJson.Element(new { type = "cancel", run_id = "same-run" }), default);
            Assert.Equal("cancelled", fixture.Record().GetProperty("phase").GetString());
            release.TrySetResult(); await Task.WhenAll(resume, cancel).WaitAsync(TimeSpan.FromSeconds(2));
        }
        finally { release.TrySetResult(); await session.DisposeAsync(); }
        Assert.Equal(0, surface.Prepares); Assert.Equal(0, surface.Submissions);
        Assert.Equal("cancelled", fixture.Record().GetProperty("phase").GetString());
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task Cancelling_or_disposing_a_resumed_worker_during_prompt_verification_never_submits(bool dispose)
    {
        using var fixture = new Fixture(); fixture.Seed();
        var hold = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var surface = new Surface { HoldWrite = hold.Task };
        var session = fixture.Session(surface);
        try
        {
            await session.HandleAsync(Resume(Run(), Guid.NewGuid().ToString()), default);
            await surface.WriteEntered.Task.WaitAsync(TimeSpan.FromSeconds(2));
            if (!dispose) await session.HandleAsync(ImageAutomationJson.Element(new { type = "cancel", run_id = "same-run" }), default);
        }
        finally { await session.DisposeAsync(); }
        Assert.Equal(1, surface.Prepares); Assert.Equal(0, surface.Submissions);
        Assert.False(fixture.Record().GetProperty("submit_intent").GetBoolean());
        Assert.DoesNotContain(fixture.Messages, x => x.TryGetProperty("pre_submit_retryable", out var retryable) && retryable.GetBoolean());
    }

    sealed class Fixture : IDisposable
    {
        public string Directory { get; } = Path.Combine(Path.GetTempPath(), "entwurf-resume-" + Guid.NewGuid());
        public string Database => Path.Combine(Directory, "runs.json");
        public string Artifact => Path.Combine(Directory, "same-run.image");
        public ConcurrentQueue<JsonElement> Messages { get; } = new();
        public bool FailArtifactReport;
        public Func<JsonElement, Task>? OnSend;
        public ImageAutomationSession Session(Surface surface) => new(Directory, surface, (message, _) => { Messages.Enqueue(message.Clone()); if (FailArtifactReport && message.GetProperty("type").GetString() == "artifact") throw Failure(); return OnSend?.Invoke(message) ?? Task.CompletedTask; });
        public JsonElement Record() { using var stream = new FileStream(Database, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete); using var json = JsonDocument.Parse(stream); return json.RootElement.GetProperty("same-run").Clone(); }
        public async Task Until(Func<bool> condition) { using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(4)); try { while (!condition()) await Task.Delay(10, deadline.Token); } catch (OperationCanceledException) { throw new Xunit.Sdk.XunitException("Timed out; durable fixture record: " + Record().GetRawText()); } }
        public void Seed(string? condition = null)
        {
            System.IO.Directory.CreateDirectory(Directory);
            var record = new Dictionary<string, object?> { ["run_id"] = "same-run", ["task_id"] = "task", ["base_version_id"] = "version", ["prompt_sha256"] = Run().PromptSha256, ["phase"] = condition == "terminal" ? "cancelled" : "paused", ["reason"] = "automation_unavailable", ["submit_intent"] = false };
            if (condition == "missing_intent") record.Remove("submit_intent");
            if (condition == "null_intent") record["submit_intent"] = null;
            if (condition == "submitted") record["submit_intent"] = true;
            if (condition == "binding") record["surface_binding"] = new { owned = true };
            if (condition == "sha") record["sha256"] = new string('a', 64);
            if (condition == "mime") record["mime_type"] = "image/png";
            if (condition == "artifact") File.WriteAllBytes(Artifact, Png);
            if (condition == "download") { var path = Path.Combine(Directory, "downloads", "same-run"); System.IO.Directory.CreateDirectory(path); File.WriteAllBytes(Path.Combine(path, "original.png"), Png); }
            if (condition == "limit") record["resume_request_ids"] = Enumerable.Range(0, 64).Select(_ => Guid.NewGuid().ToString()).ToArray();
            var records = new Dictionary<string, object?> { ["same-run"] = record };
            if (condition == "other_run") records["other-run"] = new { run_id = "other-run", task_id = "task", base_version_id = "version", prompt_sha256 = Run().PromptSha256, phase = "paused", submit_intent = false };
            if (condition == "request_elsewhere") records["other-run"] = new { run_id = "other-run", task_id = "task", base_version_id = "version", prompt_sha256 = Run().PromptSha256, phase = "cancelled", submit_intent = false, resume_request_ids = new[] { "11111111-1111-4111-8111-111111111111" } };
            File.WriteAllBytes(Database, JsonSerializer.SerializeToUtf8Bytes(records));
        }
        public void Dispose() { if (System.IO.Directory.Exists(Directory)) System.IO.Directory.Delete(Directory, true); }
    }
    static Exception Failure() => new InvalidOperationException(PrivateMessage, new IOException(PrivateMessage));
    sealed class Conversation : PreparedImageConversation;
    sealed class Surface : IImageAutomationSurface
    {
        public Task HoldWrite = Task.CompletedTask;
        public TaskCompletionSource WriteEntered { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public string? Fail; public int Prepares; public int Submissions; public Action? BeforePrepare; public Task Cleanup = Task.CompletedTask;
        public JsonElement? Binding; public Action? BeforeCleanup;
        void Check(string stage) { if (Fail == stage) throw Failure(); }
        public Task<ImageSurfaceProbe> ProbeAsync(CancellationToken token) => Task.FromResult(new ImageSurfaceProbe(true, "ready"));
        public Task<PreparedImageConversation> PrepareAsync(ImageAutomationRun run, CancellationToken token) { token.ThrowIfCancellationRequested(); Interlocked.Increment(ref Prepares); BeforePrepare?.Invoke(); Check("prepare"); return Task.FromResult<PreparedImageConversation>(new Conversation()); }
        public async Task WriteAndVerifyPromptAsync(PreparedImageConversation conversation, string prompt, CancellationToken token) { WriteEntered.TrySetResult(); await HoldWrite.WaitAsync(token); Check("write"); }
        public Task SubmitOnceAsync(PreparedImageConversation conversation, CancellationToken token) { token.ThrowIfCancellationRequested(); Interlocked.Increment(ref Submissions); Check("submit"); return Task.CompletedTask; }
        public Task WaitForGeneratedImageAsync(PreparedImageConversation conversation, CancellationToken token) { Check("wait"); return Task.CompletedTask; }
        public Task<string> DownloadOriginalAsync(PreparedImageConversation conversation, string directory, CancellationToken token) { Check("download"); var path = Path.Combine(directory, "original.png"); File.WriteAllBytes(path, Png); return Task.FromResult(path); }
        public Task ClearOwnedDraftAsync(PreparedImageConversation conversation, string prompt, CancellationToken token) { BeforeCleanup?.Invoke(); return Cleanup.WaitAsync(token); }
        public JsonElement? ExportBinding(PreparedImageConversation conversation) => Binding;
        public void Dispose() { }
    }
}
