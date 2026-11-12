namespace Entwurf.Bridge;

internal static class NativeRuntimeIdentity
{
    internal static string Read(int[]? property, Func<int[]?> readNative, string unavailableReason)
    {
        // GetRuntimeId is the UIA client identity API. Some providers omit the
        // RuntimeId property while still supplying this same identity through it.
        var identity = property is { Length: > 0 } ? property : readNative();
        if (identity is not { Length: > 0 }) throw new ImageAutomationException(unavailableReason);
        return string.Join(".", identity);
    }
}
