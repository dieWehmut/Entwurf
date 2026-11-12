using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.RegularExpressions;
using System.Text.Json;
using FlaUI.Core;
using FlaUI.Core.AutomationElements;
using FlaUI.Core.Definitions;
using FlaUI.Core.Exceptions;
using FlaUI.UIA3;

namespace Entwurf.Bridge;

/// <summary>Independent UIA adapter for the existing signed-in Windows ChatGPT application. No browser/profile/token access.</summary>
public sealed class WindowsImageSurface : IImageAutomationSurface
{
    readonly object uiGate = new();
    readonly NativeImageWindowAccess windowAccess = new(ReadWindowIdentity);
    UIA3Automation? automation;
    NativeImageTarget? sourceTarget;
    sealed record Frame(List<NativeImageNode> Nodes, Dictionary<string, AutomationElement> Elements, string DocumentId);
    sealed record Binding(int Version, long Window, int ProcessId, long ProcessStartedUtcTicks, string DocumentId,
        string EditId, string[] BeforeIds, string PromptSha256, string? UserId, NativeGeneratedImage? Result, string? PreviewRootId);
    sealed class Conversation(NativeImageTarget target, string documentId, string editId, HashSet<string> before, string? prompt, string promptSha256) : PreparedImageConversation
    {
        public NativeImageTarget Target { get; } = target;
        public string DocumentId { get; } = documentId;
        public string EditId { get; } = editId;
        public HashSet<string> Before { get; } = before;
        public string? Prompt { get; } = prompt;
        public string PromptSha256 { get; } = promptSha256;
        public bool Submitted;
        public string? UserId;
        public NativeGeneratedImage? Result;
        public string? PreviewRootId;
    }

    public Task<ImageSurfaceProbe> ProbeAsync(CancellationToken cancellation) => Task.Run(() => ReadProbe(() =>
    {
        cancellation.ThrowIfCancellationRequested();
        lock (uiGate)
        {
            return ProbeSource(DiscoverTargets(), GetForegroundWindow(), target => ProbeTarget(target, cancellation));
        }
    }), cancellation);

    internal static ImageSurfaceProbe ReadProbe(Func<ImageSurfaceProbe> read)
    {
        try { return read(); }
        catch (ImageAutomationException error) { return new ImageSurfaceProbe(false, error.Reason); }
        catch (Exception error) when (error is COMException or InvalidOperationException or TimeoutException or FlaUIException)
        { return new ImageSurfaceProbe(false, "image_surface_unavailable"); }
    }

    public async Task<PreparedImageConversation> PrepareAsync(ImageAutomationRun run, CancellationToken cancellation)
    {
        var preparation = new NativeImagePreparation();
        var frames = new Dictionary<NativeImageTarget, Frame>();
        NativeImageFrame ReadFrame(NativeImageTarget target)
        {
            var frame = Capture(target, cancellation);
            frames[target] = frame;
            return new(frame.Nodes, frame.DocumentId);
        }
        lock (uiGate)
        {
            cancellation.ThrowIfCancellationRequested();
            var source = Discover(cancellation);
            preparation.Begin(source, DiscoverTargets(), ReadFrame, target =>
            {
                sourceTarget = target;
                OpenNewWindow(target, cancellation);
            }, cancellation, target => ImageHome(frames[target]));
        }
        var deadline = Stopwatch.StartNew();
        while (deadline.Elapsed < TimeSpan.FromSeconds(35))
        {
            cancellation.ThrowIfCancellationRequested();
            lock (uiGate)
            {
                var edit = preparation.Advance(DiscoverTargets(), target =>
                {
                    RequireWindow(target, cancellation);
                    return ReadFrame(target);
                }, (target, node) =>
                {
                    var element = frames[target].Elements[node.Id];
                    return element.Patterns.Value.IsSupported && !element.Patterns.Value.Pattern.IsReadOnly.Value ? ReadValue(element) : null;
                }, (target, id) =>
                {
                    RequireWindow(target, cancellation);
                    Invoke(target, frames[target].Elements[id], cancellation);
                }, cancellation);
                if (edit is not null)
                {
                    var target = preparation.Target!;
                    var frame = frames[target];
                    return new Conversation(target, frame.DocumentId, edit.Id, frame.Nodes.Select(x => x.Id).ToHashSet(StringComparer.Ordinal), run.Prompt, run.PromptSha256);
                }
            }
            await Task.Delay(500, cancellation);
        }
        throw new ImageAutomationException(preparation.Target is null ? "independent_window_unavailable" : "image_surface_unavailable");
    }

