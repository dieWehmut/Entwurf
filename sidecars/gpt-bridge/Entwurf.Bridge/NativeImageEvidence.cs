using System.Security.Cryptography;
using System.Text;
using System.Globalization;

namespace Entwurf.Bridge;

/// <summary>Transient UIA evidence. Names are never persisted, logged, or sent over the companion protocol.</summary>
public sealed record NativeImageNode(string Id, string? ParentId, string Role, string Name,
    bool Visible = true, bool Enabled = true, int Width = 0, int Height = 0, string? Speaker = null);
public sealed record NativeGeneratedImage(string UserId, string ResponseId, string ImageId, string ActionId, bool OpensPreview, string AssistantId,
    string? UserHeadingId = null);
public sealed record NativeUserTurn(string TextId, string HeadingId);

public static class NativeImageEvidence
{
    public static bool IsDownload(string name) => name is "下载图像" or "下载图片" or "Download image";
    public static bool IsPreviewDownload(string name) => IsDownload(name) || name is "下载" or "Download";
    public static bool IsPreviewDialog(string name) => name is "图像预览" or "图片预览" or "Image preview";
    public static bool IsPreview(string name) => name is "打开图像预览" or "打开图片预览" or "Open image preview" or "查看图像" or "View image";
    public static bool IsGeneratedImageLabel(string name)
    {
        const string prefix = "已生成图像 ";
        if (!name.StartsWith(prefix, StringComparison.Ordinal)) return false;
        var number = name.AsSpan(prefix.Length);
        return number.Length is >= 1 and <= 2 && number[0] != '0'
            && int.TryParse(number, NumberStyles.None, CultureInfo.InvariantCulture, out var index) && index is >= 1 and <= 64;
    }
    public static bool IsGenerating(string name) => name is "停止生成" or "停止流式传输" or "停止" or "Stop generating" or "Stop streaming" or "Stop"
        or "正在生成图像" or "正在创建图像" or "Creating image" or "Generating image";
    public static string? SpeakerFromHeading(string name, bool levelFour, string? ariaRole) => !levelFour || ariaRole != "heading" ? null
        : name is "你说：" or "You said:" ? "user" : name is "ChatGPT 说：" or "ChatGPT said:" ? "assistant" : null;

    public static NativeGeneratedImage? Find(IReadOnlyList<NativeImageNode> nodes, IReadOnlySet<string> before,
        string prompt, string? boundUserId, string? boundUserHeadingId = null)
        => FindMatching(nodes, before, name => name == prompt, boundUserId, boundUserHeadingId);

    public static NativeGeneratedImage? FindByHash(IReadOnlyList<NativeImageNode> nodes, IReadOnlySet<string> before,
        string promptSha256, string? boundUserId, string? boundUserHeadingId = null)
        => FindMatching(nodes, before, name => MatchesHash(name, promptSha256), boundUserId, boundUserHeadingId);

    public static NativeUserTurn? FindUserTurnByHash(IReadOnlyList<NativeImageNode> nodes, IReadOnlySet<string> before,
        string promptSha256, string? boundUserId, string? boundUserHeadingId = null)
    {
        var map = nodes.ToDictionary(x => x.Id, StringComparer.Ordinal);
        return FindUserTurn(nodes, before, name => MatchesHash(name, promptSha256), boundUserId, boundUserHeadingId, map, DocumentOrder(nodes, map));
    }

    public static bool SameResponse(NativeGeneratedImage? current, NativeGeneratedImage expected) => current is not null
        && current.ResponseId == expected.ResponseId && current.ImageId == expected.ImageId && current.ActionId == expected.ActionId
        && current.OpensPreview == expected.OpensPreview && current.AssistantId == expected.AssistantId
        && (expected.UserHeadingId is not null ? current.UserHeadingId == expected.UserHeadingId : current.UserId == expected.UserId);

