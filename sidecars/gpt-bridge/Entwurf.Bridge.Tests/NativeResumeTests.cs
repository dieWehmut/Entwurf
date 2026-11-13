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

    [Fact]
    public async Task New_runs_preserve_a_parked_run_and_its_consumed_resume_until_explicit_artifact_ack()
    {
        using var fixture = new Fixture(); fixture.Seed("history");
        var surface = new Surface();
        await using var session = fixture.Session(surface);
        var original = fixture.Record().GetRawText();
        var consumed = fixture.Record().GetProperty("resume_request_ids")[0].GetString()!;
        await session.ConnectedAsync(default);
        Assert.True(fixture.Messages.Last(x => x.GetProperty("type").GetString() == "status").GetProperty("pre_submit_parked").GetBoolean());

        await session.HandleAsync(Start(Run("new-run")), default);
        await fixture.Until(() => fixture.Messages.Any(x => x.GetProperty("type").GetString() == "artifact" && x.GetProperty("run_id").GetString() == "new-run"));
        await fixture.Until(() => fixture.Messages.Any(x => x.GetProperty("type").GetString() == "status" && x.GetProperty("run_id").GetString() == "same-run"
            && x.GetProperty("pre_submit_parked").GetBoolean() && !x.GetProperty("pre_submit_retryable").GetBoolean()));
        await session.HandleAsync(Start(Run()), default);
        await session.HandleAsync(Resume(Run(), consumed), default);
        await session.HandleAsync(Start(Run("blocked-third")), default);
        Assert.Equal("adapter_busy", fixture.Record("blocked-third").GetProperty("reason").GetString());
        Assert.Equal(original, fixture.Record().GetRawText());
        Assert.Equal(new[] { "new-run" }, surface.PreparedRunIds);

        await session.HandleAsync(ImageAutomationJson.Element(new { type = "artifact_accepted", run_id = "new-run" }), default);
        Assert.True(fixture.Messages.Last(x => x.GetProperty("type").GetString() == "status" && x.GetProperty("run_id").GetString() == "same-run").GetProperty("pre_submit_retryable").GetBoolean());
        await session.HandleAsync(Start(Run("second-new")), default);
        await fixture.Until(() => fixture.Messages.Any(x => x.GetProperty("type").GetString() == "artifact" && x.GetProperty("run_id").GetString() == "second-new"));
        Assert.Equal(new[] { "new-run", "second-new" }, surface.PreparedRunIds);
        Assert.Equal(2, surface.Submissions);
        Assert.Equal(original, fixture.Record().GetRawText());
        Assert.DoesNotContain("pre_submit_parked", File.ReadAllText(fixture.Database));
    }

    [Fact]
    public async Task Multiple_parked_runs_and_an_exhausted_resume_history_do_not_execute_or_block_new_runs()
    {
        using var fixture = new Fixture(); fixture.Seed("parked_limit");
        var surface = new Surface { Fail = "prepare" };
        await using var session = fixture.Session(surface);
        var original = fixture.Record().GetRawText();
        var other = fixture.Record("other-run").GetRawText();
        await session.ConnectedAsync(default);
        var initial = fixture.Messages.Last(x => x.GetProperty("type").GetString() == "status" && x.GetProperty("run_id").GetString() == "same-run");
        Assert.True(initial.GetProperty("pre_submit_parked").GetBoolean());
        Assert.False(initial.GetProperty("pre_submit_retryable").GetBoolean());
        await session.HandleAsync(Start(Run("new-run")), default);
        await fixture.Until(() => fixture.Messages.Any(x => x.GetProperty("type").GetString() == "status" && x.GetProperty("run_id").GetString() == "new-run"
            && x.GetProperty("pre_submit_parked").GetBoolean()));
        await session.HandleAsync(Start(Run("new-run")), default);
        await session.HandleAsync(Resume(Run(), Guid.NewGuid().ToString()), default);
        await session.HandleAsync(Start(Run("next-run")), default);
        await fixture.Until(() => fixture.Messages.Any(x => x.GetProperty("type").GetString() == "status" && x.GetProperty("run_id").GetString() == "next-run"
            && x.GetProperty("pre_submit_parked").GetBoolean()));
        Assert.Equal(new[] { "new-run", "next-run" }, surface.PreparedRunIds);
        Assert.Equal(0, surface.Submissions);
        Assert.Equal(original, fixture.Record().GetRawText());
        Assert.Equal(other, fixture.Record("other-run").GetRawText());
    }

    [Theory]
    [InlineData("missing_intent")]
    [InlineData("null_intent")]
    [InlineData("submitted")]
    [InlineData("binding")]
    [InlineData("interrupted_write")]
    [InlineData("interrupted_submit")]
    [InlineData("sha")]
    [InlineData("mime")]
    [InlineData("artifact")]
    [InlineData("download")]
    [InlineData("artifact_directory")]
    [InlineData("download_file")]
    [InlineData("download_parent_file")]
    public async Task An_unsubmitted_flag_alone_does_not_release_an_ambiguous_run_slot(string condition)
    {
        using var fixture = new Fixture(); fixture.Seed(condition);
        var surface = new Surface();
        await using var session = fixture.Session(surface);
        var original = fixture.Record().GetRawText();
        await session.ConnectedAsync(default);
        Assert.False(fixture.Messages.Last(x => x.GetProperty("type").GetString() == "status").GetProperty("pre_submit_parked").GetBoolean());
        await session.HandleAsync(Start(Run("new-run")), default);
        Assert.Equal("adapter_busy", fixture.Record("new-run").GetProperty("reason").GetString());
        Assert.Equal(0, surface.Prepares);
        Assert.Equal(0, surface.Submissions);
        Assert.Equal(original, fixture.Record().GetRawText());
    }

    [UnprivilegedSymlinkTheory]
    [InlineData("artifact_reparse")]
    [InlineData("downloads_reparse")]
    public Task Dangling_reparse_paths_do_not_release_a_run_slot(string condition)
        => An_unsubmitted_flag_alone_does_not_release_an_ambiguous_run_slot(condition);

    [UnprivilegedDirectoryRemovalFact]
    public async Task A_missing_bound_directory_is_not_an_observed_empty_parked_run()
    {
        using var fixture = new Fixture(); fixture.Seed();
        var surface = new Surface();
        await using var session = fixture.Session(surface);
        // Linux can remove an open directory tree; Windows ownership locking
        // prevents this fixture operation without changing the lock's contract.
        Directory.Delete(fixture.Directory, true);
        await session.HandleAsync(Status("same-run"), default);
        var status = fixture.Messages.Last(x => x.GetProperty("type").GetString() == "status");
        Assert.False(status.GetProperty("pre_submit_parked").GetBoolean());
        Assert.False(status.GetProperty("pre_submit_retryable").GetBoolean());
        Assert.Equal(0, surface.Prepares);
        Assert.Equal(0, surface.Submissions);
    }

    sealed class UnprivilegedSymlinkTheoryAttribute : TheoryAttribute
    {
        public UnprivilegedSymlinkTheoryAttribute()
        {
            if (OperatingSystem.IsWindows()) Skip = "Symlink fixtures run on Linux without enabling Windows developer mode or elevation.";
        }
    }

    sealed class UnprivilegedDirectoryRemovalFactAttribute : FactAttribute
    {
        public UnprivilegedDirectoryRemovalFactAttribute()
        {
            if (OperatingSystem.IsWindows()) Skip = "Removing an open owned directory is exercised on Linux without changing Windows ownership locks.";
        }
    }

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
        await restarted.HandleAsync(Start(Run("independent")), default);
        Assert.Equal("adapter_busy", fixture.Record("independent").GetProperty("reason").GetString());
        Assert.Equal(0, restartedSurface.Prepares);
        Assert.Equal(0, restartedSurface.Submissions);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task A_snapshot_taken_during_pending_write_cannot_replay_after_abrupt_restart(bool exportsBinding)
    {
        using var fixture = new Fixture();
        using var interrupted = new Fixture();
        var hold = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var surface = new Surface
        {
            HoldWrite = hold.Task,
            Binding = exportsBinding ? ImageAutomationJson.Element(new { window = 20, document_id = "owned", draft_cleanup_state = "not_started" }) : null
        };
        byte[]? snapshot = null;
        surface.BeforeWrite = () =>
        {
            var persisted = fixture.Record();
            Assert.Equal("write", persisted.GetProperty("stage").GetString());
            Assert.False(persisted.GetProperty("submit_intent").GetBoolean());
            if (exportsBinding) Assert.Equal("owned", persisted.GetProperty("surface_binding").GetProperty("document_id").GetString());
            snapshot = File.ReadAllBytes(fixture.Database);
        };
        await using var active = fixture.Session(surface);
        await active.HandleAsync(Start(Run()), default);
        await surface.WriteEntered.Task.WaitAsync(TimeSpan.FromSeconds(2));
        Assert.NotNull(snapshot);
        Assert.DoesNotContain(Run().Prompt, System.Text.Encoding.UTF8.GetString(snapshot));
        // Copy the authoritative in-flight disk state before either failure handling
        // or cleanup runs, as an abruptly stopped process would leave it.
        System.IO.Directory.CreateDirectory(interrupted.Directory);
        File.WriteAllBytes(interrupted.Database, snapshot);
        var restartedSurface = new Surface();
        await using var restarted = interrupted.Session(restartedSurface);
        await restarted.ConnectedAsync(default);
        await restarted.HandleAsync(Start(Run()), default);
        await restarted.HandleAsync(Status("same-run"), default);
        Assert.False(interrupted.Messages.Last(x => x.GetProperty("type").GetString() == "status").GetProperty("pre_submit_retryable").GetBoolean());
        await restarted.HandleAsync(Resume(Run(), Guid.NewGuid().ToString()), default);
        Assert.Contains(interrupted.Messages, x => x.GetProperty("type").GetString() == "resume_rejected");
        await restarted.HandleAsync(Start(Run("independent")), default);
        Assert.Equal("adapter_busy", interrupted.Record("independent").GetProperty("reason").GetString());
        Assert.Equal(0, restartedSurface.Prepares);
        Assert.False(restartedSurface.WriteEntered.Task.IsCompleted);
        Assert.Equal(0, restartedSurface.Submissions);
        Assert.False(hold.Task.IsCompleted);
    }

    [Fact]
    public async Task A_failed_binding_export_prevents_the_first_prompt_write()
    {
        using var fixture = new Fixture();
        var surface = new Surface
        {
            FailingBindingExports = 1,
            Binding = ImageAutomationJson.Element(new { window = 20, document_id = "owned" })
        };
        await using var session = fixture.Session(surface);
        await session.HandleAsync(Start(Run()), default);
        await fixture.Until(() => fixture.Messages.Any(x => x.TryGetProperty("exception_type", out _)));
        Assert.False(surface.WriteEntered.Task.IsCompleted);
        Assert.Equal(0, surface.Submissions);
        Assert.False(fixture.Record().GetProperty("submit_intent").GetBoolean());
    }

    [Fact]
    public async Task A_failed_binding_persist_prevents_the_first_prompt_write()
    {
        using var fixture = new Fixture();
        var cleanupEntered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var releaseCleanup = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var surface = new Surface
        {
            Binding = ImageAutomationJson.Element(new { window = 20, document_id = "owned" }),
            BeforePrepare = () =>
            {
                File.Move(fixture.Database, fixture.Database + ".preserved");
                System.IO.Directory.CreateDirectory(fixture.Database);
            },
            BeforeCleanup = () => cleanupEntered.TrySetResult(),
            Cleanup = releaseCleanup.Task
        };
        var session = fixture.Session(surface);
        try
        {
            await session.HandleAsync(Start(Run()), default);
            await cleanupEntered.Task.WaitAsync(TimeSpan.FromSeconds(2));
            Assert.False(surface.WriteEntered.Task.IsCompleted);
            Assert.Equal(0, surface.Submissions);
        }
        finally
        {
            if (System.IO.Directory.Exists(fixture.Database)) System.IO.Directory.Delete(fixture.Database);
            if (File.Exists(fixture.Database + ".preserved")) File.Move(fixture.Database + ".preserved", fixture.Database);
            releaseCleanup.TrySetResult();
            await session.DisposeAsync();
        }
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
    [InlineData("interrupted_write")]
    [InlineData("interrupted_submit")]
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
        await using (var session = fixture.Session(surface))
        {
            await session.HandleAsync(Resume(Run(), Guid.NewGuid().ToString()), default);
            await fixture.Until(() => fixture.Messages.Any(x => x.TryGetProperty("phase", out var phase) && phase.GetString() == "paused" && x.TryGetProperty("exception_type", out _)));
            try
            {
                await session.HandleAsync(Status("same-run"), default);
                Assert.False(fixture.Messages.Last(x => x.GetProperty("type").GetString() == "status").GetProperty("pre_submit_retryable").GetBoolean());
                await session.HandleAsync(Resume(Run(), Guid.NewGuid().ToString()), default);
                await session.HandleAsync(Start(Run("during-cleanup")), default);
                Assert.Equal("adapter_busy", fixture.Record("during-cleanup").GetProperty("reason").GetString());
                Assert.Equal(1, surface.Prepares);
            }
            finally { release.TrySetResult(); }
        }
        var restartedSurface = new Surface();
        await using var restarted = fixture.Session(restartedSurface);
        await restarted.HandleAsync(Status("same-run"), default);
        Assert.False(fixture.Messages.Last(x => x.GetProperty("type").GetString() == "status").GetProperty("pre_submit_retryable").GetBoolean());
        await restarted.HandleAsync(Resume(Run(), Guid.NewGuid().ToString()), default);
        await restarted.HandleAsync(Start(Run("after-cleanup")), default);
        Assert.Equal("adapter_busy", fixture.Record("after-cleanup").GetProperty("reason").GetString());
        Assert.Equal(0, restartedSurface.Prepares);
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
        public JsonElement Record(string id = "same-run") { using var stream = new FileStream(Database, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete); using var json = JsonDocument.Parse(stream); return json.RootElement.GetProperty(id).Clone(); }
        public async Task Until(Func<bool> condition) { using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(4)); try { while (!condition()) await Task.Delay(10, deadline.Token); } catch (OperationCanceledException) { throw new Xunit.Sdk.XunitException("Timed out; durable fixture record: " + Record().GetRawText()); } }
        public void Seed(string? condition = null)
        {
            System.IO.Directory.CreateDirectory(Directory);
            var record = new Dictionary<string, object?> { ["run_id"] = "same-run", ["task_id"] = "task", ["base_version_id"] = "version", ["prompt_sha256"] = Run().PromptSha256, ["phase"] = condition == "terminal" ? "cancelled" : "paused", ["reason"] = "automation_unavailable", ["submit_intent"] = false };
            if (condition == "missing_intent") record.Remove("submit_intent");
            if (condition == "null_intent") record["submit_intent"] = null;
            if (condition == "submitted") record["submit_intent"] = true;
            if (condition == "binding") record["surface_binding"] = new { owned = true };
            if (condition is "interrupted_write" or "interrupted_submit")
            { record["phase"] = "preparing"; record["stage"] = condition == "interrupted_write" ? "write" : "submit"; }
            if (condition == "sha") record["sha256"] = new string('a', 64);
            if (condition == "mime") record["mime_type"] = "image/png";
            if (condition == "artifact") File.WriteAllBytes(Artifact, Png);
            if (condition == "artifact_directory") System.IO.Directory.CreateDirectory(Artifact);
            if (condition == "artifact_reparse") File.CreateSymbolicLink(Artifact, Path.Combine(Directory, "missing-artifact"));
            if (condition == "download") { var path = Path.Combine(Directory, "downloads", "same-run"); System.IO.Directory.CreateDirectory(path); File.WriteAllBytes(Path.Combine(path, "original.png"), Png); }
            if (condition == "download_file") { var path = Path.Combine(Directory, "downloads"); System.IO.Directory.CreateDirectory(path); File.WriteAllBytes(Path.Combine(path, "same-run"), Png); }
            if (condition == "downloads_reparse") System.IO.Directory.CreateSymbolicLink(Path.Combine(Directory, "downloads"), Path.Combine(Directory, "missing-downloads"));
            if (condition == "download_parent_file") File.WriteAllBytes(Path.Combine(Directory, "downloads"), Png);
            if (condition == "history") record["resume_request_ids"] = new[] { "11111111-1111-4111-8111-111111111111" };
            if (condition is "limit" or "parked_limit") record["resume_request_ids"] = Enumerable.Range(0, 64).Select(_ => Guid.NewGuid().ToString()).ToArray();
            var records = new Dictionary<string, object?> { ["same-run"] = record };
            if (condition is "other_run" or "parked_limit") records["other-run"] = new { run_id = "other-run", task_id = "task", base_version_id = "version", prompt_sha256 = Run().PromptSha256, phase = "paused", submit_intent = condition == "other_run" ? (bool?)null : false };
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
        public ConcurrentQueue<string> PreparedRunIds { get; } = new();
        public JsonElement? Binding; public Action? BeforeCleanup; public Action? BeforeWrite; public int FailingBindingExports;
        void Check(string stage) { if (Fail == stage) throw Failure(); }
        public Task<ImageSurfaceProbe> ProbeAsync(CancellationToken token) => Task.FromResult(new ImageSurfaceProbe(true, "ready"));
        public Task<PreparedImageConversation> PrepareAsync(ImageAutomationRun run, CancellationToken token) { token.ThrowIfCancellationRequested(); Interlocked.Increment(ref Prepares); PreparedRunIds.Enqueue(run.RunId); BeforePrepare?.Invoke(); Check("prepare"); return Task.FromResult<PreparedImageConversation>(new Conversation()); }
        public async Task WriteAndVerifyPromptAsync(PreparedImageConversation conversation, string prompt, CancellationToken token) { BeforeWrite?.Invoke(); WriteEntered.TrySetResult(); await HoldWrite.WaitAsync(token); Check("write"); }
        public Task SubmitOnceAsync(PreparedImageConversation conversation, CancellationToken token) { token.ThrowIfCancellationRequested(); Interlocked.Increment(ref Submissions); Check("submit"); return Task.CompletedTask; }
        public Task WaitForGeneratedImageAsync(PreparedImageConversation conversation, CancellationToken token) { Check("wait"); return Task.CompletedTask; }
        public Task<string> DownloadOriginalAsync(PreparedImageConversation conversation, string directory, CancellationToken token) { Check("download"); var path = Path.Combine(directory, "original.png"); File.WriteAllBytes(path, Png); return Task.FromResult(path); }
        public Task ClearOwnedDraftAsync(PreparedImageConversation conversation, string prompt, CancellationToken token) { BeforeCleanup?.Invoke(); return Cleanup.WaitAsync(token); }
        public JsonElement? ExportBinding(PreparedImageConversation conversation)
        {
            if (FailingBindingExports > 0) { FailingBindingExports--; throw Failure(); }
            return Binding;
        }
        public void Dispose() { }
    }
}
