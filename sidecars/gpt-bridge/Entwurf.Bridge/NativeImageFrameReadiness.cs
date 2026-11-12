using System.Diagnostics;
using System.Runtime.InteropServices;

namespace Entwurf.Bridge;

internal static class NativeImageFrameReadiness
{
    const int ElementNotAvailable = unchecked((int)0x80040201);
    const int MaximumAttempts = 4;
    static readonly TimeSpan MaximumRetryWindow = TimeSpan.FromSeconds(3);

    internal static T Read<T>(Func<T> read, CancellationToken cancellation,
        Action? onDiscard = null, TimeSpan? timeout = null)
    {
        ArgumentNullException.ThrowIfNull(read);
        var retryWindow = timeout ?? MaximumRetryWindow;
        if (retryWindow <= TimeSpan.Zero) throw new ArgumentOutOfRangeException(nameof(timeout));
        if (retryWindow > MaximumRetryWindow) retryWindow = MaximumRetryWindow;
        var elapsed = Stopwatch.StartNew();
        for (var attempt = 0; ; attempt++)
        {
            cancellation.ThrowIfCancellationRequested();
            try
            {
                var frame = read();
                cancellation.ThrowIfCancellationRequested();
                return frame;
            }
            catch (COMException error) when (error.HResult == ElementNotAvailable)
            {
                // The caller must rebuild a complete frame on the next read.
                // A discarded observation cannot contribute to image stability.
                onDiscard?.Invoke();
                cancellation.ThrowIfCancellationRequested();
                if (attempt + 1 >= MaximumAttempts || elapsed.Elapsed >= retryWindow) throw;

                var remaining = retryWindow - elapsed.Elapsed;
                var waitMilliseconds = (int)Math.Ceiling(Math.Min(100, remaining.TotalMilliseconds));
                if (waitMilliseconds <= 0) throw;
                cancellation.WaitHandle.WaitOne(waitMilliseconds);
                cancellation.ThrowIfCancellationRequested();
                // This budget limits retry starts. A valid provider read that
                // started in time may take longer and still return its frame.
                if (elapsed.Elapsed >= retryWindow) throw;
            }
        }
    }
}
