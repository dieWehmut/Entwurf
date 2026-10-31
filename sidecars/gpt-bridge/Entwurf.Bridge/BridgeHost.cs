using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace Entwurf.Bridge;

/// <summary>One connection, one consumed bootstrap secret, and at most one foreground task.</summary>
public sealed class BridgeHost : IDisposable
{
    readonly IWindowsAutomation automation;
    readonly ArtifactWatcher artifacts;
    readonly object gate = new();
    readonly byte[] tokenHash;
    readonly CancellationTokenSource lifetime = new();
    CancellationTokenSource taskCancellation = new();
    readonly Task watchdog;
    bool authorized, paused, operationRunning, disposed;
    Task<CapabilityReport>? discovery;
    string? taskId;
    int revision;
    string state = "idle";

    public BridgeHost(IWindowsAutomation automation, TextWriter log, string? tokenOverride = null, string[]? artifactDirectories = null)
    {
        this.automation = automation;
        tokenHash = SHA256.HashData(Encoding.UTF8.GetBytes(tokenOverride ?? Convert.ToHexString(RandomNumberGenerator.GetBytes(32))));
        artifacts = new ArtifactWatcher(artifactDirectories ?? []);
        watchdog = MonitorAsync();
    }

    public bool Authorized { get { lock (gate) return authorized; } }
    public BridgeResponse Handle(BridgeRequest request) => HandleAsync(request, default).GetAwaiter().GetResult();

