namespace Entwurf.Bridge;

public static class NativeImagePage
{
    public static bool IsHome(IReadOnlyList<NativeImageNode> nodes, string editId)
    {
        if (HasTranscriptOrGeneration(nodes)) return false;
        var map = nodes.ToDictionary(x => x.Id, StringComparer.Ordinal);
        if (!map.TryGetValue(editId, out var edit) || edit.Role != "Edit" || !edit.Visible || !edit.Enabled) return false;
        if (nodes.Count(x => x.ParentId == edit.ParentId && x.Role == "Button" && x.Visible && IsModelPicker(x.Name)) != 1) return false;
        var navigation = nodes.Where(x => x.Role == "Button" && x.Visible && x.Enabled && x.Name is "图像" or "Images").ToArray();
        if (navigation.Length != 1) return false;
        var parentId = edit.ParentId;
        for (var ancestors = 0; parentId is not null && ancestors < 32 && map.TryGetValue(parentId, out var parent); ancestors++, parentId = parent.ParentId)
        {
            if (parent.Role != "Group" || !nodes.Any(x => x.ParentId == parent.Id && x.Role == "Text" && x.Visible && x.Name is "图像" or "Images")) continue;
            var cursor = parent;
            var depth = 0;
            while (cursor.Role != "Document" && depth < 5 && cursor.ParentId is { } id && map.TryGetValue(id, out var ancestor))
            { cursor = ancestor; depth++; }
            // Observed Images heading is a direct child of the main page panel (three groups below Document).
            // Transcript text and a model picker from another panel cannot authorize composer input.
            if (cursor.Role == "Document" && depth <= 4 && !NativeImageEvidence.IsWithin(navigation[0].Id, parent.Id, map)) return true;
        }
        return false;
    }

    internal static bool HasTranscriptOrGeneration(IReadOnlyList<NativeImageNode> nodes) => nodes.Any(x =>
        x.Speaker is not null || x.Name is "你说：" or "You said:" or "ChatGPT 说：" or "ChatGPT said:"
        || NativeImageEvidence.IsGenerating(x.Name));

    public static bool IsModelPicker(string name) => name is "选择 ChatGPT 模型" or "Choose a ChatGPT model" or "Select ChatGPT model";
}
