using System.Text;
using System.Text.Json;
using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class TransportTests
{
    [Theory]
    [InlineData("OpenAI.Codex_26.1027.3384.0_x64__2p2nqsd0c76g0", null)]
    [InlineData("OpenAI.ChatGPT_26.1027.3384.0_x64__untrusted", null)]
    [InlineData("OpenAI.ChatGPT_26.1027.3384.0_x64__2p2nqsd0c76g0", "26.1027.3384.0")]
    [InlineData("OpenAI.ChatGPT-Desktop_1.2024.333.0_x64__2p2nqsd0c76g0", "1.2024.333.0")]
    public void Package_identity_excludes_Codex_and_untrusted_publisher(string package, string? expected)
        => Assert.Equal(expected, WindowsAutomation.PackageVersion(package));

    [Fact]
    public async Task Frames_use_little_endian_byte_length_and_preserve_unicode()
    {
        using var stream = new MemoryStream();
        await FrameCodec.WriteAsync(stream, Encoding.UTF8.GetBytes("概率"), default);
        Assert.Equal(new byte[] {6,0,0,0,0xe6,0xa6,0x82,0xe7,0x8e,0x87}, stream.ToArray());
        stream.Position = 0;
        Assert.Equal("概率", Encoding.UTF8.GetString((await FrameCodec.ReadAsync(stream, default))!));
        Assert.Null(await FrameCodec.ReadAsync(stream, default));
    }

    [Theory]
    [InlineData(new byte[] {0,0,0,0})]
    [InlineData(new byte[] {1,0,16,0})]
    [InlineData(new byte[] {255,255,255,255})]
    public async Task Untrusted_frame_lengths_are_rejected_before_allocation(byte[] bytes)
    {
        await Assert.ThrowsAsync<InvalidDataException>(async () => await FrameCodec.ReadAsync(new MemoryStream(bytes), default));
    }

    [Theory]
    [InlineData(new byte[] {2,0})]
    [InlineData(new byte[] {2,0,0,0,65})]
    public async Task Truncated_frame_is_not_treated_as_clean_eof(byte[] bytes)
    {
        await Assert.ThrowsAsync<EndOfStreamException>(async () => await FrameCodec.ReadAsync(new MemoryStream(bytes), default));
    }

    [Fact]
    public async Task Rpc_preserves_numeric_and_string_ids_on_errors_and_never_leaks_token()
    {
        using var host = new BridgeHost(new WindowsAutomation(), TextWriter.Null, tokenOverride: new string('a',64));
        var rpc = new RpcDispatcher(host);
        using var unauthorized = JsonDocument.Parse(await rpc.DispatchAsync(Encoding.UTF8.GetBytes("{\"jsonrpc\":\"2.0\",\"id\":17,\"method\":\"observe\"}"), default));
        Assert.Equal(17, unauthorized.RootElement.GetProperty("id").GetInt32());
        Assert.Equal("unauthorized", unauthorized.RootElement.GetProperty("error").GetProperty("data").GetProperty("code").GetString());
        Assert.DoesNotContain(new string('a',64), unauthorized.RootElement.GetRawText());
        using var invalid = JsonDocument.Parse(await rpc.DispatchAsync(Encoding.UTF8.GetBytes("{\"jsonrpc\":\"2.0\",\"id\":\"request-1\",\"method\":42}"), default));
        Assert.Equal("request-1", invalid.RootElement.GetProperty("id").GetString());
        Assert.Equal(-32600, invalid.RootElement.GetProperty("error").GetProperty("code").GetInt32());
    }

    [Theory]
    [InlineData("{", -32700)]
    [InlineData("[]", -32600)]
    [InlineData("{\"jsonrpc\":\"2.0\",\"id\":{},\"method\":\"detect_app\"}", -32600)]
    [InlineData("{\"jsonrpc\":\"2.0\",\"id\":1,\"id\":2,\"method\":\"detect_app\"}", -32600)]
    [InlineData("{\"jsonrpc\":\"1.0\",\"id\":1,\"method\":\"detect_app\"}", -32600)]
    public async Task Malformed_requests_are_structured_errors(string request, int code)
    {
        using var host = new BridgeHost(new WindowsAutomation(), TextWriter.Null, tokenOverride: new string('a',64));
        using var response = JsonDocument.Parse(await new RpcDispatcher(host).DispatchAsync(Encoding.UTF8.GetBytes(request), default));
        Assert.Equal(code, response.RootElement.GetProperty("error").GetProperty("code").GetInt32());
    }
}