    public async Task WriteAndVerifyPromptAsync(PreparedImageConversation conversation, string prompt, CancellationToken cancellation)
    {
        var run = Cast(conversation);
        lock (uiGate)
        {
            var frame = BeforeSubmit(run, cancellation);
            var edit = frame.Elements[run.EditId];
            if (ReadValue(edit).Length != 0 || prompt != run.Prompt) throw new ImageAutomationException("user_changed_composer");
            RequireWindow(run.Target, cancellation);
            SetValue(run.Target, edit, prompt, cancellation);
            if (ReadValue(edit) != prompt) throw new ImageAutomationException("user_changed_composer");
        }
        var settled = Stopwatch.StartNew();
        while (settled.Elapsed < TimeSpan.FromSeconds(4))
        {
            await Task.Delay(150, cancellation);
            lock (uiGate)
            {
                var frame = BeforeSubmit(run, cancellation);
                if (ReadValue(frame.Elements[run.EditId]) != prompt) throw new ImageAutomationException("user_changed_composer");
                var parent = frame.Nodes.Single(x => x.Id == run.EditId).ParentId;
                if (frame.Nodes.Count(x => x.ParentId == parent && x.Role == "Button" && x.Visible && x.Enabled
                    && x.Name is "发送" or "Send" or "Send prompt" or "Send message") == 1) return;
            }
        }
        throw new ImageAutomationException("image_surface_unavailable");
    }

    public Task SubmitOnceAsync(PreparedImageConversation conversation, CancellationToken cancellation)
    {
        var run = Cast(conversation);
        lock (uiGate)
        {
            if (run.Submitted) throw new ImageAutomationException("submission_uncertain");
            var frame = BeforeSubmit(run, cancellation);
            if (ReadValue(frame.Elements[run.EditId]) != run.Prompt) throw new ImageAutomationException("user_changed_composer");
            var parent = frame.Nodes.Single(x => x.Id == run.EditId).ParentId;
            var send = frame.Nodes.Where(x => x.ParentId == parent && x.Role == "Button" && x.Visible && x.Enabled && x.Name is "发送" or "Send" or "Send prompt" or "Send message").ToArray();
            if (send.Length != 1) throw new ImageAutomationException("image_surface_unavailable");
            RequireWindow(run.Target, cancellation);
            windowAccess.Perform(run.Target, cancellation, () =>
            {
                run.Submitted = true; // An ambiguous native call can never trigger a second Invoke.
                InvokePattern(frame.Elements[send[0].Id], cancellation);
            });
        }
        return Task.CompletedTask;
    }

    public async Task WaitForGeneratedImageAsync(PreparedImageConversation conversation, CancellationToken cancellation)
    {
        var run = Cast(conversation);
        var elapsed = Stopwatch.StartNew();
        string? stableId = null;
        var stableSince = TimeSpan.Zero;
        while (elapsed.Elapsed < TimeSpan.FromMinutes(8))
        {
            cancellation.ThrowIfCancellationRequested();
            lock (uiGate)
            {
                RequireWindow(run.Target, cancellation);
                var frame = Capture(run.Target, cancellation, run);
                if (frame.DocumentId != run.DocumentId || HasLogin(frame)) throw new ImageAutomationException("conversation_changed");
                var ownUsers = frame.Nodes.Where(x => x.Role == "Text" && !run.Before.Contains(x.Id) && NativeImageEvidence.MatchesHash(x.Name, run.PromptSha256)).ToArray();
                if (run.UserId is null && ownUsers.Length == 1) run.UserId = ownUsers[0].Id;
                if (run.UserId is null && elapsed.Elapsed > TimeSpan.FromSeconds(20)) throw new ImageAutomationException("conversation_changed");
                var result = NativeImageEvidence.FindByHash(frame.Nodes, run.Before, run.PromptSha256, run.UserId);
                var generating = frame.Nodes.Any(x => x.Visible && NativeImageEvidence.IsGenerating(x.Name));
                if (result is not null && !generating)
                {
                    var image = frame.Nodes.Single(x => x.Id == result.ImageId);
                    var signature = result.ImageId + ":" + result.ActionId + ":" + image.Width + "x" + image.Height;
                    if (stableId != signature) { stableId = signature; stableSince = elapsed.Elapsed; }
                    else if (elapsed.Elapsed - stableSince >= TimeSpan.FromSeconds(2)) { run.Result = result; return; }
                }
                else stableId = null;
            }
            await Task.Delay(500, cancellation);
        }
        throw new ImageAutomationException("generation_timeout");
    }

