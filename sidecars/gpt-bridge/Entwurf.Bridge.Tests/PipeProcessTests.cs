using System.Diagnostics;
using System.IO.Pipes;
using System.Security.Cryptography;
using System.Text.Json;
using Entwurf.Bridge;
using Xunit;
using Xunit.Abstractions;

namespace Entwurf.Bridge.Tests;

public sealed class PipeProcessTests(ITestOutputHelper output)
{
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task Child_bootstrap_authentication_detection_and_pause_use_framed_named_pipe(bool standardInputBootstrap)
    {
        var directory = Path.Combine(Path.GetTempPath(), "entwurf-pipe-test-" + Guid.NewGuid());
        Directory.CreateDirectory(directory);
        using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(25));
        using var bootstrap = new AnonymousPipeServerStream(PipeDirection.Out, HandleInheritability.Inheritable);
        var pipeName = "entwurf-bridge-" + Guid.NewGuid().ToString("N");
        var secret = Convert.ToHexString(RandomNumberGenerator.GetBytes(32));
        var info = new ProcessStartInfo(Path.Combine(AppContext.BaseDirectory, "Entwurf.Bridge.exe"))
        { UseShellExecute = false, RedirectStandardOutput = true, RedirectStandardError = true, RedirectStandardInput = standardInputBootstrap };
        info.ArgumentList.Add(standardInputBootstrap ? "--bootstrap-stdin" : "--bootstrap-handle");
        if (!standardInputBootstrap) info.ArgumentList.Add(bootstrap.GetClientHandleAsString());
        // Native dotnet was installed for the current user; apphost needs its runtime path.
        info.Environment["DOTNET_ROOT"] = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".dotnet");
        using var child = Process.Start(info)!;
        try
        {
            bootstrap.DisposeLocalCopyOfClientHandle();
            await FrameCodec.WriteAsync(standardInputBootstrap ? child.StandardInput.BaseStream : bootstrap, JsonSerializer.SerializeToUtf8Bytes(new
            { pipe_name = pipeName, token = secret, artifact_directories = new[] { directory } }), deadline.Token);
            using var pipe = new NamedPipeClientStream(".", pipeName, PipeDirection.InOut, PipeOptions.Asynchronous | PipeOptions.CurrentUserOnly);
            await pipe.ConnectAsync(deadline.Token);
            using var denied = await Call("observe", 1, null);
            Assert.Equal("unauthorized", denied.RootElement.GetProperty("error").GetProperty("message").GetString());
            using var attached = await Call("attach", 2, new { token = secret });
            Assert.True(attached.RootElement.GetProperty("result").GetProperty("attached").GetBoolean());
            using var replay = await Call("attach", 3, new { token = secret });
            Assert.Equal("already_attached", replay.RootElement.GetProperty("error").GetProperty("message").GetString());
            using var detection = await Call("detect_app", 4, null);
            var report = detection.RootElement.GetProperty("result");
            Assert.False(report.GetProperty("automation_enabled").GetBoolean());
            Assert.False(report.GetProperty("gates").GetProperty("reliable_input").GetProperty("passed").GetBoolean());
            Assert.DoesNotContain(secret, report.GetRawText());
            output.WriteLine("READ_ONLY_DISCOVERY=" + report.GetRawText());
            // Both frames are sent before reading: pause must interrupt an outstanding watch.
            await FrameCodec.WriteAsync(pipe, JsonSerializer.SerializeToUtf8Bytes(new
            { jsonrpc = "2.0", id = 5, method = "watch_artifact", @params = new { task_id = "pipe-test", revision = 1, timeout_ms = 30000 } }), deadline.Token);
            await FrameCodec.WriteAsync(pipe, JsonSerializer.SerializeToUtf8Bytes(new
            { jsonrpc = "2.0", id = 6, method = "pause" }), deadline.Token);
            using var first = JsonDocument.Parse((await FrameCodec.ReadAsync(pipe, deadline.Token))!);
            using var second = JsonDocument.Parse((await FrameCodec.ReadAsync(pipe, deadline.Token))!);
            var replies = new[] { first.RootElement, second.RootElement }.ToDictionary(x => x.GetProperty("id").GetInt32());
            Assert.Equal("cancelled", replies[5].GetProperty("error").GetProperty("message").GetString());
            Assert.Equal("paused", replies[6].GetProperty("result").GetProperty("state").GetString());
            pipe.Dispose();
            await child.WaitForExitAsync(deadline.Token);
            Assert.Equal(0, child.ExitCode);
            Assert.Empty(await child.StandardOutput.ReadToEndAsync());
            Assert.Empty(await child.StandardError.ReadToEndAsync());

            async Task<JsonDocument> Call(string method, int id, object? parameters)
            {
                var request = JsonSerializer.SerializeToUtf8Bytes(new { jsonrpc = "2.0", id, method, @params = parameters ?? new { } });
                await FrameCodec.WriteAsync(pipe, request, deadline.Token);
                return JsonDocument.Parse((await FrameCodec.ReadAsync(pipe, deadline.Token))!);
            }
        }
        finally { if (!child.HasExited) { child.Kill(entireProcessTree: true); await child.WaitForExitAsync(); } Directory.Delete(directory, true); }
    }
}
