using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class ArtifactSetupTests : IDisposable
{
    readonly string directory = Path.Combine(Path.GetTempPath(), "entwurf-watcher-setup-" + Guid.NewGuid());
    readonly List<TrackedWatcher> created = [];
    public ArtifactSetupTests() => Directory.CreateDirectory(directory);

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task Later_watcher_setup_failure_disposes_all_previous_watchers(bool failWhenEnabling)
    {
        var first = Directory.CreateDirectory(Path.Combine(directory, "first")).FullName;
        var second = Directory.CreateDirectory(Path.Combine(directory, "second")).FullName;
        var watcher = new ArtifactWatcher([first, second], root =>
        {
            if (root == second && !failWhenEnabling) throw new DirectoryNotFoundException("Controlled later-root construction failure.");
            var current = new TrackedWatcher(root);
            created.Add(current);
            // Use a real OS watcher, deleting its own fixture root only after initial validation.
            if (root == second) Directory.Delete(second);
            return current;
        });
        var error = await Record.ExceptionAsync(async () => await watcher.WatchAsync("setup-test", 1, TimeSpan.FromSeconds(2), default));
        Assert.NotNull(error);
        Assert.Equal(failWhenEnabling ? 2 : 1, created.Count);
        Assert.All(created, item => Assert.True(item.WasDisposed));
    }

    sealed class TrackedWatcher(string root) : FileSystemWatcher(root)
    {
        public bool WasDisposed { get; private set; }
        protected override void Dispose(bool disposing)
        {
            if (disposing) WasDisposed = true;
            base.Dispose(disposing);
        }
    }
    public void Dispose()
    {
        foreach (var watcher in created) watcher.Dispose();
        Directory.Delete(directory, true);
    }
}