    public async Task<string> DownloadOriginalAsync(PreparedImageConversation conversation, string ownedDirectory, CancellationToken cancellation)
    {
        var run = Cast(conversation);
        NativeGeneratedImage expected = run.Result ?? throw new ImageAutomationException("download_unavailable");
        HashSet<string> beforePreview;
        lock (uiGate)
        {
            RequireWindow(run.Target, cancellation);
            var frame = Capture(run.Target, cancellation, run);
            var current = NativeImageEvidence.FindByHash(frame.Nodes, run.Before, run.PromptSha256, run.UserId);
            if (current != expected || frame.Nodes.Any(x => x.Visible && NativeImageEvidence.IsGenerating(x.Name)))
                throw new ImageAutomationException("conversation_changed");
            beforePreview = frame.Nodes.Select(x => x.Id).ToHashSet(StringComparer.Ordinal);
            Invoke(run.Target, frame.Elements[current.ActionId], cancellation);
        }
        if (expected.OpensPreview)
        {
            var opened = false;
            var wait = Stopwatch.StartNew();
            while (!opened && wait.Elapsed < TimeSpan.FromSeconds(15))
            {
                await Task.Delay(300, cancellation);
                lock (uiGate)
                {
                    RequireWindow(run.Target, cancellation);
                    var frame = Capture(run.Target, cancellation, run);
                    var previews = frame.Nodes.Where(x => x.Visible && x.Role is "Window" or "Group"
                        && !beforePreview.Contains(x.Id) && IsPreviewDialog(x.Name)).ToArray();
                    if (previews.Length > 1) throw new ImageAutomationException("download_unavailable");
                    if (previews.Length == 0) continue;
                    run.PreviewRootId = previews[0].Id;
                    var map = frame.Nodes.ToDictionary(x => x.Id, StringComparer.Ordinal);
                    var downloads = frame.Nodes.Where(x => x.Role == "Button" && x.Visible && x.Enabled && !beforePreview.Contains(x.Id)
                        && NativeImageEvidence.IsDownload(x.Name) && NativeImageEvidence.IsWithin(x.Id, run.PreviewRootId, map)).ToArray();
                    if (downloads.Length > 1) throw new ImageAutomationException("download_unavailable");
                    if (downloads.Length == 1) { Invoke(run.Target, frame.Elements[downloads[0].Id], cancellation); opened = true; }
                }
            }
            if (!opened) throw new ImageAutomationException("download_unavailable");
        }
        var destination = Path.Combine(Path.GetFullPath(ownedDirectory), "generated-" + Guid.NewGuid().ToString("N") + ".png");
        ImageAutomationSession.EnsureOrdinaryPath(destination);
        var saved = false;
        var timer = Stopwatch.StartNew();
        while (timer.Elapsed < TimeSpan.FromSeconds(25))
        {
            cancellation.ThrowIfCancellationRequested();
            lock (uiGate)
            {
                RequireWindow(run.Target, cancellation);
                var dialog = SaveDialog(run.Target);
                if (dialog is not null)
                {
                    if (saved) throw new ImageAutomationException("download_failed");
                    var edits = dialog.FindAllDescendants(x => x.ByControlType(ControlType.Edit))
                        .Where(x => !x.IsOffscreen && x.Patterns.Value.IsSupported && (x.AutomationId == "1001"
                            || x.Name is "文件名(N):" or "文件名:" or "File name:" or "File name")).ToArray();
                    var save = dialog.FindAllDescendants(x => x.ByControlType(ControlType.Button))
                        .Where(x => !x.IsOffscreen && x.IsEnabled && x.AutomationId == "1" && x.Name is "保存(S)" or "保存" or "Save" or "&Save").ToArray();
                    if (edits.Length != 1 || save.Length != 1) throw new ImageAutomationException("download_dialog_unavailable");
                    SetValue(run.Target, edits[0], destination, cancellation);
                    if (ReadValue(edits[0]) != destination) throw new ImageAutomationException("download_dialog_unavailable");
                    cancellation.ThrowIfCancellationRequested();
                    Invoke(run.Target, save[0], cancellation);
                    saved = true;
                }
            }
            if (saved && File.Exists(destination))
            {
                try
                {
                    ImageAutomationSession.EnsureOrdinaryPath(destination);
                    using var file = new FileStream(destination, FileMode.Open, FileAccess.Read, FileShare.None);
                    if (file.Length > ImageAutomationSession.MaximumImageBytes) throw new ImageAutomationException("artifact_invalid");
                    if (file.Length > 0) return destination;
                }
                catch (IOException) { }
            }
            await Task.Delay(300, cancellation);
        }
        throw new ImageAutomationException(saved ? "download_failed" : "download_dialog_unavailable");
    }

