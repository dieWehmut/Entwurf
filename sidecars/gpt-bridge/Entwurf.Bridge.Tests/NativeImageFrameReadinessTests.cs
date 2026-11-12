using System.Diagnostics;
using System.Runtime.InteropServices;
using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativeImageFrameReadinessTests
{
    const int StaleElement = unchecked((int)0x80040201);

    [Fact]
    public void A_stale_partial_snapshot_is_discarded_before_a_whole_frame_is_read_again()
    {
        var traversals = new List<List<NativeImageNode>>();
        var frame = NativeImageFrameReadiness.Read(() =>
        {
            var nodes = Nodes();
            traversals.Add(nodes);
            if (traversals.Count == 1)
            {
                nodes.Add(new("partial-image", "document", "Image", "已生成图像 1"));
                throw Unavailable();
            }
            nodes.Add(new("complete-image", "document", "Image", "已生成图像 1"));
            return NativeImageFrame.Capture(nodes);
        }, default);
        Assert.Equal(2, traversals.Count);
        Assert.NotSame(traversals[0], traversals[1]);
        Assert.Equal("document", frame.DocumentId);
        Assert.Contains(frame.Nodes, node => node.Id == "complete-image");
        Assert.DoesNotContain(frame.Nodes, node => node.Id == "partial-image");
    }

    [Theory]
    [InlineData(unchecked((int)0x80040200))]
    [InlineData(unchecked((int)0x80040204))]
    [InlineData(unchecked((int)0x80131505))]
    public void Other_UIA_failures_are_fatal_without_a_second_read(int code)
    {
        var reads = 0; var discarded = 0;
        var failure = new COMException("fixture non-stale provider failure", code);
        var error = Assert.Throws<COMException>(() => NativeImageFrameReadiness.Read<int>(() =>
        {
            reads++;
            throw failure;
        }, default, () => discarded++));
        Assert.Same(failure, error);
        Assert.Equal(1, reads);
        Assert.Equal(0, discarded);
    }

    [Fact]
    public void A_wrapped_stale_error_is_not_treated_as_the_observed_direct_COM_failure()
    {
        var reads = 0; var discarded = 0;
        var failure = new InvalidOperationException("fixture wrapped provider failure", Unavailable());
        var error = Assert.Throws<InvalidOperationException>(() => NativeImageFrameReadiness.Read<int>(() =>
        {
            reads++;
            throw failure;
        }, default, () => discarded++));
        Assert.Same(failure, error);
        Assert.Equal(1, reads);
        Assert.Equal(0, discarded);
    }

    [Fact]
    public void Missing_runtime_identity_is_fatal_without_retrying_or_omitting_the_node()
    {
        var reads = 0; var discarded = 0;
        var error = Assert.Throws<ImageAutomationException>(() => NativeImageFrameReadiness.Read(() =>
        {
            reads++;
            return NativeRuntimeIdentity.Read(null, () => null, "image_surface_unavailable");
        }, default, () => discarded++));
        Assert.Equal("image_surface_unavailable", error.Reason);
        Assert.Equal(1, reads);
        Assert.Equal(0, discarded);
    }

    [Fact]
    public void Repeated_stale_snapshots_stop_after_four_attempts_and_preserve_the_provider_error()
    {
        var reads = 0; var discarded = 0; COMException? lastFailure = null;
        var error = Assert.Throws<COMException>(() => NativeImageFrameReadiness.Read<int>(() =>
        {
            reads++;
            throw lastFailure = Unavailable();
        }, default, () => discarded++));
        Assert.Equal(4, reads);
        Assert.Equal(4, discarded);
        Assert.Same(lastFailure, error);
    }

    [Fact]
    public void An_expired_retry_window_prevents_another_attempt()
    {
        var reads = 0; var timeout = TimeSpan.FromMilliseconds(20); var elapsed = Stopwatch.StartNew();
        Assert.Throws<COMException>(() => NativeImageFrameReadiness.Read<int>(() =>
        {
            reads++;
            throw Unavailable();
        }, default, timeout: timeout));
        Assert.Equal(1, reads);
        Assert.True(elapsed.Elapsed >= timeout);
    }

    [Fact]
    public void A_valid_first_capture_can_finish_after_the_retry_window()
    {
        var reads = 0;
        var frame = NativeImageFrameReadiness.Read(() =>
        {
            reads++;
            Task.Delay(30).GetAwaiter().GetResult();
            return NativeImageFrame.Capture(Nodes());
        }, default, timeout: TimeSpan.FromMilliseconds(10));
        Assert.Equal(1, reads);
        Assert.Equal("document", frame.DocumentId);
    }

    [Fact]
    public void Cancellation_before_capture_does_not_read_the_provider()
    {
        using var cancellation = new CancellationTokenSource(); cancellation.Cancel();
        var reads = 0;
        Assert.ThrowsAny<OperationCanceledException>(() => NativeImageFrameReadiness.Read(() =>
        {
            reads++;
            return NativeImageFrame.Capture(Nodes());
        }, cancellation.Token));
        Assert.Equal(0, reads);
    }

    [Fact]
    public void Cancellation_during_a_complete_read_cannot_report_success()
    {
        using var cancellation = new CancellationTokenSource(); var reads = 0;
        Assert.ThrowsAny<OperationCanceledException>(() => NativeImageFrameReadiness.Read(() =>
        {
            reads++;
            cancellation.Cancel();
            return NativeImageFrame.Capture(Nodes());
        }, cancellation.Token));
        Assert.Equal(1, reads);
    }

    [Fact]
    public void Cancellation_after_discard_prevents_another_attempt()
    {
        using var cancellation = new CancellationTokenSource(); var reads = 0;
        Assert.ThrowsAny<OperationCanceledException>(() => NativeImageFrameReadiness.Read<int>(() =>
        {
            reads++;
            throw Unavailable();
        }, cancellation.Token, cancellation.Cancel));
        Assert.Equal(1, reads);
    }

    [Fact]
    public void Cancellation_interrupts_the_retry_wait_before_the_next_read()
    {
        using var cancellation = new CancellationTokenSource(); var reads = 0;
        Assert.ThrowsAny<OperationCanceledException>(() => NativeImageFrameReadiness.Read<int>(() =>
        {
            reads++;
            cancellation.CancelAfter(20);
            throw Unavailable();
        }, cancellation.Token));
        Assert.Equal(1, reads);
    }

    [Fact]
    public void A_discarded_frame_resets_stability_even_when_the_complete_signature_is_unchanged()
    {
        const string signature = "same-response-image";
        string? stableId = signature; var reads = 0; var discarded = 0;
        var observed = NativeImageFrameReadiness.Read(() =>
        {
            if (++reads == 1) throw Unavailable();
            return signature;
        }, default, () => { stableId = null; discarded++; });
        Assert.Equal(signature, observed);
        Assert.Null(stableId);
        Assert.Equal(1, discarded);
    }

    [Fact]
    public void A_replaced_process_during_retry_prevents_a_subsequent_native_action()
    {
        var target = new NativeImageTarget(10, 7, DateTime.UnixEpoch);
        var identity = new NativeWindowIdentity(10, 7, DateTime.UnixEpoch, true);
        var current = identity;
        var access = new NativeImageWindowAccess(_ => current);
        var reads = 0; var actions = 0;
        var error = Assert.Throws<ImageAutomationException>(() =>
        {
            NativeImageFrameReadiness.Read(() =>
            {
                access.Require(target, default);
                reads++;
                if (reads == 1)
                {
                    current = identity with { Started = DateTime.UnixEpoch.AddSeconds(1) };
                    throw Unavailable();
                }
                return NativeImageFrame.Capture(Nodes());
            }, default);
            access.Perform(target, default, () => actions++);
        });
        Assert.Equal("conversation_changed", error.Reason);
        Assert.Equal(1, reads);
        Assert.Equal(0, actions);
    }

    [Fact]
    public void A_changed_document_after_retry_prevents_a_subsequent_native_action()
    {
        var reads = 0; var actions = 0;
        var error = Assert.Throws<ImageAutomationException>(() =>
        {
            NativeImageFrameReadiness.Read(() =>
            {
                if (++reads == 1) throw Unavailable();
                var frame = NativeImageFrame.Capture(Nodes("replacement-document"));
                if (frame.DocumentId != "document") throw new ImageAutomationException("conversation_changed");
                return frame;
            }, default);
            actions++;
        });
        Assert.Equal("conversation_changed", error.Reason);
        Assert.Equal(2, reads);
        Assert.Equal(0, actions);
    }

    [Fact]
    public void A_stale_native_action_after_capture_is_not_retried()
    {
        var reads = 0; var actions = 0;
        var failure = Unavailable();
        var error = Assert.Throws<COMException>((Action)(() =>
        {
            NativeImageFrameReadiness.Read(() => { reads++; return NativeImageFrame.Capture(Nodes()); }, default);
            actions++;
            throw failure;
        }));
        Assert.Same(failure, error);
        Assert.Equal(1, reads);
        Assert.Equal(1, actions);
    }

    static COMException Unavailable() => new("fixture stale child element", StaleElement);
    static List<NativeImageNode> Nodes(string documentId = "document") =>
        [new("window", null, "Window", ""), new(documentId, "window", "Document", "")];
}
