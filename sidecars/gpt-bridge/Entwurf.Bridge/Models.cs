using System.Text.Json;

namespace Entwurf.Bridge;

public sealed record BridgeRequest(string Id, string Method, string? Token, JsonElement? Parameters);
public sealed record BridgeResponse(bool Ok, string Id, JsonElement Result)
{
    public static BridgeResponse Succeeded(string id, object result) => new(true, id, JsonSerializer.SerializeToElement(result, WireJson.Options));
}
public sealed record GateEvidence(bool Passed, string Evidence);
public sealed record CapabilityReport(bool AppFound, bool WebViewAccessibility, bool Foreground, bool? LoginLikely,
    string? AppVersion, int WindowCount, int AccessibleControls, string Detail, Dictionary<string, GateEvidence> Gates)
{
    public bool AutomationEnabled => new[] { "discover_window", "reliable_input", "reliable_completion", "reliable_capture" }
        .All(key => Gates.TryGetValue(key, out var gate) && gate.Passed);
}
public sealed record SafetyState(bool Foreground, bool Unlocked, bool StructureUnchanged)
{
    public bool Safe => Foreground && Unlocked && StructureUnchanged;
}
public sealed record Observation(bool Complete, string State, string? Detail);
public sealed record Artifact(string Path, string Sha256, long Bytes, string Confidence);
public sealed class BridgeException(string code, string message) : Exception(message)
{
    public string Code { get; } = code;
}
public interface IWindowsAutomation
{
    CapabilityReport Detect();
    SafetyState CheckSafety();
    Observation Observe();
    string CaptureText();
    void Paste(string text);
    void Submit();
    void OpenChatGpt();
}
public static class WireJson
{
    public static readonly JsonSerializerOptions Options = new() { PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower, MaxDepth = 64 };
}