    public Task ClearOwnedDraftAsync(PreparedImageConversation conversation, string prompt, CancellationToken cancellation)
    {
        var run = Cast(conversation);
        lock (uiGate)
        {
            if (run.Submitted) return Task.CompletedTask;
            var frame = BeforeSubmit(run, cancellation);
            var edit = frame.Elements[run.EditId];
            if (ReadValue(edit) == prompt) SetValue(run.Target, edit, "", cancellation);
        }
        return Task.CompletedTask;
    }

    public JsonElement? ExportBinding(PreparedImageConversation conversation)
    {
        var run = Cast(conversation);
        return ImageAutomationJson.Element(new Binding(1, run.Target.Handle.ToInt64(), run.Target.ProcessId, run.Target.Started.Ticks,
            run.DocumentId, run.EditId, run.Before.ToArray(), run.PromptSha256, run.UserId, run.Result, run.PreviewRootId));
    }

    public async Task<PreparedImageConversation?> RestoreSubmittedAsync(ImageRunRecord record, JsonElement saved, CancellationToken cancellation)
    {
        var binding = saved.Deserialize<Binding>(ImageAutomationJson.Options) ?? throw new ImageAutomationException("submission_uncertain");
        if (record.SubmitIntent is not true || binding.Version != 1 || binding.PromptSha256 != record.PromptSha256 || binding.BeforeIds.Length > 4096
            || binding.Window == 0 || binding.ProcessId <= 0) throw new ImageAutomationException("submission_uncertain");
        var target = new NativeImageTarget((nint)binding.Window, binding.ProcessId, new DateTime(binding.ProcessStartedUtcTicks, DateTimeKind.Utc));
        var run = new Conversation(target, binding.DocumentId, binding.EditId, binding.BeforeIds.ToHashSet(StringComparer.Ordinal), null, binding.PromptSha256)
            { Submitted = true, UserId = binding.UserId, Result = binding.Result, PreviewRootId = binding.PreviewRootId };
        // A preview opened by this run can hide the source transcript from UIA. Close only that exact bound preview,
        // then revalidate the original user turn before considering a new download action.
        lock (uiGate)
        {
            RequireWindow(target, cancellation);
            using var process = Process.GetProcessById(target.ProcessId);
            if (PackageVersion(process) is null) throw new ImageAutomationException("conversation_changed");
            var frame = Capture(target, cancellation, run);
            if (frame.DocumentId != run.DocumentId) throw new ImageAutomationException("conversation_changed");
            if (run.PreviewRootId is { } preview && frame.Nodes.Any(x => x.Id == preview && x.Visible && IsPreviewDialog(x.Name)))
            {
                RequireWindow(target, cancellation);
                var map = frame.Nodes.ToDictionary(x => x.Id, StringComparer.Ordinal);
                var close = frame.Nodes.Where(x => x.Role == "Button" && x.Visible && x.Enabled && NativeImageEvidence.IsWithin(x.Id, preview, map)
                    && x.Name is "关闭图像预览" or "关闭图片预览" or "Close image preview" or "关闭" or "Close").ToArray();
                if (close.Length != 1) throw new ImageAutomationException("download_unavailable");
                Invoke(target, frame.Elements[close[0].Id], cancellation);
                run.PreviewRootId = null;
            }
        }
        await Task.Delay(300, cancellation);
        lock (uiGate)
        {
            var frame = Capture(target, cancellation, run);
            var own = frame.Nodes.Where(x => x.Role == "Text" && !run.Before.Contains(x.Id) && (run.UserId is null || x.Id == run.UserId)
                && NativeImageEvidence.MatchesHash(x.Name, run.PromptSha256)).ToArray();
            if (frame.DocumentId != run.DocumentId || own.Length != 1) throw new ImageAutomationException("submission_uncertain");
            run.UserId = own[0].Id;
            return run;
        }
    }

