using System.Collections.Concurrent;
using System.Net;
using System.Net.Sockets;
using System.Net.WebSockets;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class ImageAutomationSessionTests
{
    static readonly byte[] Png = Convert.FromBase64String("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6kVwAAAAASUVORK5CYII=");

    [Fact]
    public async Task Blocked_surface_cleanup_is_bounded_and_preserves_staged_artifact_without_resubmission()
    {
        using var fixture = new Fixture();
        using var release = new ManualResetEventSlim();
        var entered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var finished = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var surface = new Surface { OnDispose = () => { entered.SetResult(); release.Wait(); finished.SetResult(); } };
        var session = fixture.Session(surface);
        await session.HandleAsync(Start("bounded-cleanup"), default);
        await fixture.WaitPhase("bounded-cleanup", "awaiting_import");
        var database = await File.ReadAllBytesAsync(Path.Combine(fixture.Directory, "runs.json"));
        var artifact = await File.ReadAllBytesAsync(Path.Combine(fixture.Directory, "bounded-cleanup.image"));
        var cleanup = Task.Run(async () => await session.DisposeAsync());
        try
        {
            await entered.Task.WaitAsync(TimeSpan.FromSeconds(2));
            await cleanup.WaitAsync(TimeSpan.FromSeconds(3));
            Assert.Equal(database, await File.ReadAllBytesAsync(Path.Combine(fixture.Directory, "runs.json")));
            Assert.Equal(artifact, await File.ReadAllBytesAsync(Path.Combine(fixture.Directory, "bounded-cleanup.image")));
            Assert.Equal(1, surface.Submissions);
        }
        finally
        {
            release.Set();
            await cleanup.WaitAsync(TimeSpan.FromSeconds(3));
            await finished.Task.WaitAsync(TimeSpan.FromSeconds(3));
        }
        fixture.Messages.Clear();
        var restoredSurface = new Surface();
        await using var restored = fixture.Session(restoredSurface);
        await restored.ConnectedAsync(default);
        Assert.Equal("awaiting_import", fixture.LastPhase("bounded-cleanup"));
        Assert.Equal(artifact, Convert.FromBase64String(fixture.Last("artifact").GetProperty("bytes_base64").GetString()!));
        Assert.Equal(0, restoredSurface.Submissions);
        Assert.False(restoredSurface.PromptEntered.Task.IsCompleted);
    }

    [Fact]
    public async Task A_connected_helper_reports_unlocked_availability_without_restart_or_any_run()
    {
        using var fixture = new Fixture();
        using var lifetime = new CancellationTokenSource(TimeSpan.FromSeconds(12));
        using var listener = new TcpListener(IPAddress.Loopback, 0);
        listener.Start();
        using var transport = new AutomationWebSocketClient(new Uri($"ws://127.0.0.1:{((IPEndPoint)listener.LocalEndpoint).Port}/api/automation/extension"), "offline-ws-token");
        var surface = new Surface { Available = false };
        await using var session = new ImageAutomationSession(fixture.Directory, surface, transport.SendAsync);
        var client = transport.RunAsync(session, lifetime.Token);
        try
        {
            using var socket = await AcceptWebSocket(listener, lifetime.Token);
            await ReadWebSocket(socket, lifetime.Token);
            await SendWebSocket(socket, ImageAutomationJson.Element(new { type = "authenticated" }), lifetime.Token);
            Assert.False((await ReadWebSocket(socket, lifetime.Token)).GetProperty("available").GetBoolean());
            surface.Available = true;
            var refreshed = await ReadWebSocket(socket, lifetime.Token).WaitAsync(TimeSpan.FromSeconds(6));
            Assert.Equal("probe", refreshed.GetProperty("type").GetString());
            Assert.True(refreshed.GetProperty("available").GetBoolean());
            Assert.False(surface.PromptEntered.Task.IsCompleted);
            Assert.Equal(0, surface.Submissions);
        }
        finally { lifetime.Cancel(); try { await client; } catch (OperationCanceledException) { } }
    }

    [Fact]
    public async Task A_report_failure_after_staging_preserves_original_bytes_for_restart_without_downloading_again()
    {
        using var fixture = new Fixture();
        fixture.ThrowOnSend = message => message.GetProperty("type").GetString() == "status"
            && message.GetProperty("phase").GetString() == "awaiting_import";
        await using (var first = fixture.Session(new Surface()))
        {
            await first.HandleAsync(Start("staged"), default);
            await fixture.WaitPhase("staged", "awaiting_import");
        }
        fixture.Messages.Clear();
        fixture.ThrowOnSend = null;
        var offlineSurface = new Surface { DownloadFails = true };
        await using var restarted = fixture.Session(offlineSurface);
        await restarted.ConnectedAsync(default);
        Assert.Equal("awaiting_import", fixture.LastPhase("staged"));
        Assert.Equal(Png, Convert.FromBase64String(fixture.Last("artifact").GetProperty("bytes_base64").GetString()!));
        Assert.Equal(0, offlineSurface.Submissions);
        Assert.False(offlineSurface.PromptEntered.Task.IsCompleted);
    }

    [Fact]
    public async Task Fixing_capture_after_restart_recovers_the_same_submitted_run_without_input_or_resubmission()
    {
        using var fixture = new Fixture();
        var firstSurface = new Surface { DownloadFails = true, Recoverable = true };
        await using (var first = fixture.Session(firstSurface))
        {
            await first.HandleAsync(Start("recover"), default);
            await fixture.WaitPhase("recover", "paused");
            Assert.Equal(1, firstSurface.Submissions);
        }
        fixture.Messages.Clear();
        var artifactEntered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var deliverArtifact = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        fixture.BeforeSend = async (message, cancellation) =>
        {
            if (message.GetProperty("type").GetString() != "artifact") return;
            artifactEntered.TrySetResult();
            await deliverArtifact.Task.WaitAsync(cancellation);
        };
        var restoredSurface = new Surface { Recoverable = true };
        await using var restored = fixture.Session(restoredSurface);
        await restored.HandleAsync(Message("status", "recover"), default);
        await restoredSurface.Restored.Task.WaitAsync(TimeSpan.FromSeconds(2));
        await fixture.WaitPhase("recover", "awaiting_import");
        try
        {
            await artifactEntered.Task.WaitAsync(TimeSpan.FromSeconds(2));
            // awaiting_import is sent before artifact I/O/delivery. The matching artifact is
            // the completion condition, including when transport delivery yields here.
            var pendingArtifact = fixture.WaitArtifact("recover");
            Assert.False(pendingArtifact.IsCompleted);
            deliverArtifact.TrySetResult();
            var artifact = await pendingArtifact;
            Assert.Equal("recover", artifact.GetProperty("run_id").GetString());
            Assert.Equal("task", artifact.GetProperty("task_id").GetString());
            Assert.Equal("version", artifact.GetProperty("base_version_id").GetString());
            Assert.Equal(Png, Convert.FromBase64String(artifact.GetProperty("bytes_base64").GetString()!));
            Assert.Equal(Convert.ToHexStringLower(SHA256.HashData(Png)), artifact.GetProperty("sha256").GetString());
            Assert.Equal(0, restoredSurface.Submissions);
            Assert.False(restoredSurface.PromptEntered.Task.IsCompleted);
        }
        finally { deliverArtifact.TrySetResult(); }
    }

    [Fact]
    public async Task Real_websocket_reconnect_replays_original_artifact_without_a_second_submission()
    {
        using var fixture = new Fixture();
        using var lifetime = new CancellationTokenSource(TimeSpan.FromSeconds(15));
        using var listener = new TcpListener(IPAddress.Loopback, 0);
        listener.Start();
        var port = ((IPEndPoint)listener.LocalEndpoint).Port;
        using var transport = new AutomationWebSocketClient(new Uri($"ws://127.0.0.1:{port}/api/automation/extension"), "offline-ws-token");
        var surface = new Surface();
        await using var session = new ImageAutomationSession(fixture.Directory, surface, transport.SendAsync);
        var client = transport.RunAsync(session, lifetime.Token);
        try
        {
            using (var socket = await AcceptWebSocket(listener, lifetime.Token))
            {
                var auth = await ReadWebSocket(socket, lifetime.Token);
                Assert.Equal("authenticate", auth.GetProperty("type").GetString());
                Assert.Equal("offline-ws-token", auth.GetProperty("token").GetString());
                await SendWebSocket(socket, ImageAutomationJson.Element(new { type = "authenticated" }), lifetime.Token);
                var probe = await ReadWebSocket(socket, lifetime.Token);
                Assert.Equal("chatgpt-windows", probe.GetProperty("adapter").GetString());
                await SendWebSocket(socket, Start("ws-run"), lifetime.Token);
                JsonElement artifact;
                do { artifact = await ReadWebSocket(socket, lifetime.Token); } while (artifact.GetProperty("type").GetString() != "artifact");
                Assert.Equal(Png, Convert.FromBase64String(artifact.GetProperty("bytes_base64").GetString()!));
                Assert.Equal(1, surface.Submissions);
            }
            using var reconnected = await AcceptWebSocket(listener, lifetime.Token);
            Assert.Equal("authenticate", (await ReadWebSocket(reconnected, lifetime.Token)).GetProperty("type").GetString());
            await SendWebSocket(reconnected, ImageAutomationJson.Element(new { type = "authenticated" }), lifetime.Token);
            JsonElement replay;
            do { replay = await ReadWebSocket(reconnected, lifetime.Token); } while (replay.GetProperty("type").GetString() != "artifact");
            Assert.Equal(Png, Convert.FromBase64String(replay.GetProperty("bytes_base64").GetString()!));
            Assert.Equal(1, surface.Submissions);
            await SendWebSocket(reconnected, Message("artifact_accepted", "ws-run"), lifetime.Token);
            Assert.Equal("completed", (await ReadWebSocket(reconnected, lifetime.Token)).GetProperty("phase").GetString());
        }
        finally
        {
            lifetime.Cancel();
            try { await client; } catch (OperationCanceledException) { }
        }
    }

    [Fact]
    public async Task Failed_durable_ack_keeps_original_bytes_and_awaiting_import_state()
    {
        using var fixture = new Fixture();
        await using var session = fixture.Session(new Surface());
        await session.HandleAsync(Start("disk-failure"), default);
        await fixture.WaitPhase("disk-failure", "awaiting_import");
        var database = Path.Combine(fixture.Directory, "runs.json");
        File.Move(database, database + ".preserved");
        System.IO.Directory.CreateDirectory(database); // Real filesystem replacement failure, not a mocked write.
        await Assert.ThrowsAsync<UnauthorizedAccessException>(() => session.HandleAsync(Message("artifact_accepted", "disk-failure"), default));
        Assert.True(File.Exists(Path.Combine(fixture.Directory, "disk-failure.image")));
        await session.HandleAsync(Message("status", "disk-failure"), default);
        Assert.Equal("awaiting_import", fixture.LastPhase("disk-failure"));
    }

    [Fact]
    public async Task Download_path_outside_owned_run_directory_is_not_read_or_imported()
    {
        using var fixture = new Fixture();
        var outside = Path.Combine(Path.GetTempPath(), "entwurf-outside-" + Guid.NewGuid() + ".png");
        await File.WriteAllBytesAsync(outside, Png);
        try
        {
            await using var session = fixture.Session(new Surface { ExternalDownload = outside });
            await session.HandleAsync(Start("outside"), default);
            await fixture.WaitPhase("outside", "paused");
            Assert.Equal("artifact_invalid", fixture.Last("status", "outside").GetProperty("detail").GetString());
            Assert.DoesNotContain(fixture.Messages, m => m.GetProperty("type").GetString() == "artifact");
            Assert.Equal(Png, await File.ReadAllBytesAsync(outside));
        }
        finally { File.Delete(outside); }
    }

    [Fact]
    public async Task Original_bytes_survive_restart_until_application_ack_and_repeated_start_never_submits_again()
    {
        using var fixture = new Fixture();
        var firstSurface = new Surface();
        await using (var first = fixture.Session(firstSurface))
        {
            await first.HandleAsync(Start("one"), default);
            await fixture.WaitPhase("one", "awaiting_import");
            var artifact = await fixture.WaitArtifact("one");
            Assert.Equal(1, firstSurface.Submissions);
            Assert.Equal(Png, Convert.FromBase64String(artifact.GetProperty("bytes_base64").GetString()!));
            Assert.True(File.Exists(Path.Combine(fixture.Directory, "one.image")));
            Assert.DoesNotContain("fixture image prompt", File.ReadAllText(Path.Combine(fixture.Directory, "runs.json")));
        }
        var resumedSurface = new Surface();
        await using var resumed = fixture.Session(resumedSurface);
        await resumed.ConnectedAsync(default);
        await resumed.HandleAsync(Start("one"), default);
        Assert.Equal(0, resumedSurface.Submissions);
        Assert.Equal("awaiting_import", fixture.LastPhase("one"));
        await resumed.HandleAsync(Message("artifact_accepted", "one"), default);
        Assert.Equal("completed", fixture.LastPhase("one"));
        Assert.False(File.Exists(Path.Combine(fixture.Directory, "one.image")));
        using var saved = JsonDocument.Parse(File.ReadAllBytes(Path.Combine(fixture.Directory, "runs.json")));
        Assert.Equal("completed", saved.RootElement.GetProperty("one").GetProperty("phase").GetString());
    }

    [Fact]
    public async Task Cancellation_while_prompt_is_being_verified_prevents_submission_and_preserves_cancelled_status()
    {
        using var fixture = new Fixture();
        var surface = new Surface { HoldPrompt = true };
        await using var session = fixture.Session(surface);
        await session.HandleAsync(Start("cancel-me"), default);
        await surface.PromptEntered.Task.WaitAsync(TimeSpan.FromSeconds(3));
        await session.HandleAsync(Message("cancel", "cancel-me"), default);
        await fixture.WaitPhase("cancel-me", "cancelled");
        Assert.Equal(0, surface.Submissions);
        await session.HandleAsync(Start("cancel-me"), default);
        Assert.Equal("cancelled", fixture.LastPhase("cancel-me"));
    }

    [Fact]
    public async Task Submit_intent_is_durable_before_click_and_restart_does_not_repeat_an_uncertain_click()
    {
        using var fixture = new Fixture();
        var surface = new Surface
        {
            OnSubmit = () =>
            {
                using var json = JsonDocument.Parse(File.ReadAllBytes(Path.Combine(fixture.Directory, "runs.json")));
                Assert.Equal("submitting", json.RootElement.GetProperty("uncertain").GetProperty("phase").GetString());
                throw new IOException("simulated UIA disconnect after Invoke");
            }
        };
        await using (var session = fixture.Session(surface))
        {
            await session.HandleAsync(Start("uncertain"), default);
            await fixture.WaitPhase("uncertain", "paused");
            Assert.Equal(1, surface.Submissions);
        }
        var restartedSurface = new Surface();
        await using var restarted = fixture.Session(restartedSurface);
        await restarted.HandleAsync(Start("uncertain"), default);
        Assert.Equal("paused", fixture.LastPhase("uncertain"));
        Assert.Equal(0, restartedSurface.Submissions);
    }

    [Fact]
    public async Task Duplicate_old_ack_does_not_cancel_or_release_a_new_active_run()
    {
        using var fixture = new Fixture();
        var surface = new Surface();
        await using var session = fixture.Session(surface);
        await session.HandleAsync(Start("old"), default);
        await fixture.WaitPhase("old", "awaiting_import");
        await session.HandleAsync(Message("artifact_accepted", "old"), default);
        surface.HoldPrompt = true;
        await session.HandleAsync(Start("new"), default);
        await fixture.WaitPhase("new", "preparing");
        await session.HandleAsync(Message("artifact_accepted", "old"), default);
        await session.HandleAsync(Start("third"), default);
        Assert.Equal("failed", fixture.LastPhase("third"));
        Assert.Equal("adapter_busy", fixture.Last("status", "third").GetProperty("detail").GetString());
        Assert.Equal("preparing", fixture.LastPhase("new"));
    }

    [Fact]
    public async Task Reused_run_id_cannot_change_task_version_or_prompt()
    {
        using var fixture = new Fixture();
        await using var session = fixture.Session(new Surface { HoldPrompt = true });
        await session.HandleAsync(Start("identity"), default);
        var changed = Start("identity", "different-task");
        var error = await Assert.ThrowsAsync<ImageAutomationException>(() => session.HandleAsync(changed, default));
        Assert.Equal("run_identity_conflict", error.Reason);
    }

    [Fact]
    public async Task Corrupted_staged_bytes_are_never_replayed_or_acknowledged_after_restart()
    {
        using var fixture = new Fixture();
        await using (var session = fixture.Session(new Surface()))
        {
            await session.HandleAsync(Start("corrupt"), default);
            await fixture.WaitPhase("corrupt", "awaiting_import");
        }
        File.WriteAllBytes(Path.Combine(fixture.Directory, "corrupt.image"), "not an original image"u8.ToArray());
        fixture.Messages.Clear();
        await using var restarted = fixture.Session(new Surface());
        await restarted.ConnectedAsync(default);
        await restarted.HandleAsync(Message("artifact_accepted", "corrupt"), default);
        Assert.Equal("paused", fixture.LastPhase("corrupt"));
        Assert.Equal("artifact_invalid", fixture.Last("status", "corrupt").GetProperty("detail").GetString());
        Assert.DoesNotContain(fixture.Messages, m => m.GetProperty("type").GetString() == "artifact");
    }

    [Theory]
    [InlineData("ws://example.com/api/automation/extension")]
    [InlineData("ws://127.0.0.1:48123/api/automation/extension?token=secret")]
    [InlineData("ws://user:pass@localhost:48123/api/automation/extension")]
    [InlineData("http://127.0.0.1:48123/api/automation/extension")]
    [InlineData("ws://127.0.0.1:48123/api/commands")]
    public void Bootstrap_refuses_remote_hosts_credentials_in_URL_and_broader_endpoints(string address)
    {
        Assert.Throws<InvalidDataException>(() => new AutomationBootstrapConfig(address, "test-only", Path.GetTempPath(), Environment.ProcessId).Validate());
    }

    static JsonElement Start(string id, string task = "task")
    {
        const string prompt = "fixture image prompt";
        return ImageAutomationJson.Element(new { type = "start", run = new
        {
            run_id = id, task_id = task, base_version_id = "version", prompt,
            prompt_sha256 = Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(prompt)))
        } });
    }
    static JsonElement Message(string type, string id) => ImageAutomationJson.Element(new { type, run_id = id });

    static async Task<WebSocket> AcceptWebSocket(TcpListener listener, CancellationToken cancellation)
    {
        var client = await listener.AcceptTcpClientAsync(cancellation);
        var stream = client.GetStream();
        var header = new List<byte>();
        while (header.Count < 8192)
        {
            var one = new byte[1];
            await stream.ReadExactlyAsync(one, cancellation);
            header.Add(one[0]);
            if (header.Count >= 4 && header.TakeLast(4).SequenceEqual("\r\n\r\n"u8.ToArray())) break;
        }
        var request = Encoding.ASCII.GetString(header.ToArray());
        Assert.StartsWith("GET /api/automation/extension HTTP/1.1\r\n", request);
        Assert.DoesNotContain("offline-ws-token", request);
        var key = request.Split("\r\n").Single(x => x.StartsWith("Sec-WebSocket-Key:", StringComparison.OrdinalIgnoreCase)).Split(':', 2)[1].Trim();
        var accept = Convert.ToBase64String(SHA1.HashData(Encoding.ASCII.GetBytes(key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11")));
        await stream.WriteAsync(Encoding.ASCII.GetBytes($"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"), cancellation);
        return WebSocket.CreateFromStream(stream, true, null, TimeSpan.FromSeconds(30));
    }
    static async Task<JsonElement> ReadWebSocket(WebSocket socket, CancellationToken cancellation)
    {
        var data = new byte[8192];
        var read = await socket.ReceiveAsync(data, cancellation);
        Assert.True(read.EndOfMessage);
        using var document = JsonDocument.Parse(data.AsMemory(0, read.Count));
        return document.RootElement.Clone();
    }
    static async Task SendWebSocket(WebSocket socket, JsonElement message, CancellationToken cancellation) =>
        await socket.SendAsync(JsonSerializer.SerializeToUtf8Bytes(message), WebSocketMessageType.Text, true, cancellation);

    sealed class Fixture : IDisposable
    {
        public string Directory { get; } = Path.Combine(Path.GetTempPath(), "entwurf-native-session-" + Guid.NewGuid());
        public ConcurrentQueue<JsonElement> Messages { get; } = new();
        public Func<JsonElement, bool>? ThrowOnSend;
        public Func<JsonElement, CancellationToken, Task>? BeforeSend;
        public ImageAutomationSession Session(Surface surface) => new(Directory, surface, async (message, cancellation) =>
        {
            if (BeforeSend is not null) await BeforeSend(message, cancellation);
            Messages.Enqueue(message.Clone());
            if (ThrowOnSend?.Invoke(message) == true) throw new IOException("simulated transport failure after durable staging");
        });
        public JsonElement Last(string type, string? id = null) => Messages.Last(m => m.GetProperty("type").GetString() == type && (id is null || m.GetProperty("run_id").GetString() == id));
        public string? LastPhase(string id) => Last("status", id).GetProperty("phase").GetString();
        public async Task<JsonElement> WaitArtifact(string id)
        {
            using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(5));
            while (true)
            {
                var artifact = Messages.LastOrDefault(m => m.GetProperty("type").GetString() == "artifact" && m.GetProperty("run_id").GetString() == id);
                if (artifact.ValueKind != JsonValueKind.Undefined) return artifact;
                await Task.Delay(10, timeout.Token);
            }
        }
        public async Task WaitPhase(string id, string phase)
        {
            using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(5));
            while (!Messages.Any(m => m.GetProperty("type").GetString() == "status" && m.GetProperty("run_id").GetString() == id && m.GetProperty("phase").GetString() == phase))
                await Task.Delay(10, timeout.Token);
        }
        public void Dispose() { if (System.IO.Directory.Exists(Directory)) System.IO.Directory.Delete(Directory, true); }
    }
    sealed class Conversation : PreparedImageConversation { }
    sealed class Surface : IImageAutomationSurface
    {
        public int Submissions;
        public bool HoldPrompt;
        public Action? OnSubmit;
        public Action? OnDispose;
        public string? ExternalDownload;
        public bool DownloadFails;
        public bool Recoverable;
        public volatile bool Available = true;
        public TaskCompletionSource Restored { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource PromptEntered { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public Task<ImageSurfaceProbe> ProbeAsync(CancellationToken cancellation) => Task.FromResult(new ImageSurfaceProbe(Available, Available ? "ready" : "windows_locked"));
        public Task<PreparedImageConversation> PrepareAsync(ImageAutomationRun run, CancellationToken cancellation) => Task.FromResult<PreparedImageConversation>(new Conversation());
        public async Task WriteAndVerifyPromptAsync(PreparedImageConversation conversation, string prompt, CancellationToken cancellation)
        {
            PromptEntered.TrySetResult();
            if (HoldPrompt) await Task.Delay(Timeout.InfiniteTimeSpan, cancellation);
        }
        public Task SubmitOnceAsync(PreparedImageConversation conversation, CancellationToken cancellation)
        {
            cancellation.ThrowIfCancellationRequested();
            Interlocked.Increment(ref Submissions);
            OnSubmit?.Invoke();
            return Task.CompletedTask;
        }
        public Task WaitForGeneratedImageAsync(PreparedImageConversation conversation, CancellationToken cancellation) => Task.CompletedTask;
        public async Task<string> DownloadOriginalAsync(PreparedImageConversation conversation, string ownedDirectory, CancellationToken cancellation)
        {
            if (DownloadFails) throw new ImageAutomationException("download_dialog_unavailable");
            if (ExternalDownload is not null) return ExternalDownload;
            var path = Path.Combine(ownedDirectory, "original.png");
            await File.WriteAllBytesAsync(path, Png, cancellation);
            return path;
        }
        public Task ClearOwnedDraftAsync(PreparedImageConversation conversation, string prompt, CancellationToken cancellation) => Task.CompletedTask;
        public JsonElement? ExportBinding(PreparedImageConversation conversation) => Recoverable ? ImageAutomationJson.Element(new { existing_user_turn = "owned-existing-turn" }) : null;
        public Task<PreparedImageConversation?> RestoreSubmittedAsync(ImageRunRecord run, JsonElement binding, CancellationToken cancellation)
        {
            Assert.True(run.SubmitIntent);
            Assert.Equal("owned-existing-turn", binding.GetProperty("existing_user_turn").GetString());
            Restored.TrySetResult();
            return Task.FromResult<PreparedImageConversation?>(new Conversation());
        }
        public void Dispose() { OnDispose?.Invoke(); }
    }
}
