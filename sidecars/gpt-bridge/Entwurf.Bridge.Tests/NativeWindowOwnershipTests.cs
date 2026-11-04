using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativeWindowOwnershipTests
{
    [Fact]
    public void A_save_dialog_from_another_same_process_window_is_refused()
    {
        Assert.False(WindowsImageSurface.OwnedSaveDialog(30, 10, window => window == 30 ? 20 : 0));
    }

    [Fact]
    public void A_save_dialog_owned_indirectly_by_the_bound_window_is_accepted()
    {
        Assert.True(WindowsImageSurface.OwnedSaveDialog(30, 10, window => window == 30 ? 20 : window == 20 ? 10 : 0));
    }

    [Fact]
    public void A_cyclic_owner_chain_is_refused()
    {
        Assert.False(WindowsImageSurface.OwnedSaveDialog(30, 10, window => window == 30 ? 20 : 30));
    }

    [Fact]
    public void An_unowned_same_process_save_dialog_is_refused()
    {
        Assert.False(WindowsImageSurface.OwnedSaveDialog(30, 10, _ => 0));
    }
}
