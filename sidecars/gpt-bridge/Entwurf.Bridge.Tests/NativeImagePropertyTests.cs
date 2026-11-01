using Entwurf.Bridge;
using FlaUI.Core;
using FlaUI.Core.Exceptions;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativeImagePropertyTests
{
    [Theory]
    [InlineData(null, true, false, true)]
    [InlineData(false, null, true, false)]
    [InlineData(null, null, false, false)]
    [InlineData(false, true, true, true)]
    [InlineData(true, true, false, true)]
    [InlineData(false, false, true, false)]
    public void Missing_UIA_properties_never_authorize_visibility_or_input(bool? offscreen, bool? enabled, bool visible, bool interactive)
    {
        var observed = WindowsImageSurface.ReadAvailability(new OptionalProperty(offscreen), new OptionalProperty(enabled));
        Assert.Equal(visible, observed.Visible);
        Assert.Equal(interactive, observed.Enabled);
    }

    [Fact]
    public void Unsupported_required_provider_metadata_reports_unavailable_instead_of_escaping_the_probe()
    {
        var result = WindowsImageSurface.ReadProbe(() => throw new PropertyNotSupportedException("fixture UIA NotSupported", null!));
        Assert.False(result.Available);
        Assert.Equal("image_surface_unavailable", result.Reason);
    }

    [Fact]
    public void Probe_cancellation_remains_cancellation()
    {
        Assert.Throws<OperationCanceledException>(() => WindowsImageSurface.ReadProbe(() => throw new OperationCanceledException()));
    }

    // Matches the live provider's Window node: IsOffscreen returns UIA NotSupported.
    sealed class OptionalProperty(bool? value) : IAutomationProperty<bool>
    {
        public bool Value => value ?? throw new PropertyNotSupportedException("fixture UIA NotSupported", null!);
        public bool ValueOrDefault => value ?? false;
        public bool IsSupported => value.HasValue;
        public bool TryGetValue(out bool result) { result = ValueOrDefault; return IsSupported; }
    }
}