    Frame BeforeSubmit(Conversation run, CancellationToken cancellation)
    {
        RequireWindow(run.Target, cancellation);
        var frame = Capture(run.Target, cancellation);
        if (frame.DocumentId != run.DocumentId || !ImageHome(frame) || !frame.Elements.ContainsKey(run.EditId))
            throw new ImageAutomationException("conversation_changed");
        return frame;
    }
    static Conversation Cast(PreparedImageConversation conversation) => conversation as Conversation ?? throw new ImageAutomationException("invalid_run");
    string ReadValue(AutomationElement edit)
    {
        if (!edit.Patterns.Value.IsSupported || edit.Patterns.Value.Pattern.IsReadOnly.Value) throw new ImageAutomationException("image_surface_unavailable");
        var value = edit.Patterns.Value.Pattern.Value.Value;
        if (!NativeComposerValue.HasClass(edit.Properties.ClassName.ValueOrDefault ?? "", "ProseMirror")) return value;
        var placeholder = edit.Name;
        if (placeholder.Length is 0 or > 256 || value != placeholder + "\n") return value;
        automation ??= new UIA3Automation { ConnectionTimeout = TimeSpan.FromSeconds(2), TransactionTimeout = TimeSpan.FromSeconds(2) };
        var walker = automation.TreeWalkerFactory.GetRawViewWalker();
        var queue = new Queue<(AutomationElement Element, string? Parent)>();
        queue.Enqueue((edit, null));
        var nodes = new List<NativeComposerNode>();
        var ids = new HashSet<string>(StringComparer.Ordinal);
        var watch = Stopwatch.StartNew();
        while (queue.TryDequeue(out var item))
        {
            if (nodes.Count >= 32 || watch.Elapsed > TimeSpan.FromSeconds(2)) throw new ImageAutomationException("image_surface_unavailable");
            var node = item.Element;
            var id = string.Join(".", node.Properties.RuntimeId.Value);
            if (!ids.Add(id)) throw new ImageAutomationException("image_surface_unavailable");
            nodes.Add(new(id, item.Parent, node.ControlType.ToString(), node.Name,
                node.Properties.ClassName.ValueOrDefault ?? "", node.Properties.AriaRole.ValueOrDefault ?? ""));
            for (var child = walker.GetFirstChild(node); child is not null; child = walker.GetNextSibling(child))
            {
                queue.Enqueue((child, id));
                if (queue.Count > 32) throw new ImageAutomationException("image_surface_unavailable");
            }
        }
        return NativeComposerValue.Read(value, placeholder, nodes);
    }
    void Invoke(NativeImageTarget target, AutomationElement element, CancellationToken cancellation)
        => windowAccess.Perform(target, cancellation, () => InvokePattern(element, cancellation));

    void SetValue(NativeImageTarget target, AutomationElement element, string value, CancellationToken cancellation)
        => windowAccess.Perform(target, cancellation, () =>
        {
            if (!element.IsEnabled || element.IsOffscreen || !element.Patterns.Value.IsSupported)
                throw new ImageAutomationException("image_surface_unavailable");
            var pattern = element.Patterns.Value.Pattern;
            if (pattern.IsReadOnly.Value) throw new ImageAutomationException("image_surface_unavailable");
            cancellation.ThrowIfCancellationRequested();
            pattern.SetValue(value);
        });

    static void InvokePattern(AutomationElement element, CancellationToken cancellation)
    {
        if (!element.IsEnabled || element.IsOffscreen || !element.Patterns.Invoke.IsSupported) throw new ImageAutomationException("image_surface_unavailable");
        var pattern = element.Patterns.Invoke.Pattern;
        cancellation.ThrowIfCancellationRequested();
        pattern.Invoke();
    }
    static NativeImageNode? GlobalImages(Frame frame)
    {
        var matches = frame.Nodes.Where(x => x.Role == "Button" && x.Visible && x.Enabled && x.Name is "图像" or "Images").ToArray();
        return matches.Length == 1 ? matches[0] : null;
    }
    static bool HasLogin(Frame frame) => frame.Nodes.Any(x => x.Role == "Button" && x.Visible && x.Name is "登录" or "注册" or "Log in" or "Sign up");
    static bool IsPreviewDialog(string name) => name is "图像预览" or "图片预览" or "Image preview";
    static bool ImageHome(Frame frame) => !HasLogin(frame) && WritableEdit(frame) is { } edit && NativeImagePage.IsHome(frame.Nodes, edit.Id);
    static bool ChatGptProduct(Frame frame) => ImageHome(frame) || frame.Nodes.Any(x => x.Role == "Button" && x.Visible
        && x.Name is "切换模式，当前模式：ChatGPT" or "Switch mode, current mode: ChatGPT");
    static NativeImageNode? WritableEdit(Frame frame)
    {
        var candidates = frame.Nodes.Where(x => x.Role == "Edit" && x.Visible && x.Enabled
            && frame.Elements[x.Id].Patterns.Value.IsSupported && !frame.Elements[x.Id].Patterns.Value.Pattern.IsReadOnly.Value).ToArray();
        return candidates.Length == 1 ? candidates[0] : null;
    }

