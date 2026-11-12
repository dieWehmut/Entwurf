using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativeImageSourceSelectionTests
{
    static readonly NativeImageTarget Code = new(10, 7, DateTime.UnixEpoch);
    static readonly NativeImageTarget Images = new(20, 7, DateTime.UnixEpoch);

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void A_unique_verified_product_can_be_selected_with_another_app_foreground(bool reversed)
    {
        var surface = new WindowsImageSurface(); var reads = new List<NativeImageTarget>();
        NativeImageTarget[] targets = reversed ? [Images, Code] : [Code, Images];
        var probe = surface.ProbeSource(targets, 99, target =>
        {
            reads.Add(target);
            return target == Images ? new(true, "ready") : new(false, "chatgpt_mode_required");
        });
        Assert.True(probe.Available);
        Assert.Equal(targets, reads);
        Assert.Equal(Images, surface.DiscoverSource(targets, 0));
    }

    [Fact]
    public void Two_verified_surfaces_remain_ambiguous_and_are_not_cached()
    {
        var surface = new WindowsImageSurface();
        Assert.Equal("ambiguous_chatgpt_windows", Assert.Throws<ImageAutomationException>(() =>
            surface.ProbeSource([Code, Images], 99, _ => new(true, "ready"))).Reason);
        Assert.Throws<ImageAutomationException>(() => surface.DiscoverSource([Code, Images], 99));
    }

    [Fact]
    public void Package_identity_without_a_verified_product_does_not_select_a_source()
    {
        var surface = new WindowsImageSurface();
        var probe = surface.ProbeSource([Code, Images], 99, _ => new(false, "chatgpt_mode_required"));
        Assert.False(probe.Available);
        Assert.Equal("chatgpt_mode_required", probe.Reason);
        Assert.Throws<ImageAutomationException>(() => surface.DiscoverSource([Code, Images], 99));
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void An_unreadable_other_window_cannot_be_ignored_to_pin_the_remaining_window(bool throws)
    {
        var surface = new WindowsImageSurface();
        var probe = WindowsImageSurface.ReadProbe(() => surface.ProbeSource([Images, Code], 99, target =>
        {
            if (target == Images) return new(true, "ready");
            if (throws) throw new ImageAutomationException("image_surface_unavailable");
            return new(false, "image_surface_unavailable");
        }));
        Assert.False(probe.Available);
        Assert.Equal("image_surface_unavailable", probe.Reason);
        Assert.Throws<ImageAutomationException>(() => surface.DiscoverSource([Code, Images], 99));
    }

    [Fact]
    public void A_cached_window_that_changes_product_cannot_fall_back_to_another_window()
    {
        var surface = new WindowsImageSurface();
        Assert.True(surface.ProbeSource([Code, Images], Images.Handle, _ => new(true, "ready")).Available);
        var probe = surface.ProbeSource([Code, Images], 99, target =>
        {
            Assert.Equal(Images, target);
            return new(false, "chatgpt_mode_required");
        });
        Assert.False(probe.Available);
        Assert.Equal(Images, surface.DiscoverSource([Code, Images], 99));
    }
}
