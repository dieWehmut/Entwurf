namespace Entwurf.Bridge;

internal sealed record NativeImageWindowBounds(int Left, int Top, int Width, int Height);

internal static class NativeImageWindowLayout
{
    // Both mutations keep activation and Z order unchanged. The caller supplies
    // the actual Win32 calls; every resize/restore uses the trusted identity guard.
    internal const uint PositionFlags = 0x0004 | 0x0010 | 0x0200;

    internal static void Refresh(NativeImageTarget target, NativeImageWindowAccess access,
        Func<NativeImageTarget, NativeImageWindowBounds> readBounds,
        Action<NativeImageTarget, NativeImageWindowBounds> setBounds,
        Action<CancellationToken> settle, CancellationToken cancellation)
    {
        access.Require(target, cancellation);
        var original = readBounds(target);
        access.Require(target, cancellation);
        if (original.Width <= 0 || original.Width > int.MaxValue - 32 || original.Height <= 0)
            throw new ImageAutomationException("image_surface_unavailable");
        var dispatched = false;
        try
        {
            access.Perform(target, cancellation, () =>
            {
                dispatched = true; // A failed native call can still have changed bounds.
                setBounds(target, original with { Width = original.Width + 32 });
            });
            settle(cancellation);
        }
        finally
        {
            if (dispatched)
            {
                // Cancellation ends preparation but cannot skip restoration of a
                // still-trusted non-minimized window that this call resized.
                access.Perform(target, CancellationToken.None, () => setBounds(target, original));
                access.Require(target, CancellationToken.None);
                var restored = readBounds(target);
                access.Require(target, CancellationToken.None);
                if (restored != original) throw new ImageAutomationException("image_surface_unavailable");
            }
        }
    }
}