    Frame Capture(NativeImageTarget target, CancellationToken cancellation, Conversation? conversation = null)
    {
        RequireWindow(target, cancellation);
        automation ??= new UIA3Automation { ConnectionTimeout = TimeSpan.FromSeconds(2), TransactionTimeout = TimeSpan.FromSeconds(2) };
        var root = automation.FromHandle(target.Handle);
        var walker = automation.TreeWalkerFactory.GetControlViewWalker();
        var elements = new Dictionary<string, AutomationElement>(StringComparer.Ordinal);
        var nodes = new List<NativeImageNode>();
        var queue = new Queue<(AutomationElement Element, string? Parent)>();
        queue.Enqueue((root, null));
        var watch = Stopwatch.StartNew();
        while (queue.TryDequeue(out var item))
        {
            cancellation.ThrowIfCancellationRequested();
            if (nodes.Count >= 4096 || watch.Elapsed > TimeSpan.FromSeconds(7)) throw new ImageAutomationException("image_surface_unavailable");
            var element = item.Element;
            var id = string.Join(".", element.Properties.RuntimeId.Value);
            if (!elements.TryAdd(id, element)) throw new ImageAutomationException("image_surface_unavailable");
            var role = element.ControlType.ToString();
            // Only transient, bounded matching values; account fields, edit values and arbitrary message text are never exported.
            var name = "";
            if (role is "Button" or "Text" or "MenuItem" or "Window" or "Group")
            {
                var label = element.Name;
                if (KnownLabel(label)) name = label;
                else if (role == "Text" && conversation is not null && label.Length <= 200_000
                    && !conversation.Before.Contains(id) && (conversation.UserId is null || id == conversation.UserId)
                    && NativeImageEvidence.MatchesHash(label, conversation.PromptSha256)) name = label;
            }
            var bounds = element.BoundingRectangle;
            string? speaker = null;
            if (name is "你说：" or "You said:" or "ChatGPT 说：" or "ChatGPT said:")
                speaker = NativeImageEvidence.SpeakerFromHeading(name, element.Properties.HeadingLevel.ValueOrDefault == HeadingLevel.Level4,
                    element.Properties.AriaRole.ValueOrDefault);
            var availability = ReadAvailability(element.Properties.IsOffscreen, element.Properties.IsEnabled);
            nodes.Add(new(id, item.Parent, role, name, availability.Visible, availability.Enabled, bounds.Width, bounds.Height, speaker));
            var child = walker.GetFirstChild(element);
            while (child is not null)
            {
                queue.Enqueue((child, id));
                child = walker.GetNextSibling(child);
                if (queue.Count > 4096) throw new ImageAutomationException("image_surface_unavailable");
            }
        }
        var scoped = NativeImageFrame.Capture(nodes);
        return new(scoped.Nodes.ToList(), scoped.Nodes.ToDictionary(x => x.Id, x => elements[x.Id], StringComparer.Ordinal), scoped.DocumentId);
    }

    internal static (bool Visible, bool Enabled) ReadAvailability(IAutomationProperty<bool> offscreen, IAutomationProperty<bool> enabled)
    {
        // The observed top-level Window provider omits IsOffscreen. Keep its children in
        // the tree, but never make a node with missing visibility/enabled metadata actionable.
        var visible = offscreen.TryGetValue(out var isOffscreen) && !isOffscreen;
        var interactive = enabled.TryGetValue(out var isEnabled) && isEnabled;
        return (visible, interactive);
    }

    static bool KnownLabel(string label) => NativeImageEvidence.IsDownload(label) || NativeImageEvidence.IsPreview(label)
        || NativeImageEvidence.IsGenerating(label) || IsPreviewDialog(label)
        || label is "图像" or "Images" or "切换模式，当前模式：ChatGPT" or "Switch mode, current mode: ChatGPT"
            or "选择 ChatGPT 模型" or "Choose a ChatGPT model" or "Select ChatGPT model" or "登录" or "注册" or "Log in" or "Sign up"
            or "发送" or "Send" or "Send prompt" or "Send message" or "关闭图像预览" or "关闭图片预览" or "Close image preview" or "关闭" or "Close"
            or "你说：" or "You said:" or "ChatGPT 说：" or "ChatGPT said:";

    internal NativeImageTarget DiscoverSource(IReadOnlyList<NativeImageTarget> targets, nint foreground)
        => NativeImagePreparation.SelectSource(targets, sourceTarget, foreground);

    internal ImageSurfaceProbe ProbeSource(IReadOnlyList<NativeImageTarget> targets, nint foreground, Func<NativeImageTarget, ImageSurfaceProbe> read)
    {
        NativeImageTarget target;
        try { target = DiscoverSource(targets, foreground); }
        catch (ImageAutomationException error) when (sourceTarget is null && error.Reason == "ambiguous_chatgpt_windows")
        {
            // When another app is foreground, only a unique verified product surface
            // can be selected. Unknown provider failures and multiple valid surfaces
            // remain ambiguous; package identity and enumeration order are insufficient.
            var qualified = new List<NativeImageTarget>();
            foreach (var candidate in targets)
            {
                var candidateProbe = read(candidate);
                if (candidateProbe.Available) qualified.Add(candidate);
                else if (candidateProbe.Reason != "chatgpt_mode_required") return candidateProbe;
            }
            if (qualified.Count == 0) return new(false, "chatgpt_mode_required");
            if (qualified.Count != 1) throw new ImageAutomationException("ambiguous_chatgpt_windows");
            sourceTarget = qualified[0];
            return new(true, "ready");
        }
        var probe = read(target);
        if (probe.Available) sourceTarget = target;
        return probe;
    }

