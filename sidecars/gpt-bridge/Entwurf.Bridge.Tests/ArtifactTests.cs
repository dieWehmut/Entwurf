using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class ArtifactTests : IAsyncLifetime
{
    readonly string directory = Path.Combine(Path.GetTempPath(), "entwurf-artifacts-" + Guid.NewGuid());
    const string TaskId = "bfe39c44-1ac0-4637-bc44-b2c3b958f966";
    readonly List<TrackedWatcher> watchers = [];
    public ArtifactTests() => Directory.CreateDirectory(directory);
    public Task InitializeAsync() => Task.CompletedTask;
    ArtifactWatcher CreateWatcher() => new([directory], root =>
    {
        var watcher = new TrackedWatcher(root);
        watchers.Add(watcher);
        return watcher;
    });
    string FileName(string extension = "png") => Path.Combine(directory, $"entwurf-task-{TaskId}-v1-image.{extension}");

    [Fact]
    public async Task Watch_returns_new_task_scoped_file_with_hash_after_stability()
    {
        var watcher = CreateWatcher();
        var pending = watcher.WatchAsync(TaskId, 1, TimeSpan.FromSeconds(3), default);
        await File.WriteAllBytesAsync(FileName(), [1,2,3]);
        var result = await pending;
        Assert.Equal(FileName(), result.Path);
        Assert.Equal(3, result.Bytes);
        Assert.Equal("039058c6f2c0cb492c533b0a4d14ef77cc0f78abccced5287d84a1a2011cfb81", result.Sha256);
    }

    [Fact]
    public async Task Preexisting_files_and_unrelated_names_do_not_get_assigned_to_task()
    {
        await File.WriteAllBytesAsync(FileName(), [1]);
        var watcher = CreateWatcher();
        var pending = watcher.WatchAsync(TaskId, 1, TimeSpan.FromMilliseconds(450), default);
        await File.WriteAllTextAsync(Path.Combine(directory, "download.png"), "new");
        Assert.Equal("artifact_timeout", (await Assert.ThrowsAsync<BridgeException>(async () => await pending)).Code);
    }

    [Fact]
    public async Task Multiple_candidates_require_manual_selection()
    {
        var pending = CreateWatcher().WatchAsync(TaskId, 1, TimeSpan.FromSeconds(3), default);
        await File.WriteAllBytesAsync(FileName("png"), [1]);
        await File.WriteAllBytesAsync(FileName("jpg"), [2]);
        Assert.Equal("artifact_ambiguous", (await Assert.ThrowsAsync<BridgeException>(async () => await pending)).Code);
    }

    [Fact]
    public async Task Filename_prefix_alone_does_not_prove_task_artifact_identity()
    {
        var pending = CreateWatcher().WatchAsync(TaskId, 1, TimeSpan.FromMilliseconds(450), default);
        await File.WriteAllBytesAsync(FileName("unrelated.png"), [1]);
        Assert.Equal("artifact_timeout", (await Assert.ThrowsAsync<BridgeException>(async () => await pending)).Code);
    }

    [Fact]
    public async Task Cancellation_interrupts_wait()
    {
        using var cancel = new CancellationTokenSource();
        var pending = CreateWatcher().WatchAsync(TaskId, 1, TimeSpan.FromSeconds(30), cancel.Token);
        cancel.Cancel();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(async () => await pending);
    }
    public async Task DisposeAsync()
    {
        // Detect missing production disposal even if the OS permits deleting an open directory.
        Assert.All(watchers, watcher => Assert.True(watcher.WasDisposed));
        await TemporaryDirectory.DeleteAsync(directory);
    }
    sealed class TrackedWatcher(string root) : FileSystemWatcher(root)
    {
        public bool WasDisposed { get; private set; }
        protected override void Dispose(bool disposing)
        {
            base.Dispose(disposing);
            if (disposing) WasDisposed = true;
        }
    }
}
