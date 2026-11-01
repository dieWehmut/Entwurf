using System.Diagnostics;

namespace Entwurf.Bridge.Tests;

internal static class TemporaryDirectory
{
    public static async Task DeleteAsync(string path, TimeSpan? timeout = null)
    {
        var elapsed = Stopwatch.StartNew();
        var limit = timeout ?? TimeSpan.FromSeconds(2);
        while (true)
        {
            try { Directory.Delete(path, true); return; }
            // Call only after asserting owned watchers/children have stopped. Windows can still
            // have a transient directory handle open. Retry only ERROR_SHARING_VIOLATION;
            // a retained handle fails with the original IOException once the deadline expires.
            catch (IOException error) when (OperatingSystem.IsWindows() && (error.HResult & 0xffff) == 32 && elapsed.Elapsed < limit)
            {
                await Task.Delay(TimeSpan.FromMilliseconds(10));
            }
        }
    }
}
