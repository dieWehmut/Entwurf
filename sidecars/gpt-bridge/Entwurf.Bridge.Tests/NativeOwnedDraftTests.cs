using System.Reflection;
using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativeOwnedDraftTests
{
    [Theory]
    [InlineData(false, false)]
    [InlineData(true, true)]
    public async Task Cleanup_never_accesses_an_unwritten_or_submitted_conversation(bool submitted, bool writeDispatched)
    {
        using var surface = new WindowsImageSurface();
        const string prompt = "a user draft that exactly matches the planned prompt";
        var type = typeof(WindowsImageSurface).GetNestedType("Conversation", BindingFlags.NonPublic)!;
        var conversation = (PreparedImageConversation)Activator.CreateInstance(type,
            BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic, null,
            [new NativeImageTarget(0, 0, DateTime.UnixEpoch), "owned-document", "owned-edit",
                new HashSet<string>(StringComparer.Ordinal), prompt, new string('a', 64)], null)!;
        type.GetField("Submitted")!.SetValue(conversation, submitted);
        type.GetField("PromptWriteDispatched")!.SetValue(conversation, writeDispatched);
        // HWND zero is deliberately invalid: any capture or native mutation would
        // throw. Preparation/persistence failure must not authorize touching a draft.
        await surface.ClearOwnedDraftAsync(conversation, prompt, default);
        Assert.Equal(submitted ? "submitted" : "not_started",
            surface.ExportBinding(conversation)!.Value.GetProperty("draft_cleanup_state").GetString());
    }
}
