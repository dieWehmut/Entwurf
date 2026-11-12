namespace Entwurf.Bridge;

internal sealed record NativeImageTarget(nint Handle, int ProcessId, DateTime Started);

// Owns preparation ordering; the adapter supplies only native discovery, capture and actions.
internal sealed class NativeImagePreparation
{
    public NativeImageTarget? Target { get; private set; }
    NativeImageTarget? source;
    IReadOnlyList<NativeImageTarget> before = [];
    bool creationAttempted;
    bool creationReturned;
    bool imagesInvoked;
    bool blankRefreshAttempted;
    string? documentId;

    internal void Begin(NativeImageTarget original, IReadOnlyList<NativeImageTarget> windows,
        Func<NativeImageTarget, NativeImageFrame> capture, Action<NativeImageTarget> createWindow, CancellationToken cancellation,
        Func<NativeImageTarget, bool>? verifiedImageHome = null)
    {
        cancellation.ThrowIfCancellationRequested();
        if (creationAttempted) throw new ImageAutomationException("independent_window_unavailable");
        if (!windows.Contains(original)) throw new ImageAutomationException("conversation_changed");
        var frame = capture(original);
        // The fallback verifies writable capabilities from this capture without reading the source draft.
        if (HasLogin(frame.Nodes) || (!HasMode(frame.Nodes) && verifiedImageHome?.Invoke(original) != true))
            throw new ImageAutomationException("chatgpt_mode_required");
        source = original;
        before = windows.ToArray();
        creationAttempted = true; // A thrown native call cannot justify a second window action.
        cancellation.ThrowIfCancellationRequested();
        createWindow(original);
        creationReturned = true;
    }
    internal NativeImageNode? Advance(IReadOnlyList<NativeImageTarget> windows, Func<NativeImageTarget, NativeImageFrame> capture,
        Func<NativeImageTarget, NativeImageNode, string?> readWritable, Action<NativeImageTarget, string> invoke, CancellationToken cancellation,
        Action<NativeImageTarget>? refreshBlankWindow = null)
    {
        cancellation.ThrowIfCancellationRequested();
        if (!creationReturned || source is null) throw new ImageAutomationException("independent_window_unavailable");
        if (Target is null)
        {
            var created = windows.Where(x => !before.Contains(x)).ToArray();
            if (created.Length == 0) return null;
            if (created.Length != 1 || before.Any(x => x.Handle == created[0].Handle)
                || created[0].ProcessId != source.ProcessId || created[0].Started != source.Started)
                throw new ImageAutomationException("independent_window_unavailable");
            Target = created[0];
        }
        if (!windows.Contains(Target)) throw new ImageAutomationException("conversation_changed");
        NativeImageFrame frame;
        try { frame = capture(Target); }
        catch (ImageAutomationException error) when (documentId is null && error.Reason == "image_surface_unavailable") { return null; }
        if (documentId is not null && frame.DocumentId != documentId) throw new ImageAutomationException("conversation_changed");
        var nodes = frame.Nodes;
        if (HasLogin(nodes)) throw new ImageAutomationException("chatgpt_login_required");
        if (NativeImagePage.HasTranscriptOrGeneration(nodes)) throw new ImageAutomationException("conversation_changed");
        // The observed new renderer can stall at only Document + empty root groups.
        // Refresh layout once on this independently-created HWND, never on a source
        // or a page already exposing controls/content. It still must load a fresh
        // empty chat and pass normal Images navigation before becoming writable.
        if (!imagesInvoked && !blankRefreshAttempted && refreshBlankWindow is not null && IsBlankDocument(frame))
        {
            blankRefreshAttempted = true;
            cancellation.ThrowIfCancellationRequested();
            refreshBlankWindow(Target);
            return null;
        }
        // Newer fresh windows omit the mode switch; Images navigation must be actionable before composer reads.
        var images = !imagesInvoked ? Button(nodes, "图像", "Images") : null;
        if (!imagesInvoked && images is null) return null;
        var edit = WritableEdit(nodes, node => readWritable(Target, node));
        if (edit is null) return null;
        if (readWritable(Target, edit)?.Length != 0) throw new ImageAutomationException("user_changed_composer");
        var imageHome = NativeImagePage.IsHome(nodes, edit.Id);
        if (!imagesInvoked)
        {
            // createFreshWindow('/') must expose an empty chat, not a carried Images session.
            if (imageHome) throw new ImageAutomationException("conversation_changed");
            documentId = frame.DocumentId;
            imagesInvoked = true;
            cancellation.ThrowIfCancellationRequested();
            invoke(Target, images!.Id);
            return null;
        }
        return imageHome ? edit : null;
    }
    static bool IsBlankDocument(NativeImageFrame frame) => !frame.HasAdditionalContent && frame.Nodes.Count is 2 or 3
        && frame.Nodes.Count(x => x.Id == frame.DocumentId && x.Role == "Document") == 1
        && frame.Nodes.All(x => x.Name.Length == 0 && x.Speaker is null && x.Visible && x.Enabled
            && (x.Id == frame.DocumentId ? x.Role == "Document" : x.Role == "Group"));
    internal static NativeImageTarget SelectSource(IReadOnlyList<NativeImageTarget> windows, NativeImageTarget? cached, nint foreground)
    {
        if (cached is not null) return windows.Contains(cached) ? cached : throw new ImageAutomationException("conversation_changed");
        if (windows.Count == 0) throw new ImageAutomationException("chatgpt_not_found");
        if (windows.Count == 1) return windows[0];
        var selected = windows.Where(x => x.Handle == foreground).ToArray();
        return selected.Length == 1 ? selected[0] : throw new ImageAutomationException("ambiguous_chatgpt_windows");
    }
    internal static NativeImageNode? Button(IReadOnlyList<NativeImageNode> nodes, string chinese, string english)
    {
        var matches = nodes.Where(x => x.Role == "Button" && x.Visible && x.Enabled && (x.Name == chinese || x.Name == english)).ToArray();
        return matches.Length == 1 ? matches[0] : null;
    }
    internal static bool HasMode(IReadOnlyList<NativeImageNode> nodes) => nodes.Any(x => x.Role == "Button" && x.Visible
        && x.Name is "切换模式，当前模式：ChatGPT" or "Switch mode, current mode: ChatGPT");
    internal static bool HasLogin(IReadOnlyList<NativeImageNode> nodes) => nodes.Any(x => x.Role == "Button" && x.Visible && x.Name is "登录" or "注册" or "Log in" or "Sign up");
    static NativeImageNode? WritableEdit(IReadOnlyList<NativeImageNode> nodes, Func<NativeImageNode, string?> readWritable)
    {
        var edits = nodes.Where(x => x.Role == "Edit" && x.Visible && x.Enabled && readWritable(x) is not null).ToArray();
        return edits.Length == 1 ? edits[0] : null;
    }
}