    ImageSurfaceProbe ProbeTarget(NativeImageTarget target, CancellationToken cancellation)
    {
        var frame = Capture(target, cancellation);
        return ChatGptProduct(frame) && GlobalImages(frame) is not null
            ? new(true, "ready") : new(false, "chatgpt_mode_required");
    }

    NativeImageTarget Discover(CancellationToken cancellation)
    {
        var probe = ProbeSource(DiscoverTargets(), GetForegroundWindow(), target => ProbeTarget(target, cancellation));
        return probe.Available ? sourceTarget! : throw new ImageAutomationException(probe.Reason);
    }

    static List<NativeImageTarget> DiscoverTargets()
    {
        var candidates = new List<NativeImageTarget>();
        foreach (var process in Process.GetProcessesByName("ChatGPT"))
        {
            using (process)
            {
                try
                {
                    if (PackageVersion(process) is null) continue;
                    EnumWindows((window, _) =>
                    {
                        GetWindowThreadProcessId(window, out var id);
                        if (id == process.Id && IsWindowVisible(window) && GetWindow(window, 4) == 0)
                            candidates.Add(new(window, process.Id, process.StartTime.ToUniversalTime()));
                        return true;
                    }, 0);
                }
                catch (Exception error) when (error is System.ComponentModel.Win32Exception or InvalidOperationException) { }
            }
        }
        return candidates;
    }
    void OpenNewWindow(NativeImageTarget target, CancellationToken cancellation)
    {
        var elements = new Dictionary<string, AutomationElement>(StringComparer.Ordinal);
        IReadOnlyList<NativeMenuNode> ReadMenu()
        {
            RequireWindow(target, cancellation);
            var root = automation!.FromHandle(target.Handle);
            var walker = automation.TreeWalkerFactory.GetControlViewWalker();
            var nodes = new List<NativeMenuNode>();
            var seen = new HashSet<string>(StringComparer.Ordinal);
            var queue = new Queue<(AutomationElement Element, string? Parent)>();
            queue.Enqueue((root, null));
            var watch = Stopwatch.StartNew();
            while (queue.TryDequeue(out var item))
            {
                cancellation.ThrowIfCancellationRequested();
                if (nodes.Count >= 4096 || watch.Elapsed > TimeSpan.FromSeconds(3)) throw new ImageAutomationException("independent_window_unavailable");
                var element = item.Element;
                var id = string.Join(".", element.Properties.RuntimeId.Value);
                if (!seen.Add(id)) throw new ImageAutomationException("independent_window_unavailable");
                elements[id] = element;
                var role = element.ControlType.ToString();
                var name = "";
                if (role is "Menu" or "MenuItem")
                {
                    var label = element.Name;
                    if (label is "文件" or "File" or "新建窗口" or "New Window") name = label;
                }
                var availability = ReadAvailability(element.Properties.IsOffscreen, element.Properties.IsEnabled);
                var automationId = element.Properties.AutomationId.ValueOrDefault ?? "";
                if (automationId is not ("application-menu-content" or "application-menu-trigger-file-menu" or "application-menu-trigger-edit-menu"
                    or "application-menu-trigger-view-menu" or "application-menu-trigger-help-menu")) automationId = "";
                var ariaRole = element.Properties.AriaRole.ValueOrDefault == "menubar" ? "menubar" : "";
                nodes.Add(new(new(id, item.Parent, role, name, availability.Visible, availability.Enabled),
                    element.Patterns.ExpandCollapse.IsSupported, element.Patterns.Invoke.IsSupported, automationId, ariaRole));
                var child = walker.GetFirstChild(element);
                while (child is not null)
                {
                    queue.Enqueue((child, id));
                    if (queue.Count > 4096) throw new ImageAutomationException("independent_window_unavailable");
                    child = walker.GetNextSibling(child);
                }
            }
            return nodes;
        }
        NativeWindowMenu.CreateWindow(ReadMenu, id =>
        {
            RequireWindow(target, cancellation);
            var file = elements[id];
            if (file.ControlType != ControlType.MenuItem || file.AutomationId != "application-menu-trigger-file-menu"
                || file.Name is not ("文件" or "File") || !file.IsEnabled || file.IsOffscreen || !file.Patterns.ExpandCollapse.IsSupported)
                throw new ImageAutomationException("independent_window_unavailable");
            var pattern = file.Patterns.ExpandCollapse.Pattern;
            windowAccess.Perform(target, cancellation, () =>
            {
                cancellation.ThrowIfCancellationRequested();
                pattern.Expand();
            });
        }, id =>
        {
            RequireWindow(target, cancellation);
            var command = elements[id];
            if (command.ControlType != ControlType.MenuItem || command.Name is not ("新建窗口" or "New Window"))
                throw new ImageAutomationException("independent_window_unavailable");
            var walker = automation!.TreeWalkerFactory.GetControlViewWalker();
            var parent = walker.GetParent(command);
            var menuFound = false;
            for (var depth = 0; parent is not null && depth < 32; depth++, parent = walker.GetParent(parent))
            {
                if (parent.ControlType == ControlType.Document) break;
                if (parent.ControlType == ControlType.Menu && parent.AutomationId == "application-menu-content" && parent.Name is "文件" or "File")
                { menuFound = true; break; }
            }
            if (!menuFound) throw new ImageAutomationException("independent_window_unavailable");
            Invoke(target, command, cancellation);
        }, id =>
        {
            try
            {
                RequireWindow(target, CancellationToken.None);
                windowAccess.Perform(target, CancellationToken.None, () => elements[id].Patterns.ExpandCollapse.Pattern.Collapse());
            }
            catch (Exception error) when (error is ImageAutomationException or COMException or InvalidOperationException or FlaUIException or TimeoutException) { }
        }, cancellation);
    }

