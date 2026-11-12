namespace Entwurf.Bridge;

internal sealed record NativeWindowIdentity(nint Handle, int ProcessId, DateTime Started, bool TrustedPackage, bool Minimized = false);

// Bind each UIA mutation to a trusted window and process lifetime.
// Input-desktop state and foreground focus do not participate in that binding.
internal sealed class NativeImageWindowAccess(Func<nint, NativeWindowIdentity?> inspect)
{
    internal void Require(NativeImageTarget target, CancellationToken cancellation)
        => Read(target, cancellation);

    NativeWindowIdentity Read(NativeImageTarget target, CancellationToken cancellation)
    {
        cancellation.ThrowIfCancellationRequested();
        var current = inspect(target.Handle);
        if (current is null || !current.TrustedPackage || current.Handle != target.Handle
            || current.ProcessId != target.ProcessId || current.Started != target.Started)
            throw new ImageAutomationException("conversation_changed");
        cancellation.ThrowIfCancellationRequested();
        return current;
    }

    internal void Perform(NativeImageTarget target, CancellationToken cancellation, Action action)
    {
        if (Read(target, cancellation).Minimized) throw new ImageAutomationException("image_surface_unavailable");
        action();
        if (Read(target, cancellation).Minimized) throw new ImageAutomationException("image_surface_unavailable");
    }
}
