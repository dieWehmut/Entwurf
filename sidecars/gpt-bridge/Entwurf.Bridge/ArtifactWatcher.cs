using System.Security.Cryptography;
using System.Text.RegularExpressions;

namespace Entwurf.Bridge;

/// <summary>Read-only recovery from explicitly parent-authorized directories. Never guesses ownership.</summary>
public sealed class ArtifactWatcher
{
    readonly string[] roots;
    readonly Func<string, FileSystemWatcher> createWatcher;
    public ArtifactWatcher(string[] authorizedDirectories) : this(authorizedDirectories, root => new FileSystemWatcher(root)) { }
    internal ArtifactWatcher(string[] authorizedDirectories, Func<string, FileSystemWatcher> createWatcher)
    {
        this.createWatcher = createWatcher;
        roots = authorizedDirectories.Select(Path.GetFullPath).Distinct(StringComparer.OrdinalIgnoreCase).ToArray();
        foreach (var root in roots) ValidateRoot(root);
    }
    static void ValidateRoot(string root)
    {
        for (var info = new DirectoryInfo(root); info is not null; info = info.Parent)
            if (!info.Exists || info.Attributes.HasFlag(FileAttributes.ReparsePoint)) throw new BridgeException("invalid_directory", "Artifact roots must exist and cannot traverse links.");
    }
    public async Task<Artifact> WatchAsync(string taskId, int revision, TimeSpan timeout, CancellationToken token)
    {
        if (roots.Length == 0) throw new BridgeException("artifact_directory_required", "The parent must authorize an import directory at bootstrap.");
        if (!Regex.IsMatch(taskId, "^[A-Za-z0-9_-]{1,80}$", RegexOptions.CultureInvariant) || revision < 1 || timeout <= TimeSpan.Zero || timeout > TimeSpan.FromMinutes(2))
            throw new BridgeException("invalid_params", "Invalid task identity or timeout.");
        foreach (var root in roots) ValidateRoot(root);
        var prefix = $"entwurf-task-{taskId}-v{revision}-image.";
        var baseline = roots.SelectMany(Directory.EnumerateFiles).ToHashSet(StringComparer.OrdinalIgnoreCase);
        using var changed = new SemaphoreSlim(0, 1);
        var watchers = new List<FileSystemWatcher>();
        void Signal() { try { changed.Release(); } catch (SemaphoreFullException) { } catch (ObjectDisposedException) { } }
        using var deadline = CancellationTokenSource.CreateLinkedTokenSource(token);
        deadline.CancelAfter(timeout);
        var stability = new Dictionary<string, (long Length, DateTime Written, DateTime Since)>(StringComparer.OrdinalIgnoreCase);
        try
        {
            foreach (var root in roots)
            {
                deadline.Token.ThrowIfCancellationRequested();
                var watcher = createWatcher(root);
                // Take ownership before any configuration/enabling step can throw.
                watchers.Add(watcher);
                watcher.NotifyFilter = NotifyFilters.FileName | NotifyFilters.LastWrite | NotifyFilters.Size;
                watcher.Created += (_, _) => Signal();
                watcher.Changed += (_, _) => Signal();
                watcher.Renamed += (_, _) => Signal();
                watcher.Error += (_, _) => Signal(); // Bounded rescans also handle watcher overflow.
                watcher.EnableRaisingEvents = true;
            }
            while (true)
            {
                deadline.Token.ThrowIfCancellationRequested();
                foreach (var root in roots) ValidateRoot(root);
                var candidates = roots.SelectMany(Directory.EnumerateFiles)
                    .Where(path => !baseline.Contains(path) && new[] { "png", "jpg", "jpeg", "webp" }
                        .Any(extension => string.Equals(Path.GetFileName(path), prefix + extension, StringComparison.OrdinalIgnoreCase))).ToArray();
                if (candidates.Length > 1) throw new BridgeException("artifact_ambiguous", "Multiple task-scoped files appeared; select one manually.");
                if (candidates.Length == 1)
                {
                    var path = candidates[0];
                    var info = new FileInfo(path);
                    if (info.Attributes.HasFlag(FileAttributes.ReparsePoint)) throw new BridgeException("invalid_artifact", "Linked artifacts are not accepted.");
                    if (info.Length > 32 * 1024 * 1024) throw new BridgeException("artifact_too_large", "Artifact exceeds 32 MiB.");
                    var now = DateTime.UtcNow;
                    if (!stability.TryGetValue(path, out var old) || old.Length != info.Length || old.Written != info.LastWriteTimeUtc)
                        stability[path] = (info.Length, info.LastWriteTimeUtc, now);
                    else if (info.Length > 0 && now - old.Since >= TimeSpan.FromMilliseconds(250))
                    {
                        try
                        {
                            await using var file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read, 65536, FileOptions.Asynchronous);
                            var hash = await SHA256.HashDataAsync(file, deadline.Token);
                            if (file.Length != old.Length) { stability.Remove(path); continue; }
                            return new Artifact(path, Convert.ToHexStringLower(hash), file.Length, "explicit_task_filename");
                        }
                        catch (IOException) { stability.Remove(path); }
                    }
                }
                await changed.WaitAsync(TimeSpan.FromMilliseconds(100), deadline.Token);
            }
        }
        catch (OperationCanceledException) when (!token.IsCancellationRequested) { throw new BridgeException("artifact_timeout", "No unique stable task artifact was found; choose the download manually."); }
        finally { foreach (var watcher in watchers) watcher.Dispose(); }
    }
}
