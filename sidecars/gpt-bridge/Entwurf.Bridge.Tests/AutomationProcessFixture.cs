using Entwurf.Bridge;

namespace Entwurf.Bridge.Tests;

// A real child process exercises inherited stdin and parent lifetime without any ChatGPT UI access.
internal static class AutomationProcessFixture
{
    public static Task<int> Main(string[] args) => args is ["--blocked-dispose-stdin"]
        ? AutomationBootstrap.RunAsync(() => new BlockedSurface()) : Task.FromResult(2);

    sealed class BlockedSurface : IImageAutomationSurface
    {
        readonly object gate = new();

        public BlockedSurface()
        {
            using var entered = new ManualResetEventSlim();
            var worker = new Thread(() =>
            {
                lock (gate)
                {
                    entered.Set();
                    Thread.Sleep(Timeout.Infinite); // Models a UIA call that never releases its surface lock.
                }
            }) { IsBackground = true };
            worker.Start();
            if (!entered.Wait(TimeSpan.FromSeconds(3))) throw new TimeoutException("fixture worker did not start");
        }

        public Task<ImageSurfaceProbe> ProbeAsync(CancellationToken cancellation) => throw new InvalidOperationException("fixture must not authenticate");
        public Task<PreparedImageConversation> PrepareAsync(ImageAutomationRun run, CancellationToken cancellation) => throw new InvalidOperationException();
        public Task WriteAndVerifyPromptAsync(PreparedImageConversation conversation, string prompt, CancellationToken cancellation) => throw new InvalidOperationException();
        public Task SubmitOnceAsync(PreparedImageConversation conversation, CancellationToken cancellation) => throw new InvalidOperationException();
        public Task WaitForGeneratedImageAsync(PreparedImageConversation conversation, CancellationToken cancellation) => throw new InvalidOperationException();
        public Task<string> DownloadOriginalAsync(PreparedImageConversation conversation, string ownedDirectory, CancellationToken cancellation) => throw new InvalidOperationException();
        public Task ClearOwnedDraftAsync(PreparedImageConversation conversation, string prompt, CancellationToken cancellation) => throw new InvalidOperationException();
        public void Dispose() { lock (gate) { } }
    }
}
