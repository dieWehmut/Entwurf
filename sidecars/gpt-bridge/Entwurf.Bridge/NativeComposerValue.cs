namespace Entwurf.Bridge;

internal sealed record NativeComposerNode(string Id, string? ParentId, string Role, string Name,
    string ClassName, string AriaRole = "");

internal static class NativeComposerValue
{
    // Chromium includes the CSS-generated hint and ProseMirror's trailing <br> in
    // ValuePattern. The renderer decorates only a single genuinely empty paragraph
    // with "placeholder". The raw tree, not matching text or disabled Send, proves
    // that the value is generated content rather than a user's identical draft.
    internal static string Read(string value, string placeholder, IReadOnlyList<NativeComposerNode> nodes)
    {
        if (placeholder.Length is 0 or > 256 || value != placeholder + "\n" || nodes.Count != 5
            || nodes.Select(x => x.Id).Distinct(StringComparer.Ordinal).Count() != nodes.Count) return value;
        var roots = nodes.Where(x => x.ParentId is null).ToArray();
        if (roots.Length != 1) return value;
        var root = roots[0];
        if (root.Role != "Edit" || root.AriaRole != "textbox" || root.Name != placeholder
            || !HasClass(root.ClassName, "ProseMirror")) return value;
        var paragraphs = nodes.Where(x => x.ParentId == root.Id).ToArray();
        if (paragraphs.Length != 1) return value;
        var paragraph = paragraphs[0];
        if (paragraph.Role != "Group" || paragraph.AriaRole != "group" || paragraph.Name != ""
            || !HasClass(paragraph.ClassName, "placeholder")) return value;
        var children = nodes.Where(x => x.ParentId == paragraph.Id).ToArray();
        if (children.Length != 2) return value;
        var wrappers = children.Where(x => x.Role == "Group" && x.AriaRole == "group"
            && x.Name == "" && x.ClassName == "").ToArray();
        var breaks = children.Where(x => x.Role == "Text" && x.AriaRole == "description" && x.Name == "\n"
            && HasClass(x.ClassName, "ProseMirror-trailingBreak")).ToArray();
        if (wrappers.Length != 1 || breaks.Length != 1) return value;
        var hint = nodes.Where(x => x.ParentId == wrappers[0].Id).ToArray();
        return hint.Length == 1 && hint[0].Role == "Text" && hint[0].AriaRole == "description"
            && hint[0].ClassName == "" && hint[0].Name == placeholder ? "" : value;
    }

    internal static bool HasClass(string classes, string expected)
        => classes.Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries).Contains(expected, StringComparer.Ordinal);
}
