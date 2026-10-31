using System.Text.Json;
using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class BridgeHostTests
{
    static BridgeRequest Request(string method, string? token = null, object? parameters = null) => new("req-1", method, token,
        parameters is null ? null : JsonSerializer.SerializeToElement(parameters));
    static BridgeHost Attached(IWindowsAutomation? automation = null, string[]? directories = null)
    {
        var host = new BridgeHost(automation ?? new ControlledAutomation(), TextWriter.Null, "known", directories);
        host.Handle(Request("attach", "known"));
        return host;
    }
    static void Start(BridgeHost host) => host.Handle(Request("start_conversation", parameters: new { task_id = "test-task", revision = 1, explicit_start = true }));

    [Fact]
    public void Attach_token_is_consumed_once()
    {
        using var host = Attached();
        Assert.Equal("already_attached", Assert.Throws<BridgeException>(() => host.Handle(Request("attach", "known"))).Code);
    }
    [Fact]
    public void Wrong_token_does_not_authorize_or_consume_correct_token()
    {
        using var host = new BridgeHost(new ControlledAutomation(), TextWriter.Null, "known");
        Assert.Equal("unauthorized", Assert.Throws<BridgeException>(() => host.Handle(Request("attach", "wrong"))).Code);
        Assert.Equal("unauthorized", Assert.Throws<BridgeException>(() => host.Handle(Request("submit"))).Code);
        Assert.True(host.Handle(Request("attach", "known")).Ok);
    }
    [Fact]
    public void Detection_is_public_and_does_not_grant_access()
    {
        using var host = new BridgeHost(new ControlledAutomation(), TextWriter.Null, "known");
        Assert.Equal(4, host.Handle(Request("detect_app")).Result.GetProperty("gates").EnumerateObject().Count());
        Assert.False(host.Authorized);
    }
    [Fact]
    public void Read_only_detection_remains_available_while_paused()
    {
        using var host = Attached();
        host.Handle(Request("pause"));
        Assert.True(host.Handle(Request("detect_app")).Ok);
    }

    [Fact]
    public void Missing_release_gate_and_missing_explicit_start_block_mutation()
    {
        using var host = Attached(new ControlledAutomation { Verified = false });
        Assert.Equal("explicit_start_required", Assert.Throws<BridgeException>(() => host.Handle(Request("start_conversation"))).Code);
        Assert.Equal("automation_unavailable", Assert.Throws<BridgeException>(() => Start(host)).Code);
    }
    [Fact]
    public void One_task_and_ordered_input_are_enforced()
    {
        using var host = Attached();
        Start(host);
        Assert.Equal("task_active", Assert.Throws<BridgeException>(() => Start(host)).Code);
        Assert.Equal("invalid_state", Assert.Throws<BridgeException>(() => host.Handle(Request("submit"))).Code);
        Assert.Equal("empty_prompt", Assert.Throws<BridgeException>(() => host.Handle(Request("paste_prompt", parameters: new { text = " " }))).Code);
        host.Handle(Request("paste_prompt", parameters: new { text = "test envelope" }));
        host.Handle(Request("submit"));
        Assert.Equal("invalid_state", Assert.Throws<BridgeException>(() => host.Handle(Request("submit"))).Code);
    }
    [Theory]
    [InlineData(false, true, true)]
    [InlineData(true, false, true)]
    [InlineData(true, true, false)]
    public void Foreground_lock_or_structure_change_prevents_input(bool foreground, bool unlocked, bool structure)
    {
        var automation = new ControlledAutomation();
        using var host = Attached(automation);
        Start(host);
        automation.Safety = new(foreground, unlocked, structure);
        Assert.Equal("foreground_lost", Assert.Throws<BridgeException>(() => host.Handle(Request("paste_prompt", parameters: new { text = "x" }))).Code);
        Assert.Equal("paused", Assert.Throws<BridgeException>(() => host.Handle(Request("submit"))).Code);
    }
    [Fact]
    public void Cancel_leaves_no_active_task_and_allows_fresh_explicit_start()
    {
        using var host = Attached();
        Start(host);
        host.Handle(Request("pause"));
        host.Handle(Request("cancel"));
        Assert.Equal("no_active_task", Assert.Throws<BridgeException>(() => host.Handle(Request("submit"))).Code);
        Start(host);
    }
    [Fact]
    public async Task Pause_interrupts_an_active_artifact_wait_without_waiting_for_timeout()
    {
        var directory = Path.Combine(Path.GetTempPath(), "entwurf-pause-" + Guid.NewGuid());
        Directory.CreateDirectory(directory);
        try
        {
            using var host = Attached(directories: [directory]);
            var pending = host.HandleAsync(Request("watch_artifact", parameters: new { task_id = "test", revision = 1, timeout_ms = 30000 }), default);
            host.Handle(Request("pause"));
            var error = await Assert.ThrowsAsync<BridgeException>(async () => await pending.WaitAsync(TimeSpan.FromSeconds(2)));
            Assert.Equal("cancelled", error.Code);
        }
        finally { Directory.Delete(directory, true); }
    }
    [Fact]
    public async Task Watchdog_pauses_even_when_no_new_rpc_arrives()
    {
        var automation = new ControlledAutomation();
        using var host = Attached(automation);
        Start(host);
        automation.Safety = new(false, true, true);
        await Task.Delay(250);
        Assert.Equal("paused", Assert.Throws<BridgeException>(() => host.Handle(Request("submit"))).Code);
    }
    // A deterministic adapter isolates host state/safety tests from the user's desktop.
    sealed class ControlledAutomation : IWindowsAutomation
    {
        public bool Verified { get; init; } = true;
        public SafetyState Safety { get; set; } = new(true, true, true);
        public CapabilityReport Detect() => new(true, true, true, null, "test", 1, 5, "controlled adapter", new()
        {
            ["discover_window"] = new(true, "test"), ["reliable_input"] = new(Verified, "test"),
            ["reliable_completion"] = new(true, "test"), ["reliable_capture"] = new(true, "test")
        });
        public SafetyState CheckSafety() => Safety;
        public Observation Observe() => new(false, "waiting", null);
        public string CaptureText() => "";
        public void Paste(string text) { }
        public void Submit() { }
        public void OpenChatGpt() { }
    }
}