    public static NativeImageNode? FindPreviewDownload(IReadOnlyList<NativeImageNode> nodes, IReadOnlySet<string> before,
        string previewRootId)
    {
        var map = nodes.ToDictionary(x => x.Id, StringComparer.Ordinal);
        if (!map.TryGetValue(previewRootId, out var root) || !root.Visible || root.Role is not ("Group" or "Window")
            || before.Contains(root.Id) || !IsPreviewDialog(root.Name)) throw new ImageAutomationException("download_unavailable");
        // A generic Download label is evidence only within this run's new preview.
        var downloads = nodes.Where(x => x.Role == "Button" && x.Visible && x.Enabled && !before.Contains(x.Id)
            && IsPreviewDownload(x.Name) && IsWithin(x.Id, previewRootId, map)).ToArray();
        if (downloads.Length > 1) throw new ImageAutomationException("download_unavailable");
        return downloads.SingleOrDefault();
    }

    public static bool MatchesHash(string text, string sha256) => Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(text))) == sha256;

    static NativeUserTurn? FindUserTurn(IReadOnlyList<NativeImageNode> nodes, IReadOnlySet<string> before,
        Func<string, bool> matches, string? boundUserId, string? boundUserHeadingId,
        IReadOnlyDictionary<string, NativeImageNode> map, List<NativeImageNode> ordered)
    {
        if (boundUserHeadingId is not null && (!map.TryGetValue(boundUserHeadingId, out var boundHeading)
            || boundHeading.Speaker != "user" || before.Contains(boundHeading.Id))) throw new ImageAutomationException("conversation_changed");
        var users = nodes.Where(x => x.Role == "Text" && !before.Contains(x.Id)
            && (boundUserHeadingId is not null || boundUserId is null || x.Id == boundUserId) && matches(x.Name)).ToArray();
        var turns = new List<NativeUserTurn>();
        foreach (var user in users)
        {
            // A still-present composer Text after Send is not evidence of a user
            // turn. Bind only the exact prompt in a real new h4 user fragment.
            var inEdit = nodes.Any(x => x.Role == "Edit" && IsWithin(user.Id, x.Id, map));
            if (inEdit) continue;
            var index = ordered.FindIndex(x => x.Id == user.Id);
            var heading = ordered.Take(index).LastOrDefault(x => x.Speaker is not null);
            if (heading?.Speaker != "user" || before.Contains(heading.Id)
                || (boundUserHeadingId is not null && heading.Id != boundUserHeadingId)
                || CommonGroup(user.Id, heading.Id, map) is null) continue;
            turns.Add(new(user.Id, heading.Id));
        }
        if (turns.Count != 1 && (turns.Count > 1 || boundUserId is not null || boundUserHeadingId is not null))
            throw new ImageAutomationException("conversation_changed");
        return turns.Count == 1 ? turns[0] : null;
    }

    static NativeGeneratedImage? FindMatching(IReadOnlyList<NativeImageNode> nodes, IReadOnlySet<string> before,
        Func<string, bool> matches, string? boundUserId, string? boundUserHeadingId)
    {
        var map = nodes.ToDictionary(x => x.Id, StringComparer.Ordinal);
        var ordered = DocumentOrder(nodes, map);
        var turn = FindUserTurn(nodes, before, matches, boundUserId, boundUserHeadingId, map, ordered);
        if (turn is null) return null;
        var user = map[turn.TextId];
        var userIndex = ordered.FindIndex(x => x.Id == user.Id);
        var userHeading = map[turn.HeadingId];
        var assistantIndex = ordered.FindIndex(userIndex + 1, x => x.Speaker is not null);
        if (assistantIndex < 0 || ordered[assistantIndex].Speaker != "assistant" || before.Contains(ordered[assistantIndex].Id)) return null;
        var end = ordered.FindIndex(assistantIndex + 1, x => x.Speaker == "user");
        if (end < 0) end = ordered.Count;
        var region = CommonGroup(user.Id, ordered[assistantIndex].Id, map);
        if (region is null || !IsWithin(userHeading.Id, region.Id, map)) return null;

        // Real h4 speaker headings partition the transcript. Never cross the next user heading, including a user attachment.
        // Multiple assistant fragments can belong to this same user turn; each image action must stay in its own assistant block.
        var images = ordered.Skip(assistantIndex + 1).Take(end - assistantIndex - 1)
            .Where(x => x.Role == "Image" && x.Visible && x.Width >= 128 && x.Height >= 128 && !before.Contains(x.Id) && IsWithin(x.Id, region.Id, map)).ToArray();
        var candidates = new List<NativeGeneratedImage>();
        foreach (var image in images)
        {
            var imageIndex = ordered.FindIndex(x => x.Id == image.Id);
            var headingIndex = ordered.FindLastIndex(imageIndex, x => x.Speaker is not null);
            var heading = ordered[headingIndex];
            if (heading.Speaker != "assistant" || before.Contains(heading.Id)) continue;
            var blockEnd = ordered.FindIndex(headingIndex + 1, x => x.Speaker is not null);
            if (blockEnd < 0 || blockEnd > end) blockEnd = end;
            var actions = ordered.Skip(headingIndex + 1).Take(blockEnd - headingIndex - 1)
                .Where(x => x.Role == "Button" && x.Visible && x.Enabled && !before.Contains(x.Id)
                    && (IsDownload(x.Name) || IsPreview(x.Name)
                        || (image.ParentId == x.Id && IsGeneratedImageLabel(image.Name) && x.Name == image.Name))
                    && IsWithin(x.Id, region.Id, map))
                // The observed numbered preview is the image's immediate Button
                // parent, with no assistant-only Group in this flat transcript.
                .Select(x => (Action: x, Group: image.ParentId == x.Id && IsGeneratedImageLabel(image.Name) && x.Name == image.Name
                    ? x : CommonGroup(image.Id, x.Id, map)))
                .Where(x => x.Group is not null && !IsWithin(userHeading.Id, x.Group.Id, map)).ToArray();
            if (actions.Length == 0) continue;
            var downloads = actions.Where(x => IsDownload(x.Action.Name)).ToArray();
            var usable = downloads.Length > 0 ? downloads : actions;
            if (usable.Length != 1) throw new ImageAutomationException("ambiguous_images");
            candidates.Add(new(user.Id, usable[0].Group!.Id, image.Id, usable[0].Action.Id, !IsDownload(usable[0].Action.Name), heading.Id, userHeading.Id));
        }
        if (candidates.Count > 1) throw new ImageAutomationException("ambiguous_images");
        return candidates.Count == 1 ? candidates[0] : null;
    }

    static NativeImageNode? CommonGroup(string first, string second, IReadOnlyDictionary<string, NativeImageNode> map)
    {
        for (var id = first; map.TryGetValue(id, out var node); id = node.ParentId!)
        {
            if (node.Role == "Group" && IsWithin(second, id, map)) return node;
            if (node.ParentId is null) break;
        }
        return null;
    }

    static List<NativeImageNode> DocumentOrder(IReadOnlyList<NativeImageNode> nodes, IReadOnlyDictionary<string, NativeImageNode> map)
    {
        var children = nodes.Where(x => x.ParentId is not null).GroupBy(x => x.ParentId!).ToDictionary(x => x.Key, x => x.ToArray());
        var pending = new Stack<NativeImageNode>(nodes.Where(x => x.ParentId is null || !map.ContainsKey(x.ParentId)).Reverse());
        var ordered = new List<NativeImageNode>();
        var visited = new HashSet<string>(StringComparer.Ordinal);
        while (pending.TryPop(out var node))
        {
            if (!visited.Add(node.Id)) continue;
            ordered.Add(node);
            if (children.TryGetValue(node.Id, out var descendants)) foreach (var child in descendants.Reverse()) pending.Push(child);
        }
        return ordered;
    }

    public static bool IsWithin(string nodeId, string parentId, IReadOnlyDictionary<string, NativeImageNode> map)
    {
        for (var id = nodeId; map.TryGetValue(id, out var node); id = node.ParentId!)
        {
            if (id == parentId) return true;
            if (node.ParentId is null) break;
        }
        return false;
    }
}
