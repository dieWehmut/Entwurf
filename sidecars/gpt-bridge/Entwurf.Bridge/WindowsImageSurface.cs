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
    UIA3Automation? automation;
    sealed record Target(nint Handle, int ProcessId, DateTime Started);
    sealed record Frame(List<NativeImageNode> Nodes, Dictionary<string, AutomationElement> Elements, string DocumentId);
    sealed record Binding(int Version, long Window, int ProcessId, long ProcessStartedUtcTicks, string DocumentId,
        string EditId, string[] BeforeIds, string PromptSha256, string? UserId, NativeGeneratedImage? Result, string? PreviewRootId);
    sealed class Conversation(Target target, string documentId, string editId, HashSet<string> before, string? prompt, string promptSha256) : PreparedImageConversation
    {
        public Target Target { get; } = target;
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
        if (!DesktopUnlocked()) return new ImageSurfaceProbe(false, "windows_locked");
        lock (uiGate)
        {
            var target = Discover();
            var frame = Capture(target, cancellation);
            return ChatGptProduct(frame) && GlobalImages(frame) is not null
                ? new ImageSurfaceProbe(true, "ready") : new ImageSurfaceProbe(false, "chatgpt_mode_required");
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
        Target target;
        lock (uiGate)
        {
            if (!DesktopUnlocked()) throw new ImageAutomationException("windows_locked");
            target = Discover();
            Focus(target);
            var frame = Capture(target, cancellation);
            if (!ChatGptProduct(frame)) throw new ImageAutomationException("chatgpt_mode_required");
            if (ImageHome(frame) && WritableEdit(frame) is { } existing && ReadValue(frame.Elements[existing.Id]).Length != 0)
                throw new ImageAutomationException("user_changed_composer");
            var images = GlobalImages(frame) ?? throw new ImageAutomationException("image_surface_unavailable");
            SafeInteraction(target, cancellation);
            Invoke(frame.Elements[images.Id]); // The verified product-level Images page creates a new conversation on submit.
        }
        var deadline = Stopwatch.StartNew();
        while (deadline.Elapsed < TimeSpan.FromSeconds(35))
        {
            cancellation.ThrowIfCancellationRequested();
            lock (uiGate)
            {
                SafeInteraction(target, cancellation);
                var frame = Capture(target, cancellation);
                if (HasLogin(frame)) throw new ImageAutomationException("chatgpt_login_required");
                if (ImageHome(frame) && WritableEdit(frame) is { } edit)
                {
                    if (ReadValue(frame.Elements[edit.Id]).Length != 0) throw new ImageAutomationException("user_changed_composer");
                    return new Conversation(target, frame.DocumentId, edit.Id, frame.Nodes.Select(x => x.Id).ToHashSet(StringComparer.Ordinal), run.Prompt, run.PromptSha256);
                }
            }
            await Task.Delay(500, cancellation);
        }
        throw new ImageAutomationException("image_surface_unavailable");
    }

    public async Task WriteAndVerifyPromptAsync(PreparedImageConversation conversation, string prompt, CancellationToken cancellation)
    {
        var run = Cast(conversation);
        lock (uiGate)
        {
            var frame = BeforeSubmit(run, cancellation);
            var edit = frame.Elements[run.EditId];
            if (ReadValue(edit).Length != 0 || prompt != run.Prompt) throw new ImageAutomationException("user_changed_composer");
            SafeInteraction(run.Target, cancellation);
            edit.Patterns.Value.Pattern.SetValue(prompt);
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
            SafeInteraction(run.Target, cancellation);
            run.Submitted = true; // Even an ambiguous COM exception after this point can never trigger a second Invoke.
            Invoke(frame.Elements[send[0].Id]);
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
                SafeInteraction(run.Target, cancellation);
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
            SafeInteraction(run.Target, cancellation);
            var frame = Capture(run.Target, cancellation, run);
            var current = NativeImageEvidence.FindByHash(frame.Nodes, run.Before, run.PromptSha256, run.UserId);
            if (current != expected || frame.Nodes.Any(x => x.Visible && NativeImageEvidence.IsGenerating(x.Name)))
                throw new ImageAutomationException("conversation_changed");
            beforePreview = frame.Nodes.Select(x => x.Id).ToHashSet(StringComparer.Ordinal);
            Invoke(frame.Elements[current.ActionId]);
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
                    SafeInteraction(run.Target, cancellation);
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
                    if (downloads.Length == 1) { Invoke(frame.Elements[downloads[0].Id]); opened = true; }
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
                if (!DesktopUnlocked()) throw new ImageAutomationException("windows_locked");
                VerifyTarget(run.Target);
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
                    edits[0].Patterns.Value.Pattern.SetValue(destination);
                    if (ReadValue(edits[0]) != destination) throw new ImageAutomationException("download_dialog_unavailable");
                    cancellation.ThrowIfCancellationRequested();
                    Invoke(save[0]);
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
            if (ReadValue(edit) == prompt) edit.Patterns.Value.Pattern.SetValue("");
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
        if (!record.SubmitIntent || binding.Version != 1 || binding.PromptSha256 != record.PromptSha256 || binding.BeforeIds.Length > 4096
            || binding.Window == 0 || binding.ProcessId <= 0) throw new ImageAutomationException("submission_uncertain");
        var target = new Target((nint)binding.Window, binding.ProcessId, new DateTime(binding.ProcessStartedUtcTicks, DateTimeKind.Utc));
        var run = new Conversation(target, binding.DocumentId, binding.EditId, binding.BeforeIds.ToHashSet(StringComparer.Ordinal), null, binding.PromptSha256)
            { Submitted = true, UserId = binding.UserId, Result = binding.Result, PreviewRootId = binding.PreviewRootId };
        // A preview opened by this run can hide the source transcript from UIA. Close only that exact bound preview,
        // then revalidate the original user turn before considering a new download action.
        lock (uiGate)
        {
            if (!DesktopUnlocked()) throw new ImageAutomationException("windows_locked");
            VerifyTarget(target);
            using var process = Process.GetProcessById(target.ProcessId);
            if (PackageVersion(process) is null) throw new ImageAutomationException("conversation_changed");
            var frame = Capture(target, cancellation, run);
            if (frame.DocumentId != run.DocumentId) throw new ImageAutomationException("conversation_changed");
            if (run.PreviewRootId is { } preview && frame.Nodes.Any(x => x.Id == preview && x.Visible && IsPreviewDialog(x.Name)))
            {
                SafeInteraction(target, cancellation);
                var map = frame.Nodes.ToDictionary(x => x.Id, StringComparer.Ordinal);
                var close = frame.Nodes.Where(x => x.Role == "Button" && x.Visible && x.Enabled && NativeImageEvidence.IsWithin(x.Id, preview, map)
                    && x.Name is "关闭图像预览" or "关闭图片预览" or "Close image preview" or "关闭" or "Close").ToArray();
                if (close.Length != 1) throw new ImageAutomationException("download_unavailable");
                Invoke(frame.Elements[close[0].Id]);
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
        SafeInteraction(run.Target, cancellation);
        var frame = Capture(run.Target, cancellation);
        if (frame.DocumentId != run.DocumentId || !ImageHome(frame) || !frame.Elements.ContainsKey(run.EditId))
            throw new ImageAutomationException("conversation_changed");
        return frame;
    }
    static Conversation Cast(PreparedImageConversation conversation) => conversation as Conversation ?? throw new ImageAutomationException("invalid_run");
    static string ReadValue(AutomationElement edit)
    {
        if (!edit.Patterns.Value.IsSupported || edit.Patterns.Value.Pattern.IsReadOnly.Value) throw new ImageAutomationException("image_surface_unavailable");
        return edit.Patterns.Value.Pattern.Value.Value;
    }
    static void Invoke(AutomationElement element)
    {
        if (!element.IsEnabled || element.IsOffscreen || !element.Patterns.Invoke.IsSupported) throw new ImageAutomationException("image_surface_unavailable");
        element.Patterns.Invoke.Pattern.Invoke();
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

    Frame Capture(Target target, CancellationToken cancellation, Conversation? conversation = null)
    {
        VerifyTarget(target);
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
            if (!elements.TryAdd(id, element)) continue;
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
        var documents = nodes.Where(x => x.Role == "Document").ToArray();
        if (documents.Length != 1) throw new ImageAutomationException("image_surface_unavailable");
        return new(nodes, elements, documents[0].Id);
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

    static Target Discover()
    {
        var candidates = new List<Target>();
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
        if (candidates.Count != 1) throw new ImageAutomationException(candidates.Count == 0 ? "chatgpt_not_found" : "ambiguous_chatgpt_windows");
        return candidates[0];
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
    static void VerifyTarget(Target target)
    {
        if (!IsWindow(target.Handle)) throw new ImageAutomationException("conversation_changed");
        GetWindowThreadProcessId(target.Handle, out var id);
        using var process = Process.GetProcessById(target.ProcessId);
        if (id != target.ProcessId || process.StartTime.ToUniversalTime() != target.Started) throw new ImageAutomationException("conversation_changed");
    }
    static void SafeInteraction(Target target, CancellationToken cancellation)
    {
        cancellation.ThrowIfCancellationRequested();
        VerifyTarget(target);
        if (!DesktopUnlocked()) throw new ImageAutomationException("windows_locked");
        if (IsIconic(target.Handle) || GetForegroundWindow() != target.Handle) throw new ImageAutomationException("foreground_unavailable");
    }
    static void Focus(Target target)
    {
        if (!DesktopUnlocked()) throw new ImageAutomationException("windows_locked");
        if (IsIconic(target.Handle)) ShowWindow(target.Handle, 9);
        SetForegroundWindow(target.Handle);
        if (GetForegroundWindow() == target.Handle) return;
        var current = GetCurrentThreadId();
        var foregroundThread = GetWindowThreadProcessId(GetForegroundWindow(), out _);
        var attached = foregroundThread != 0 && foregroundThread != current && AttachThreadInput(current, foregroundThread, true);
        try { BringWindowToTop(target.Handle); SetForegroundWindow(target.Handle); }
        finally { if (attached) AttachThreadInput(current, foregroundThread, false); }
        if (GetForegroundWindow() != target.Handle) throw new ImageAutomationException("foreground_unavailable");
    }
    AutomationElement? SaveDialog(Target target)
    {
        var dialogs = new List<nint>();
        EnumWindows((window, _) =>
        {
            if (!IsWindowVisible(window)) return true;
            GetWindowThreadProcessId(window, out var id);
            if (id != target.ProcessId && GetWindow(window, 4) != target.Handle) return true;
            var element = automation!.FromHandle(window);
            if (element.Name is "另存为" or "Save As") dialogs.Add(window);
            return true;
        }, 0);
        if (dialogs.Count > 1) throw new ImageAutomationException("download_dialog_unavailable");
        if (dialogs.Count == 0) return null;
        if (GetForegroundWindow() != dialogs[0]) throw new ImageAutomationException("foreground_unavailable");
        return automation!.FromHandle(dialogs[0]);
    }
    static bool DesktopUnlocked()
    {
        GetWindowThreadProcessId(GetForegroundWindow(), out var id);
        if (id != 0)
        {
            try { using var foreground = Process.GetProcessById(id); if (foreground.ProcessName is "LockApp" or "LogonUI") return false; }
            catch (ArgumentException) { return false; }
        }
        var desktop = OpenInputDesktop(0, false, 1);
        if (desktop == 0) return false;
        try { var name = new StringBuilder(256); return GetUserObjectInformation(desktop, 2, name, name.Capacity * 2, out _) && name.ToString() == "Default"; }
        finally { CloseDesktop(desktop); }
    }
    public void Dispose() { lock (uiGate) { automation?.Dispose(); automation = null; } }

    delegate bool EnumWindowProc(nint window, nint parameter);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumWindowProc callback, nint parameter);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(nint window);
    [DllImport("user32.dll")] static extern bool IsWindow(nint window);
    [DllImport("user32.dll")] static extern bool IsIconic(nint window);
    [DllImport("user32.dll")] static extern nint GetWindow(nint window, uint command);
    [DllImport("user32.dll")] static extern nint GetForegroundWindow();
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(nint window, out int process);
    [DllImport("user32.dll")] static extern bool SetForegroundWindow(nint window);
    [DllImport("user32.dll")] static extern bool BringWindowToTop(nint window);
    [DllImport("user32.dll")] static extern bool ShowWindow(nint window, int command);
    [DllImport("user32.dll")] static extern bool AttachThreadInput(uint source, uint target, bool attach);
    [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] static extern int GetPackageFullName(nint process, ref uint length, StringBuilder? fullName);
    [DllImport("user32.dll")] static extern nint OpenInputDesktop(uint flags, bool inherit, uint access);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern bool GetUserObjectInformation(nint handle, int index, StringBuilder information, int length, out int needed);
    [DllImport("user32.dll")] static extern bool CloseDesktop(nint desktop);
}
