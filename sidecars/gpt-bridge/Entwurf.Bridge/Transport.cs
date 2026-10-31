using System.Buffers.Binary;
using System.IO.Pipes;
using System.Text.Json;

namespace Entwurf.Bridge;

public static class FrameCodec
{
    public const int MaximumBytes = 1_048_576;
    public static async ValueTask<byte[]?> ReadAsync(Stream stream, CancellationToken token)
    {
        var header = new byte[4];
        var count = await stream.ReadAsync(header.AsMemory(0, 1), token);
        if (count == 0) return null;
        await stream.ReadExactlyAsync(header.AsMemory(1), token);
        var length = BinaryPrimitives.ReadUInt32LittleEndian(header);
        if (length is 0 or > MaximumBytes) throw new InvalidDataException("Frame length is outside the allowed bounds.");
        var body = new byte[(int)length];
        await stream.ReadExactlyAsync(body, token);
        return body;
    }
    public static async ValueTask WriteAsync(Stream stream, ReadOnlyMemory<byte> body, CancellationToken token)
    {
        if (body.Length is 0 or > MaximumBytes) throw new InvalidDataException("Response exceeds the frame limit.");
        var header = new byte[4];
        BinaryPrimitives.WriteUInt32LittleEndian(header, (uint)body.Length);
        await stream.WriteAsync(header, token);
        await stream.WriteAsync(body, token);
        await stream.FlushAsync(token);
    }
}

public sealed class RpcDispatcher(BridgeHost host)
{
    public async Task<byte[]> DispatchAsync(byte[] bytes, CancellationToken token)
    {
        JsonElement? id = null;
        try
        {
            using var document = JsonDocument.Parse(bytes, new JsonDocumentOptions { MaxDepth = 64 });
            var root = document.RootElement;
            if (root.ValueKind != JsonValueKind.Object) return Failure(id, -32600, "invalid_request");
            if (root.TryGetProperty("id", out var rawId))
            {
                if (rawId.ValueKind == JsonValueKind.String && rawId.GetString()!.Length <= 128 || rawId.ValueKind == JsonValueKind.Number && rawId.TryGetInt64(out _)) id = rawId.Clone();
                else return Failure(null, -32600, "invalid_request");
            }
            // Request/response IPC deliberately requires an id; batches and notifications are unsupported.
            if (id is null || DuplicateKeys(root) || !root.TryGetProperty("jsonrpc", out var version) || version.ValueKind != JsonValueKind.String || version.GetString() != "2.0"
                || !root.TryGetProperty("method", out var method) || method.ValueKind != JsonValueKind.String)
                return Failure(id, -32600, "invalid_request");
            JsonElement? parameters = root.TryGetProperty("params", out var p) ? p : null;
            if (parameters is not null && parameters.Value.ValueKind != JsonValueKind.Object) return Failure(id, -32602, "invalid_params");
            string? secret = parameters is { } value && value.TryGetProperty("token", out var t) && t.ValueKind == JsonValueKind.String ? t.GetString() : null;
            var response = await host.HandleAsync(new BridgeRequest(id.Value.ToString(), method.GetString()!, secret, parameters), token);
            var output = JsonSerializer.SerializeToUtf8Bytes(new { jsonrpc = "2.0", id, result = response.Result }, WireJson.Options);
            return output.Length <= FrameCodec.MaximumBytes ? output : Failure(id, -32000, "result_too_large");
        }
        catch (JsonException) { return Failure(id, -32700, "parse_error"); }
        catch (BridgeException error) { return Failure(id, error.Code == "unknown_method" ? -32601 : error.Code == "invalid_params" ? -32602 : -32000, error.Code); }
        catch (OperationCanceledException) { return Failure(id, -32000, "cancelled"); }
        catch { return Failure(id, -32603, "internal_error"); }
    }
    static bool DuplicateKeys(JsonElement value)
    {
        if (value.ValueKind == JsonValueKind.Object)
        {
            var names = new HashSet<string>(StringComparer.Ordinal);
            foreach (var p in value.EnumerateObject()) if (!names.Add(p.Name) || DuplicateKeys(p.Value)) return true;
        }
        else if (value.ValueKind == JsonValueKind.Array)
            foreach (var item in value.EnumerateArray()) if (DuplicateKeys(item)) return true;
        return false;
    }
    static byte[] Failure(JsonElement? id, int code, string name) => JsonSerializer.SerializeToUtf8Bytes(new
    { jsonrpc = "2.0", id, error = new { code, message = name, data = new { code = name } } }, WireJson.Options);
}

public static class PipeServer
{
    public static NamedPipeServerStream Create(string pipeName) => new(pipeName, PipeDirection.InOut, 1,
        PipeTransmissionMode.Byte, PipeOptions.Asynchronous | PipeOptions.CurrentUserOnly, 65536, 65536);

    public static async Task ServeAsync(NamedPipeServerStream pipe, BridgeHost host, CancellationToken token)
    {
        using var session = CancellationTokenSource.CreateLinkedTokenSource(token);
        using var writes = new SemaphoreSlim(1, 1);
        var pending = new List<Task>();
        var dispatcher = new RpcDispatcher(host);
        var authenticationDeadline = DateTime.UtcNow.AddSeconds(15);
        try
        {
            while (!session.IsCancellationRequested)
            {
                using var readDeadline = CancellationTokenSource.CreateLinkedTokenSource(session.Token);
                var remaining = host.Authorized ? TimeSpan.FromMinutes(3) : authenticationDeadline - DateTime.UtcNow;
                if (remaining <= TimeSpan.Zero) break;
                readDeadline.CancelAfter(remaining);
                var bytes = await FrameCodec.ReadAsync(pipe, readDeadline.Token);
                if (bytes is null) break;
                pending.RemoveAll(t => t.IsCompleted);
                if (pending.Count >= 8) break; // Bound queued requests and terminate abusive peers.
                pending.Add(RespondAsync(bytes));
            }
        }
        finally
        {
            session.Cancel();
            await Task.WhenAll(pending);
        }
        async Task RespondAsync(byte[] bytes)
        {
            try
            {
                var response = await dispatcher.DispatchAsync(bytes, session.Token);
                await writes.WaitAsync(session.Token);
                try { await FrameCodec.WriteAsync(pipe, response, session.Token); }
                finally { writes.Release(); }
            }
            catch (Exception e) when (e is IOException or OperationCanceledException or ObjectDisposedException) { session.Cancel(); }
        }
    }
}
