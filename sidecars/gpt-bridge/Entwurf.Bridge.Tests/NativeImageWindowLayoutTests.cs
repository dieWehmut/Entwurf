using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativeImageWindowLayoutTests
{
    static readonly NativeImageTarget Target = new(20, 7, DateTime.UnixEpoch);
    static readonly NativeWindowIdentity Trusted = new(20, 7, DateTime.UnixEpoch, true);
    static readonly NativeImageWindowBounds Original = new(33, 51, 480, 600);

    [Fact]
    public void Refresh_restores_the_exact_original_bounds_after_one_settling_interval()
    {
        var window = new Window();
        window.OnSettle = _ => Assert.Equal(Original with { Width = 512 }, window.Bounds);
        window.Refresh();
        Assert.Equal(new[] { Original with { Width = 512 }, Original }, window.Mutations);
        Assert.Equal(Original, window.Bounds);
        Assert.Equal(1, window.Settles);
    }

    [Fact]
    public void Cancellation_during_settling_still_restores_the_owned_window()
    {
        var window = new Window();
        using var cancellation = new CancellationTokenSource();
        window.OnSettle = token => { cancellation.Cancel(); token.ThrowIfCancellationRequested(); };
        Assert.ThrowsAny<OperationCanceledException>(() => window.Refresh(cancellation.Token));
        Assert.Equal(new[] { Original with { Width = 512 }, Original }, window.Mutations);
        Assert.Equal(Original, window.Bounds);
    }

    [Fact]
    public void An_uncertain_resize_still_attempts_exact_guarded_restoration()
    {
        var window = new Window();
        window.OnSet = bounds => { if (bounds != Original) throw new InvalidOperationException(); };
        Assert.Throws<InvalidOperationException>(() => window.Refresh());
        Assert.Equal(new[] { Original with { Width = 512 }, Original }, window.Mutations);
        Assert.Equal(Original, window.Bounds);
        Assert.Equal(0, window.Settles);
    }

    [Fact]
    public void A_minimized_window_receives_no_resize_or_restoration()
    {
        var window = new Window { Identity = Trusted with { Minimized = true } };
        Assert.Equal("image_surface_unavailable", Assert.Throws<ImageAutomationException>(() => window.Refresh()).Reason);
        Assert.Empty(window.Mutations);
        Assert.Equal(0, window.Settles);
    }

    [Theory]
    [InlineData("minimized")]
    [InlineData("replaced")]
    [InlineData("untrusted")]
    public void A_changed_window_after_resize_cannot_receive_a_restoration_call(string condition)
    {
        var window = new Window();
        window.OnSet = _ => window.Identity = condition switch
        {
            "minimized" => Trusted with { Minimized = true },
            "replaced" => Trusted with { Started = DateTime.UnixEpoch.AddSeconds(1) },
            _ => Trusted with { TrustedPackage = false }
        };
        Assert.Equal(condition == "minimized" ? "image_surface_unavailable" : "conversation_changed",
            Assert.Throws<ImageAutomationException>(() => window.Refresh()).Reason);
        Assert.Equal(new[] { Original with { Width = 512 } }, window.Mutations);
        Assert.Equal(0, window.Settles);
    }

    [Fact]
    public void A_successful_restore_return_without_matching_bounds_is_not_accepted()
    {
        var window = new Window();
        window.OnSet = bounds => { if (bounds == Original) window.Bounds = Original with { Width = 481 }; };
        Assert.Equal("image_surface_unavailable", Assert.Throws<ImageAutomationException>(() => window.Refresh()).Reason);
        Assert.Equal(2, window.Mutations.Count);
    }

    [Fact]
    public void Cancellation_before_refresh_never_reads_or_mutates_the_window()
    {
        var window = new Window();
        using var cancellation = new CancellationTokenSource();
        cancellation.Cancel();
        Assert.ThrowsAny<OperationCanceledException>(() => window.Refresh(cancellation.Token));
        Assert.Equal(0, window.Reads);
        Assert.Empty(window.Mutations);
    }

    [Theory]
    [InlineData(0)]
    [InlineData(-1)]
    [InlineData(int.MaxValue)]
    public void Invalid_original_dimensions_never_authorize_native_mutations(int width)
    {
        var window = new Window { Bounds = Original with { Width = width } };
        Assert.Equal("image_surface_unavailable", Assert.Throws<ImageAutomationException>(() => window.Refresh()).Reason);
        Assert.Empty(window.Mutations);
    }

    sealed class Window
    {
        public NativeWindowIdentity? Identity = Trusted;
        public NativeImageWindowBounds Bounds = Original;
        public List<NativeImageWindowBounds> Mutations = [];
        public Action<NativeImageWindowBounds>? OnSet;
        public Action<CancellationToken>? OnSettle;
        public int Reads, Settles;

        public void Refresh(CancellationToken cancellation = default)
            => NativeImageWindowLayout.Refresh(Target, new NativeImageWindowAccess(handle =>
            {
                Assert.Equal(Target.Handle, handle);
                return Identity;
            }), target =>
            {
                Assert.Equal(Target, target);
                Reads++;
                return Bounds;
            }, (target, bounds) =>
            {
                Assert.Equal(Target, target);
                Mutations.Add(bounds);
                Bounds = bounds;
                OnSet?.Invoke(bounds);
            }, token => { Settles++; OnSettle?.Invoke(token); }, cancellation);
    }
}
