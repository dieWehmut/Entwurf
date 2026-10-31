using System.Net;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace Entwurf.Bridge;

public sealed record AutomationBootstrapConfig(string WebsocketUrl, string Token, string StateDirectory, int ParentPid)
{
    public Uri Validate()
    {
        if (!Uri.TryCreate(WebsocketUrl, UriKind.Absolute, out var uri) || uri.Scheme != "ws"
            || uri.AbsolutePath != "/api/automation/extension" || uri.Query.Length != 0 || uri.Fragment.Length != 0
            || uri.UserInfo.Length != 0 || uri.Port is < 1 or > 65535
            || !(uri.Host == "localhost" || IPAddress.TryParse(uri.Host.Trim('[', ']'), out var ip) && IPAddress.IsLoopback(ip))
            || string.IsNullOrWhiteSpace(Token) || Token.Length > 4096 || ParentPid <= 0
            || !Path.IsPathFullyQualified(StateDirectory)) throw new InvalidDataException("invalid_bootstrap");
        return uri;
    }
}

public sealed record ImageAutomationRun(string RunId, string TaskId, string BaseVersionId, string Prompt, string PromptSha256)
{
    public void Validate()
    {
        if (!ValidId(RunId) || !ValidId(TaskId) || !ValidId(BaseVersionId) || string.IsNullOrWhiteSpace(Prompt)
            || Encoding.UTF8.GetByteCount(Prompt) > 200_000 || Prompt.Contains('\0')
            || PromptSha256 != Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(Prompt))))
            throw new ImageAutomationException("invalid_run");
    }
    public static bool ValidId(string? id) => id is not null && Regex.IsMatch(id, "^[A-Za-z0-9_-]{1,128}$", RegexOptions.CultureInvariant);
}

public sealed record ImageRunRecord(string RunId, string TaskId, string BaseVersionId, string PromptSha256,
    string Phase, string? Reason = null, string? Sha256 = null, string? MimeType = null,
    bool SubmitIntent = false, JsonElement? SurfaceBinding = null)
{
    public bool Terminal => Phase is "completed" or "cancelled" or "failed";
    public bool Matches(ImageAutomationRun run) => RunId == run.RunId && TaskId == run.TaskId
        && BaseVersionId == run.BaseVersionId && PromptSha256 == run.PromptSha256;
}

public sealed record ImageSurfaceProbe(bool Available, string Reason);
public abstract class PreparedImageConversation;
public sealed class ImageAutomationException(string reason) : Exception(reason)
{
    public string Reason { get; } = reason;
}

/// <summary>The only replaceable boundary is the external application's UI. Persistence and transport remain real.</summary>
public interface IImageAutomationSurface : IDisposable
{
    Task<ImageSurfaceProbe> ProbeAsync(CancellationToken cancellation);
    Task<PreparedImageConversation> PrepareAsync(ImageAutomationRun run, CancellationToken cancellation);
    Task WriteAndVerifyPromptAsync(PreparedImageConversation conversation, string prompt, CancellationToken cancellation);
    Task SubmitOnceAsync(PreparedImageConversation conversation, CancellationToken cancellation);
    Task WaitForGeneratedImageAsync(PreparedImageConversation conversation, CancellationToken cancellation);
    Task<string> DownloadOriginalAsync(PreparedImageConversation conversation, string ownedDirectory, CancellationToken cancellation);
    Task ClearOwnedDraftAsync(PreparedImageConversation conversation, string prompt, CancellationToken cancellation);
    JsonElement? ExportBinding(PreparedImageConversation conversation) => null;
    Task<PreparedImageConversation?> RestoreSubmittedAsync(ImageRunRecord run, JsonElement binding, CancellationToken cancellation) =>
        Task.FromResult<PreparedImageConversation?>(null);
}

public static class ImageAutomationJson
{
    public static readonly JsonSerializerOptions Options = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        DefaultIgnoreCondition = System.Text.Json.Serialization.JsonIgnoreCondition.WhenWritingNull,
        MaxDepth = 32
    };
    public static JsonElement Element(object value) => JsonSerializer.SerializeToElement(value, Options);
}