    static string? PackageVersion(Process process)
    {
        uint length = 0;
        if (GetPackageFullName(process.Handle, ref length, null) != 122 || length > 1024) return null;
        var full = new StringBuilder((int)length);
        return GetPackageFullName(process.Handle, ref length, full) == 0 ? QualifyingPackageVersion(full.ToString()) : null;
    }
    public static string? QualifyingPackageVersion(string package)
    {
        var match = Regex.Match(package, @"^OpenAI\.(?:ChatGPT(?:-Desktop)?|Codex)_([0-9]+\.[0-9]+\.[0-9]+\.[0-9]+)_(?:x64|arm64|x86|neutral)__2p2nqsd0c76g0$", RegexOptions.CultureInvariant);
        return match.Success ? match.Groups[1].Value : null;
    }
    static NativeWindowIdentity? ReadWindowIdentity(nint window)
    {
        if (!IsWindow(window)) return null;
        GetWindowThreadProcessId(window, out var id);
        try
        {
            using var process = Process.GetProcessById(id);
            return new(window, id, process.StartTime.ToUniversalTime(), PackageVersion(process) is not null);
        }
        catch (Exception error) when (error is ArgumentException or InvalidOperationException or System.ComponentModel.Win32Exception)
        { return null; }
    }

    void RequireWindow(NativeImageTarget target, CancellationToken cancellation) => windowAccess.Require(target, cancellation);

    internal static bool OwnedSaveDialog(nint window, nint targetWindow, Func<nint, nint> owner)
    {
        var visited = new HashSet<nint> { window };
        for (var depth = 0; depth < 32; depth++)
        {
            window = owner(window);
            if (window == 0 || !visited.Add(window)) return false;
            if (window == targetWindow) return true;
        }
        return false;
    }

    AutomationElement? SaveDialog(NativeImageTarget target)
    {
        var dialogs = new List<nint>();
        EnumWindows((window, _) =>
        {
            if (!IsWindowVisible(window)) return true;
            // A shared process can host unrelated conversations and Save As dialogs.
            if (!OwnedSaveDialog(window, target.Handle, handle => GetWindow(handle, 4))) return true;
            var element = automation!.FromHandle(window);
            if (element.Name is "另存为" or "Save As") dialogs.Add(window);
            return true;
        }, 0);
        if (dialogs.Count > 1) throw new ImageAutomationException("download_dialog_unavailable");
        if (dialogs.Count == 0) return null;
        return automation!.FromHandle(dialogs[0]);
    }
    public void Dispose() { lock (uiGate) { automation?.Dispose(); automation = null; } }

    delegate bool EnumWindowProc(nint window, nint parameter);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumWindowProc callback, nint parameter);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(nint window);
    [DllImport("user32.dll")] static extern bool IsWindow(nint window);
    [DllImport("user32.dll")] static extern nint GetWindow(nint window, uint command);
    [DllImport("user32.dll")] static extern nint GetForegroundWindow();
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(nint window, out int process);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] static extern int GetPackageFullName(nint process, ref uint length, StringBuilder? fullName);
}