    public async Task<BridgeResponse> HandleAsync(BridgeRequest request, CancellationToken cancellationToken)
    {
        CancellationToken activeToken;
        lock (gate)
        {
            ObjectDisposedException.ThrowIf(disposed, this);
            if (request.Method == "attach")
            {
                if (authorized) throw Error("already_attached", "The bootstrap token has already been consumed.");
                if (request.Token is null || request.Token.Length > 256 || !CryptographicOperations.FixedTimeEquals(tokenHash,
                    SHA256.HashData(Encoding.UTF8.GetBytes(request.Token)))) throw Error("unauthorized", "Attach with the parent bootstrap token.");
                authorized = true;
                CryptographicOperations.ZeroMemory(tokenHash);
                return Ok(request, new { attached = true });
            }
            if (request.Method != "detect_app" && !authorized) throw Error("unauthorized", "Attach before using this method.");
            if (request.Method is "pause" or "cancel")
            {
                taskCancellation.Cancel();
                paused = request.Method == "pause";
                state = paused ? "paused" : "cancelled";
                if (!paused)
                {
                    taskId = null;
                    taskCancellation.Dispose();
                    taskCancellation = new();
                }
                return Ok(request, new { state, external_generation_stopped = false });
            }
            if (operationRunning) throw Error("busy", "Another bridge operation is active; pause and cancel remain available.");
            if (paused && request.Method != "detect_app") throw Error("paused", "Cancel the paused task, then explicitly start a new task.");
            // Paused sessions may run a fresh read-only probe. Keep its wait cancellable
            // without clearing the paused state or re-enabling task mutations.
            if (request.Method == "detect_app" && taskCancellation.IsCancellationRequested)
            {
                taskCancellation.Dispose();
                taskCancellation = new();
            }
            operationRunning = true;
            activeToken = taskCancellation.Token;
        }
        using var linked = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken, activeToken, lifetime.Token);
        try
        {
            if (request.Method == "detect_app")
                return Ok(request, await DiscoverAsync(linked.Token));
            CapabilityReport? startCapabilities = null;
            if (request.Method == "start_conversation")
            {
                lock (gate)
                {
                    linked.Token.ThrowIfCancellationRequested();
                    ValidateStart(request);
                }
                // Provider calls run outside the state lock so the pipe can process pause/cancel.
                startCapabilities = await DiscoverAsync(linked.Token);
            }
            if (request.Method == "watch_artifact")
            {
                // Manual handoff can recover files without ever enabling UI automation.
                var id = ReadTaskId(request.Parameters);
                var rev = ReadInt(request.Parameters, "revision", 1, 1, int.MaxValue);
                var timeout = ReadInt(request.Parameters, "timeout_ms", 30_000, 1, 120_000);
                return Ok(request, await artifacts.WatchAsync(id, rev, TimeSpan.FromMilliseconds(timeout), linked.Token));
            }
            lock (gate)
            {
                linked.Token.ThrowIfCancellationRequested();
                switch (request.Method)
                {
                    case "start_conversation":
                        // Revalidate after awaiting discovery: cancellation may have changed task state.
                        ValidateStart(request);
                        var id = ReadTaskId(request.Parameters);
                        var rev = ReadInt(request.Parameters, "revision", 1, 1, int.MaxValue);
                        if (!startCapabilities!.AutomationEnabled) throw Error("automation_unavailable", "Four target-version gates have not passed; use manual handoff.");
                        RequireSafe();
                        automation.OpenChatGpt();
                        RequireSafe();
                        taskCancellation.Dispose();
                        taskCancellation = new();
                        taskId = id;
                        revision = rev;
                        state = "started";
                        return Ok(request, new { state, task_id = taskId, revision });
                    case "paste_prompt":
                        RequireState("started");
                        var text = ReadString(request.Parameters, "text");
                        if (string.IsNullOrWhiteSpace(text)) throw Error("empty_prompt", "Prompt cannot be empty.");
                        RequireSafe();
                        automation.Paste(text);
                        RequireSafe();
                        state = "prepared";
                        return Ok(request, new { state, characters = text.Length });
                    case "submit":
                        RequireState("prepared");
                        RequireSafe();
                        automation.Submit();
                        RequireSafe();
                        state = "waiting";
                        return Ok(request, new { state });
                    case "observe":
                        if (taskId is null) return Ok(request, new Observation(false, state, "No automated task is active; manual mode is available."));
                        RequireSafe();
                        var observation = automation.Observe();
                        if (observation.Complete) state = "complete";
                        return Ok(request, observation);
                    case "capture_text":
                        RequireState("complete");
                        RequireSafe();
                        var captured = automation.CaptureText();
                        if (string.IsNullOrWhiteSpace(captured)) throw Error("empty_result", "No accessible result text is available.");
                        RequireSafe();
                        return Ok(request, new { text = captured, task_id = taskId, revision });
                    default:
                        throw Error("unknown_method", "Unknown bridge method.");
                }
            }
        }
        catch (TimeoutException) { PauseOnFailure(); throw Error("timeout", "The operation timed out; use manual handoff."); }
        catch (OperationCanceledException) { throw Error("cancelled", "The operation was interrupted."); }
        finally { lock (gate) operationRunning = false; }
    }

    async Task<CapabilityReport> DiscoverAsync(CancellationToken token)
    {
        Task<CapabilityReport> probe;
        lock (gate)
        {
            token.ThrowIfCancellationRequested();
            // Reuse an underlying provider call even after its caller cancels or times out.
            // Only scheduling/cache access is locked; neither the provider nor its wait holds gate.
            if (discovery is null || discovery.IsCompleted) discovery = Task.Run(automation.Detect);
            probe = discovery;
        }
        return await probe.WaitAsync(TimeSpan.FromSeconds(8), token);
    }

    void ValidateStart(BridgeRequest request)
    {
        if (paused) throw Error("paused", "Cancel the paused task, then explicitly start a new task.");
        if (taskId is not null) throw Error("task_active", "Only one task may run at a time.");
        if (!ReadBool(request.Parameters, "explicit_start")) throw Error("explicit_start_required", "The user must explicitly start each task.");
        _ = ReadTaskId(request.Parameters);
        _ = ReadInt(request.Parameters, "revision", 1, 1, int.MaxValue);
    }

    async Task MonitorAsync()
    {
        try
        {
            using var timer = new PeriodicTimer(TimeSpan.FromMilliseconds(100));
            while (await timer.WaitForNextTickAsync(lifetime.Token))
                lock (gate)
                    if (taskId is not null && !paused)
                    {
                        try { if (!automation.CheckSafety().Safe) PauseOnFailure(); }
                        catch { PauseOnFailure(); }
                    }
        }
        catch (OperationCanceledException) { }
    }
    void RequireSafe()
    {
        if (automation.CheckSafety().Safe) return;
        PauseOnFailure();
        throw Error("foreground_lost", "The foreground, desktop lock, or target structure changed; task paused.");
    }
    void PauseOnFailure() { lock (gate) { paused = true; state = "paused"; taskCancellation.Cancel(); } }
    void RequireState(string required)
    {
        if (taskId is null) throw Error("no_active_task", "Explicitly start a task first; automatic mode requires all four gates.");
        if (state != required) throw Error("invalid_state", "This operation is not available in the current task state.");
    }
    static BridgeResponse Ok(BridgeRequest request, object value) => BridgeResponse.Succeeded(request.Id, value);
    static BridgeException Error(string code, string message) => new(code, message);
    static string ReadString(JsonElement? p, string name) => p is { ValueKind: JsonValueKind.Object } value && value.TryGetProperty(name, out var property) && property.ValueKind == JsonValueKind.String ? property.GetString()! : "";
    static bool ReadBool(JsonElement? p, string name) => p is { ValueKind: JsonValueKind.Object } value && value.TryGetProperty(name, out var property) && property.ValueKind == JsonValueKind.True;
    static string ReadTaskId(JsonElement? p)
    {
        var id = ReadString(p, "task_id");
        if (!Regex.IsMatch(id, "^[A-Za-z0-9_-]{1,80}$", RegexOptions.CultureInvariant)) throw Error("invalid_params", "task_id must be a safe nonempty identifier.");
        return id;
    }
    static int ReadInt(JsonElement? p, string name, int fallback, int min, int max)
    {
        if (p is not { ValueKind: JsonValueKind.Object } value || !value.TryGetProperty(name, out var property)) return fallback;
        if (property.ValueKind != JsonValueKind.Number || !property.TryGetInt32(out var result) || result < min || result > max) throw Error("invalid_params", $"Invalid {name}.");
        return result;
    }
    public void Dispose()
    {
        lock (gate)
        {
            if (disposed) return;
            disposed = true;
            lifetime.Cancel();
            taskCancellation.Cancel();
            CryptographicOperations.ZeroMemory(tokenHash);
        }
        watchdog.GetAwaiter().GetResult();
        lifetime.Dispose();
        taskCancellation.Dispose();
    }
}
