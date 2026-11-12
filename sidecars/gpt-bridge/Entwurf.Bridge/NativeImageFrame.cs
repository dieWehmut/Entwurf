namespace Entwurf.Bridge;

internal sealed record NativeImageFrame(IReadOnlyList<NativeImageNode> Nodes, string DocumentId, bool HasAdditionalContent = false)
{
    internal static NativeImageFrame Capture(IReadOnlyList<NativeImageNode> nodes)
    {
        if (nodes.Count == 0 || nodes.Count > 4096 || nodes.Count(x => x.ParentId is null) != 1)
            throw new ImageAutomationException("image_surface_unavailable");
        var map = new Dictionary<string, NativeImageNode>(StringComparer.Ordinal);
        foreach (var node in nodes)
            if (string.IsNullOrEmpty(node.Id) || !map.TryAdd(node.Id, node)) throw new ImageAutomationException("image_surface_unavailable");
        var owners = new Dictionary<string, string?>(StringComparer.Ordinal);
        var outerDocuments = new List<string>();
        foreach (var node in nodes)
        {
            var visited = new HashSet<string>(StringComparer.Ordinal) { node.Id };
            var parentId = node.ParentId;
            string? ancestorDocument = null;
            while (parentId is not null)
            {
                if (!visited.Add(parentId) || !map.TryGetValue(parentId, out var parent))
                    throw new ImageAutomationException("image_surface_unavailable");
                if (ancestorDocument is null && parent.Role == "Document") ancestorDocument = parent.Id;
                parentId = parent.ParentId;
            }
            if (node.Role == "Document" && ancestorDocument is null) outerDocuments.Add(node.Id);
            owners[node.Id] = node.Role == "Document" ? node.Id : ancestorDocument;
        }
        if (outerDocuments.Count != 1) throw new ImageAutomationException("image_surface_unavailable");
        var documentId = outerDocuments[0];
        // Controls inside an embedded document belong to that document, even if labels match the main product.
        return new(nodes.Where(x => owners[x.Id] == documentId).ToArray(), documentId,
            nodes.Any(x => x.Role == "Document" && x.Id != documentId));
    }
}
