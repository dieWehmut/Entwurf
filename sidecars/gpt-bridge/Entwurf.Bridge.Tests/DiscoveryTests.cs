using System.Text.Json;
using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class DiscoveryTests
{
    static BridgeRequest Request(string method) => new("probe", method, null,
        JsonSerializer.SerializeToElement(new { task_id = "blocked-probe", revision = 1, explicit_start = true }));
    static BridgeHost Attached(IWindowsAutomation automation)
    {
        var host = new BridgeHost(automation, TextWriter.Null, "known");
        host.Handle(new("attach", "attach", "known", null));
        return host;
    }
    static Task<BridgeResponse> OnDedicatedThread(BridgeHost host, string method) => Task.Factory.StartNew(
        () => host.Handle(Request(method)), CancellationToken.None, TaskCreationOptions.LongRunning, TaskScheduler.Default);

    [Theory]
    [InlineData("pause")]
    [InlineData("cancel")]
    public async Task Start_discovery_does_not_block_control_requests(string control)
    {
        using var adapter = new BlockingDiscovery();
        using var host = Attached(adapter);
        var start = OnDedicatedThread(host, "start_conversation");
        Task<BridgeResponse>? command = null;
        try
        {
            await adapter.Entered.Task.WaitAsync(TimeSpan.FromSeconds(2));
            command = OnDedicatedThread(host, control);
            var response = await command.WaitAsync(TimeSpan.FromSeconds(1));
            Assert.Equal(control == "pause" ? "paused" : "cancelled", response.Result.GetProperty("state").GetString());
            var error = await Assert.ThrowsAsync<BridgeException>(async () => await start.WaitAsync(TimeSpan.FromSeconds(1)));
            Assert.Equal("cancelled", error.Code);
            Assert.False(adapter.Released);
            Assert.Equal(0, adapter.ConversationsStarted);
        }
        finally
        {
            adapter.Release();
            try { await start; } catch (BridgeException) { }
            if (command is not null) await command;
        }
    }

    [Fact]
    public async Task Cancelled_discovery_is_reused_by_start_without_a_second_underlying_probe()
    {
        using var adapter = new BlockingDiscovery();
        using var host = Attached(adapter);
        var detection = host.HandleAsync(Request("detect_app"), default);
        Task<BridgeResponse>? start = null;
        try
        {
            await adapter.Entered.Task.WaitAsync(TimeSpan.FromSeconds(2));
            host.Handle(Request("cancel"));
            Assert.Equal("cancelled", (await Assert.ThrowsAsync<BridgeException>(async () => await detection)).Code);
            var returned = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            start = Task.Factory.StartNew(async () =>
            {
                var pending = host.HandleAsync(Request("start_conversation"), default);
                returned.TrySetResult();
                return await pending;
            }, CancellationToken.None, TaskCreationOptions.LongRunning, TaskScheduler.Default).Unwrap();
            await Task.WhenAny(returned.Task, adapter.SecondProbe.Task).WaitAsync(TimeSpan.FromSeconds(2));
            Assert.Equal(1, adapter.Probes);
            await returned.Task.WaitAsync(TimeSpan.FromSeconds(1));
            host.Handle(Request("cancel"));
            Assert.Equal("cancelled", (await Assert.ThrowsAsync<BridgeException>(async () => await start.WaitAsync(TimeSpan.FromSeconds(1)))).Code);
            Assert.Equal(0, adapter.ConversationsStarted);
        }
        finally
        {
            adapter.Release();
            try { await detection; } catch (BridgeException) { }
            if (start is not null) try { await start; } catch (BridgeException) { }
        }
    }

    [Fact]
    public async Task Start_probe_timeout_is_bounded_and_retry_reuses_the_stalled_probe()
    {
        using var adapter = new BlockingDiscovery();
        using var host = Attached(adapter);
        var start = OnDedicatedThread(host, "start_conversation");
        try
        {
            await adapter.Entered.Task.WaitAsync(TimeSpan.FromSeconds(2));
            var error = await Assert.ThrowsAsync<BridgeException>(async () => await start.WaitAsync(TimeSpan.FromSeconds(11)));
            Assert.Equal("timeout", error.Code);
            Assert.False(adapter.Released);
            host.Handle(Request("cancel"));
            var retry = host.HandleAsync(Request("start_conversation"), default);
            host.Handle(Request("pause"));
            Assert.Equal("cancelled", (await Assert.ThrowsAsync<BridgeException>(async () => await retry)).Code);
            Assert.Equal(1, adapter.Probes);
            Assert.Equal(0, adapter.ConversationsStarted);
        }
        finally
        {
            adapter.Release();
            try { await start; } catch (BridgeException) { }
        }
    }

    // The blocked external provider is replaced, not the host's coordination or cancellation.
    sealed class BlockingDiscovery : IWindowsAutomation, IDisposable
    {
        readonly TaskCompletionSource release = new(TaskCreationOptions.RunContinuationsAsynchronously);
        int probes, conversations;
        public int Probes => Volatile.Read(ref probes);
        public int ConversationsStarted => Volatile.Read(ref conversations);
        public bool Released => release.Task.IsCompleted;
        public TaskCompletionSource Entered { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource SecondProbe { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public void Release() => release.TrySetResult();
        public CapabilityReport Detect()
        {
            if (Interlocked.Increment(ref probes) > 1) SecondProbe.TrySetResult();
            Entered.TrySetResult();
            release.Task.GetAwaiter().GetResult();
            return new(true, true, true, null, "controlled", 1, 5, "controlled", new()
            {
                ["discover_window"] = new(true, "controlled"), ["reliable_input"] = new(true, "controlled"),
                ["reliable_completion"] = new(true, "controlled"), ["reliable_capture"] = new(true, "controlled")
            });
        }
        public SafetyState CheckSafety() => new(true, true, true);
        public Observation Observe() => new(false, "waiting", null);
        public string CaptureText() => "";
        public void Paste(string text) { }
        public void Submit() { }
        public void OpenChatGpt() => Interlocked.Increment(ref conversations);
        public void Dispose() => Release();
    }
}
