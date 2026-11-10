using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativeImageWindowAccessTests
{
    static readonly NativeImageTarget Target = new(10, 7, DateTime.UnixEpoch);
    static readonly NativeWindowIdentity Identity = new(10, 7, DateTime.UnixEpoch, true);

    [Fact]
    public void A_stable_bound_window_permits_successive_pattern_operations()
    {
        var checks = 0;
        var access = new NativeImageWindowAccess(handle =>
        {
            Assert.Equal(Target.Handle, handle);
            checks++;
            return Identity;
        });
        var calls = 0;
        access.Perform(Target, default, () => calls++);
        access.Perform(Target, default, () => calls++);
        Assert.Equal(2, calls);
        Assert.Equal(4, checks);
    }

    [Theory]
    [InlineData("missing")]
    [InlineData("handle")]
    [InlineData("process")]
    [InlineData("start")]
    [InlineData("publisher")]
    public void A_replaced_or_untrusted_window_is_refused_before_a_pattern_mutation(string change)
    {
        var access = new NativeImageWindowAccess(_ => Changed(change));
        var calls = 0;
        var error = Assert.Throws<ImageAutomationException>(() => access.Perform(Target, default, () => calls++));
        Assert.Equal("conversation_changed", error.Reason);
        Assert.Equal(0, calls);
    }

    [Fact]
    public void Window_replacement_during_a_native_call_is_reported_without_replaying_the_call()
    {
        NativeWindowIdentity? current = Identity;
        var access = new NativeImageWindowAccess(_ => current);
        var calls = 0;
        var error = Assert.Throws<ImageAutomationException>(() => access.Perform(Target, default, () =>
        {
            calls++;
            current = Identity with { Started = DateTime.UnixEpoch.AddSeconds(1) };
        }));
        Assert.Equal("conversation_changed", error.Reason);
        Assert.Equal(1, calls);
        Assert.Throws<ImageAutomationException>(() => access.Perform(Target, default, () => calls++));
        Assert.Equal(1, calls);
    }

    [Fact]
    public void Cancellation_prevents_native_calls_and_does_not_read_a_new_window()
    {
        using var cancellation = new CancellationTokenSource();
        cancellation.Cancel();
        var access = new NativeImageWindowAccess(_ => throw new Exception("Cancelled operation must not inspect"));
        Assert.ThrowsAny<OperationCanceledException>(() => access.Perform(Target, cancellation.Token, () => throw new Exception("Must not mutate")));
    }

    [Fact]
    public void Cancellation_during_window_validation_prevents_the_pattern_mutation()
    {
        using var cancellation = new CancellationTokenSource();
        var access = new NativeImageWindowAccess(_ =>
        {
            cancellation.Cancel();
            return Identity;
        });
        var calls = 0;
        Assert.ThrowsAny<OperationCanceledException>(() => access.Perform(Target, cancellation.Token, () => calls++));
        Assert.Equal(0, calls);
    }

    [Fact]
    public void Cancellation_during_a_pattern_call_is_observed_after_one_mutation()
    {
        using var cancellation = new CancellationTokenSource();
        var access = new NativeImageWindowAccess(_ => Identity);
        var calls = 0;
        Assert.ThrowsAny<OperationCanceledException>(() => access.Perform(Target, cancellation.Token, () =>
        {
            calls++;
            cancellation.Cancel();
        }));
        Assert.Equal(1, calls);
    }

    [Fact]
    public void An_uncertain_native_call_is_not_retried_by_the_window_access_boundary()
    {
        var access = new NativeImageWindowAccess(_ => Identity);
        var calls = 0;
        Assert.Throws<InvalidOperationException>(() => access.Perform(Target, default, () =>
        {
            calls++;
            throw new InvalidOperationException();
        }));
        Assert.Equal(1, calls);
    }

    static NativeWindowIdentity? Changed(string change) => change switch
    {
        "missing" => null,
        "handle" => Identity with { Handle = 20 },
        "process" => Identity with { ProcessId = 8 },
        "start" => Identity with { Started = DateTime.UnixEpoch.AddSeconds(1) },
        _ => Identity with { TrustedPackage = false }
    };
}
