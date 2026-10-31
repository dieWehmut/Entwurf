using System.IO.Pipes;
using System.Text.Json;
using System.Text.RegularExpressions;
using Entwurf.Bridge;

// Only the inherited anonymous pipe handle appears on the command line; secrets never do.
if (args.Length != 2 || args[0] != "--bootstrap-handle") return 2;
try
{
    using var startup = new CancellationTokenSource(TimeSpan.FromSeconds(15));
    using var bootstrap = new AnonymousPipeClientStream(PipeDirection.In, args[1]);
    var data = await FrameCodec.ReadAsync(bootstrap, startup.Token) ?? throw new InvalidDataException();
    var config = JsonSerializer.Deserialize<Bootstrap>(data, WireJson.Options) ?? throw new InvalidDataException();
    if (config.PipeName is null || config.Token is null || config.ArtifactDirectories is null || config.ArtifactDirectories.Length > 16 || !Regex.IsMatch(config.PipeName, "^entwurf-bridge-[a-f0-9]{32}$") || !Regex.IsMatch(config.Token, "^[a-fA-F0-9]{64}$")) return 2;
    using var host = new BridgeHost(new WindowsAutomation(), TextWriter.Null, config.Token, config.ArtifactDirectories);
    using var pipe = PipeServer.Create(config.PipeName);
    await pipe.WaitForConnectionAsync(startup.Token);
    await PipeServer.ServeAsync(pipe, host, CancellationToken.None);
    return 0;
}
catch (Exception e) when (e is IOException or OperationCanceledException or JsonException or ArgumentException or BridgeException)
{
    Console.Error.WriteLine("Bridge stopped safely; reconnect through the parent application.");
    return 1;
}

internal sealed record Bootstrap(string PipeName, string Token, string[] ArtifactDirectories);
