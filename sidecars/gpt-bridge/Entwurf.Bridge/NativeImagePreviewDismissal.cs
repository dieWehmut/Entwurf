using System.Diagnostics;

namespace Entwurf.Bridge;

// Only reads after a single close dispatch. A missing transcript during dismissal
// cannot authorize a new user binding or another native action.
internal sealed class NativeImagePreviewDismissal(string documentId, string previewRootId,
    IReadOnlySet<string> before, string promptSha256, string? userId, string? userHeadingId)
{
    static readonly TimeSpan MaximumWaitWindow = TimeSpan.FromSeconds(4);

    internal NativeImageNode? ReadPreview(NativeImageFrame frame)
    {
        if (userId is null && userHeadingId is null) throw new ImageAutomationException("submission_uncertain");
        if (frame.DocumentId != documentId) throw new ImageAutomationException("conversation_changed");
        ValidateKnownUserEvidence(frame.Nodes);
        _ = FindRestoredTurn(frame.Nodes); // Reject already available ambiguous bindings before close.
        var previews = frame.Nodes.Where(x => x.Visible && x.Role is "Group" or "Window"
            && NativeImageEvidence.IsPreviewDialog(x.Name)).ToArray();
        if (previews.Any(x => x.Id != previewRootId)) throw new ImageAutomationException("conversation_changed");
        var owned = frame.Nodes.SingleOrDefault(x => x.Id == previewRootId && x.Visible);
        if (owned is not null && (owned.Role is not ("Group" or "Window") || !NativeImageEvidence.IsPreviewDialog(owned.Name)))
            throw new ImageAutomationException("conversation_changed");
        return owned;
    }

    internal async Task<NativeUserTurn> WaitAsync(Func<TimeSpan, NativeImageFrame> readBoundFrame,
        string? dispatchedCloseId, CancellationToken cancellation, TimeSpan? timeout = null, TimeSpan? pollInterval = null)
    {
        var budget = timeout ?? MaximumWaitWindow;
        var interval = pollInterval ?? TimeSpan.FromMilliseconds(100);
        if (budget <= TimeSpan.Zero) throw new ArgumentOutOfRangeException(nameof(timeout));
        if (interval <= TimeSpan.Zero) throw new ArgumentOutOfRangeException(nameof(pollInterval));
        if (budget > MaximumWaitWindow) budget = MaximumWaitWindow;
        var previewGone = dispatchedCloseId is null;
        var elapsed = Stopwatch.StartNew();
        while (elapsed.Elapsed < budget)
        {
            cancellation.ThrowIfCancellationRequested();
            // Like complete-frame capture, the budget limits read/retry starts;
            // a complete provider read started in time may finish later.
            var readBudget = budget - elapsed.Elapsed;
            if (readBudget <= TimeSpan.Zero) break;
            var frame = readBoundFrame(readBudget);
            cancellation.ThrowIfCancellationRequested();
            var preview = ReadPreview(frame);
            if (preview is not null)
            {
                if (previewGone) throw new ImageAutomationException("conversation_changed");
                var map = frame.Nodes.ToDictionary(x => x.Id, StringComparer.Ordinal);
                var close = frame.Nodes.Where(x => x.Role == "Button" && x.Visible && NativeImageEvidence.IsPreviewClose(x.Name)
                    && NativeImageEvidence.IsWithin(x.Id, previewRootId, map)).ToArray();
                // The dispatched control can become disabled or disappear during
                // closing. A second or replacement close is different evidence.
                if (close.Length > 1 || (close.Length == 1 && close[0].Id != dispatchedCloseId))
                    throw new ImageAutomationException("conversation_changed");
            }
            else previewGone = true;

            var own = FindRestoredTurn(frame.Nodes);
            if (previewGone && own is not null) return own;
            var remaining = budget - elapsed.Elapsed;
            if (remaining <= TimeSpan.Zero) break;
            await Task.Delay(interval < remaining ? interval : remaining, cancellation);
        }
        cancellation.ThrowIfCancellationRequested();
        throw new ImageAutomationException("submission_uncertain");
    }

    void ValidateKnownUserEvidence(IReadOnlyList<NativeImageNode> nodes)
    {
        var text = userId is null ? null : nodes.SingleOrDefault(x => x.Id == userId);
        if (text is not null && (text.Role != "Text" || before.Contains(text.Id) || !NativeImageEvidence.MatchesHash(text.Name, promptSha256)))
            throw new ImageAutomationException("conversation_changed");
        if (userHeadingId is not null)
        {
            var heading = nodes.SingleOrDefault(x => x.Id == userHeadingId);
            if ((heading is not null && (heading.Speaker != "user" || before.Contains(heading.Id)))
                || nodes.Any(x => x.Speaker == "user" && !before.Contains(x.Id) && x.Id != userHeadingId))
                throw new ImageAutomationException("conversation_changed");
        }
    }

    NativeUserTurn? FindRestoredTurn(IReadOnlyList<NativeImageNode> nodes)
    {
        // The exact original text can reappear before its h4 fragment. It is
        // still pending until the original heading is available for validation.
        if (userHeadingId is not null && !nodes.Any(x => x.Id == userHeadingId)) return null;
        var map = nodes.ToDictionary(x => x.Id, StringComparer.Ordinal);
        var promptPresent = nodes.Any(x => x.Role == "Text" && !before.Contains(x.Id)
            && NativeImageEvidence.MatchesHash(x.Name, promptSha256)
            && !nodes.Any(edit => edit.Role == "Edit" && NativeImageEvidence.IsWithin(x.Id, edit.Id, map)));
        if (!promptPresent) return null;
        // Preserve the strict original heading (v2) or text ID (v1) semantics.
        return NativeImageEvidence.FindUserTurnByHash(nodes, before, promptSha256, userId, userHeadingId);
    }
}
