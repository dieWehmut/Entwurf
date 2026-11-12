using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativeRuntimeIdentityTests
{
    [Fact]
    public void An_existing_property_identity_is_preserved_without_another_provider_read()
    {
        Assert.Equal("42.14813646.4.45.1.2", NativeRuntimeIdentity.Read([42, 14813646, 4, 45, 1, 2],
            () => throw new Exception("Do not replace an available identity"), "image_surface_unavailable"));
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void A_missing_property_can_use_only_the_native_client_identity(bool empty)
    {
        var reads = 0;
        Assert.Equal("42.854668", NativeRuntimeIdentity.Read(empty ? [] : null,
            () => { reads++; return [42, 854668]; }, "image_surface_unavailable"));
        Assert.Equal(1, reads);
    }

    [Theory]
    [InlineData(false, "image_surface_unavailable")]
    [InlineData(true, "image_surface_unavailable")]
    [InlineData(false, "independent_window_unavailable")]
    public void Missing_native_identity_never_produces_a_synthetic_node_id(bool empty, string reason)
    {
        Assert.Equal(reason, Assert.Throws<ImageAutomationException>(() =>
            NativeRuntimeIdentity.Read(null, () => empty ? [] : null, reason)).Reason);
    }
}
