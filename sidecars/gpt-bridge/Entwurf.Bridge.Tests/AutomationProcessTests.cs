using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Net.WebSockets;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class AutomationProcessTests
{
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task Automation_bootstrap_authenticates_over_websocket_and_exits_on_stdin_eof_or_auth_refusal(bool refuseAuthentication)
    {
        var directory = Path.Combine(Path.GetTempPath(), "entwurf-automation-test-" + Guid.NewGuid());
        Directory.CreateDirectory(directory);
        using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(15));
        using var listener = new TcpListener(IPAddress.Loopback, 0);
        listener.Start();
        var port = ((IPEndPoint)listener.LocalEndpoint).Port;
        var info = new ProcessStartInfo(Path.Combine(AppContext.BaseDirectory, "Entwurf.Bridge.exe"))
        { UseShellExecute = false, RedirectStandardInput = true, RedirectStandardOutput = true, RedirectStandardError = true };
        info.ArgumentList.Add("--automation-stdin");
        info.Environment["DOTNET_ROOT"] = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".dotnet");
        using var child = Process.Start(info)!;
        try
        {
            const string secret = "offline-owner-capability-never-in-query";
            await FrameCodec.WriteAsync(child.StandardInput.BaseStream, JsonSerializer.SerializeToUtf8Bytes(new
            {
                websocket_url = $"ws://127.0.0.1:{port}/api/automation/extension", token = secret,
                state_directory = directory, parent_pid = Environment.ProcessId
            }), deadline.Token);
            var accepting = listener.AcceptTcpClientAsync(deadline.Token).AsTask();
            var exited = child.WaitForExitAsync(deadline.Token);
            Assert.Same(accepting, await Task.WhenAny(accepting, exited));
            using var client = await accepting;
            using var stream = client.GetStream();
            var header = new List<byte>();
            while (header.Count < 8192)
            {
                var one = new byte[1];
                await stream.ReadExactlyAsync(one, deadline.Token);
                header.Add(one[0]);
                if (header.Count >= 4 && header.TakeLast(4).SequenceEqual("\r\n\r\n"u8.ToArray())) break;
            }
            var request = Encoding.ASCII.GetString(header.ToArray());
            Assert.StartsWith("GET /api/automation/extension HTTP/1.1\r\n", request);
            Assert.DoesNotContain(secret, request);
            var key = request.Split("\r\n").Single(x => x.StartsWith("Sec-WebSocket-Key:", StringComparison.OrdinalIgnoreCase)).Split(':', 2)[1].Trim();
            var accept = Convert.ToBase64String(SHA1.HashData(Encoding.ASCII.GetBytes(key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11")));
            await stream.WriteAsync(Encoding.ASCII.GetBytes($"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"), deadline.Token);
            using var socket = WebSocket.CreateFromStream(stream, true, null, TimeSpan.FromSeconds(30));
            var buffer = new byte[4096];
            var received = await socket.ReceiveAsync(buffer, deadline.Token);
            using var authentication = JsonDocument.Parse(buffer.AsMemory(0, received.Count));
            Assert.Equal("authenticate", authentication.RootElement.GetProperty("type").GetString());
            Assert.Equal(secret, authentication.RootElement.GetProperty("token").GetString());
            // No authenticated response is sent: these cases never inspect or operate ChatGPT.
            if (refuseAuthentication)
                await socket.SendAsync("{\"type\":\"error\",\"code\":\"unauthorized\"}"u8.ToArray(), WebSocketMessageType.Text, true, deadline.Token);
            else child.StandardInput.Close();
            await child.WaitForExitAsync(deadline.Token);
            Assert.Equal(refuseAuthentication ? 1 : 0, child.ExitCode);
            Assert.Empty(await child.StandardOutput.ReadToEndAsync());
            Assert.DoesNotContain(secret, await child.StandardError.ReadToEndAsync());
        }
        finally
        {
            if (!child.HasExited) { child.Kill(true); await child.WaitForExitAsync(); }
            Directory.Delete(directory, true);
        }
    }
}
