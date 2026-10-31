using System.Diagnostics;
using System.Net.WebSockets;
using System.Text.Json;

namespace Entwurf.Bridge;

public sealed class AutomationWebSocketClient(Uri address, string token) : IDisposable
{
    readonly SemaphoreSlim writes = new(1, 1);
    ClientWebSocket? current;

    public async Task RunAsync(ImageAutomationSession session, CancellationToken cancellation)
    {
        while (!cancellation.IsCancellationRequested)
        {
            using var socket = new ClientWebSocket();
            using var connected = CancellationTokenSource.CreateLinkedTokenSource(cancellation);
            Task probes = Task.CompletedTask;
            socket.Options.KeepAliveInterval = TimeSpan.FromSeconds(20);
            socket.Options.KeepAliveTimeout = TimeSpan.FromSeconds(10);
            try
            {
                using var startup = CancellationTokenSource.CreateLinkedTokenSource(cancellation);
                startup.CancelAfter(TimeSpan.FromSeconds(10));
                await socket.ConnectAsync(address, startup.Token);
                await SendOnAsync(socket, ImageAutomationJson.Element(new { type = "authenticate", token }), startup.Token);
                using var authentication = await ReceiveAsync(socket, startup.Token);
                if (authentication is null || authentication.RootElement.GetProperty("type").GetString() != "authenticated")
                    throw new ImageAutomationException("unauthorized");
                Volatile.Write(ref current, socket);
                await session.ConnectedAsync(cancellation);
                probes = RefreshProbeAsync(session, connected.Token);
                while (socket.State == WebSocketState.Open && !cancellation.IsCancellationRequested)
                {
                    using var message = await ReceiveAsync(socket, cancellation);
                    if (message is null) break;
                    await session.HandleAsync(message.RootElement, cancellation);
                }
            }
            catch (Exception error) when (error is WebSocketException or IOException or OperationCanceledException)
            { if (cancellation.IsCancellationRequested) break; }
            finally
            {
                connected.Cancel();
                Interlocked.CompareExchange(ref current, null, socket);
                try { await probes.WaitAsync(TimeSpan.FromSeconds(3)); }
                catch (Exception error) when (error is OperationCanceledException or TimeoutException) { }
            }
            // Reconnect only replays durable statuses/artifacts. The session never runs a repeated start.
            await Task.Delay(TimeSpan.FromSeconds(2), cancellation);
        }
    }

    static async Task RefreshProbeAsync(ImageAutomationSession session, CancellationToken cancellation)
    {
        var probe = ImageAutomationJson.Element(new { type = "probe" });
        while (!cancellation.IsCancellationRequested)
        {
            await Task.Delay(TimeSpan.FromSeconds(5), cancellation);
            // Read-only capability refresh is independent of the command read loop and never schedules a run.
            await session.HandleAsync(probe, cancellation);
        }
    }

    public async Task SendAsync(JsonElement message, CancellationToken cancellation)
    {
        await writes.WaitAsync(cancellation);
        try
        {
            var socket = Volatile.Read(ref current);
            if (socket?.State != WebSocketState.Open) return;
            try { await SendOnAsync(socket, message, cancellation); }
            catch (WebSocketException) { socket.Abort(); }
        }
        finally { writes.Release(); }
    }
    static async Task SendOnAsync(WebSocket socket, JsonElement message, CancellationToken cancellation)
    {
        var bytes = JsonSerializer.SerializeToUtf8Bytes(message, ImageAutomationJson.Options);
        await socket.SendAsync(bytes, WebSocketMessageType.Text, true, cancellation);
    }
    static async Task<JsonDocument?> ReceiveAsync(WebSocket socket, CancellationToken cancellation)
    {
        using var payload = new MemoryStream();
        var buffer = new byte[8192];
        while (true)
        {
            var result = await socket.ReceiveAsync(buffer, cancellation);
            if (result.MessageType == WebSocketMessageType.Close) return null;
            if (result.MessageType != WebSocketMessageType.Text || payload.Length + result.Count > FrameCodec.MaximumBytes)
                throw new InvalidDataException("invalid_automation_frame");
            payload.Write(buffer, 0, result.Count);
            if (result.EndOfMessage) return JsonDocument.Parse(payload.ToArray(), new JsonDocumentOptions { MaxDepth = 32 });
        }
    }
    public void Dispose() { current?.Abort(); writes.Dispose(); }
}

public static class AutomationBootstrap
{
    public static async Task<int> RunAsync()
    {
        if (!Console.IsInputRedirected) return 2;
        try
        {
            using var input = Console.OpenStandardInput();
            using var startup = new CancellationTokenSource(TimeSpan.FromSeconds(15));
            var frame = await FrameCodec.ReadAsync(input, startup.Token) ?? throw new InvalidDataException("missing_bootstrap");
            var config = JsonSerializer.Deserialize<AutomationBootstrapConfig>(frame, ImageAutomationJson.Options) ?? throw new InvalidDataException("invalid_bootstrap");
            var address = config.Validate();
            using var parent = Process.GetProcessById(config.ParentPid);
            var parentStart = parent.StartTime.ToUniversalTime();
            using var lifetime = new CancellationTokenSource();
            using var transport = new AutomationWebSocketClient(address, config.Token);
            await using var session = new ImageAutomationSession(config.StateDirectory, new WindowsImageSurface(), transport.SendAsync);
            var inputClosed = MonitorInputAsync(input, lifetime);
            var parentExited = MonitorParentAsync(parent, parentStart, lifetime);
            try { await transport.RunAsync(session, lifetime.Token); }
            catch (OperationCanceledException) when (lifetime.IsCancellationRequested) { }
            finally
            {
                lifetime.Cancel();
                // Redirected Windows console reads can ignore token cancellation while ReadFile is pending.
                // They must not keep an authentication failure or parent shutdown alive indefinitely.
                input.Dispose();
                try { await Task.WhenAll(inputClosed, parentExited).WaitAsync(TimeSpan.FromSeconds(1)); }
                catch (TimeoutException) { }
            }
            return 0;
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException or JsonException or ArgumentException or InvalidOperationException
            or OperationCanceledException or System.ComponentModel.Win32Exception or ImageAutomationException)
        {
            Console.Error.WriteLine("Windows image automation stopped safely; reconnect through Entwurf.");
            return 1;
        }
    }
    static async Task MonitorInputAsync(Stream input, CancellationTokenSource lifetime)
    {
        try
        {
            var buffer = new byte[1];
            _ = await input.ReadAsync(buffer, lifetime.Token); // EOF or unexpected additional input ends this inherited capability.
            Stop(lifetime);
        }
        catch (OperationCanceledException) { }
        catch (Exception error) when (error is IOException or ObjectDisposedException) { Stop(lifetime); }
    }
    static async Task MonitorParentAsync(Process parent, DateTime started, CancellationTokenSource lifetime)
    {
        try
        {
            while (!lifetime.IsCancellationRequested)
            {
                if (parent.HasExited || parent.StartTime.ToUniversalTime() != started) { lifetime.Cancel(); return; }
                await Task.Delay(500, lifetime.Token);
            }
        }
        catch (OperationCanceledException) { }
        catch (Exception error) when (error is InvalidOperationException or System.ComponentModel.Win32Exception) { Stop(lifetime); }
    }
    static void Stop(CancellationTokenSource lifetime) { try { lifetime.Cancel(); } catch (ObjectDisposedException) { } }
}
