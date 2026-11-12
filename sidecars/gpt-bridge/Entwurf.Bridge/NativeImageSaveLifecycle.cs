using System.Diagnostics;

namespace Entwurf.Bridge;

internal sealed record NativeSaveDialog(string Id, Action Save, bool Ready = true);

internal static class NativeImageSaveLifecycle
{
    internal static async Task WaitAsync(Func<NativeSaveDialog?> readOwnedDialog, Func<bool> fileReady,
        CancellationToken cancellation, TimeSpan? timeout = null, TimeSpan? pollInterval = null)
    {
        string? observedDialogId = null;
        var saveDispatched = false;
        var elapsed = Stopwatch.StartNew();
        while (elapsed.Elapsed < (timeout ?? TimeSpan.FromSeconds(25)))
        {
            cancellation.ThrowIfCancellationRequested();
            var dialog = readOwnedDialog();
            if (dialog is not null)
            {
                if (observedDialogId is not null && dialog.Id != observedDialogId)
                    throw new ImageAutomationException(saveDispatched ? "download_failed" : "download_dialog_unavailable");
                observedDialogId ??= dialog.Id;
                if (!saveDispatched && dialog.Ready)
                {
                    // UIA Invoke can return before the native dialog closes. Pin
                    // this dialog and dispatch Save once, including uncertain calls.
                    cancellation.ThrowIfCancellationRequested();
                    saveDispatched = true;
                    dialog.Save();
                }
            }
            if (saveDispatched)
            {
                var ready = fileReady();
                cancellation.ThrowIfCancellationRequested();
                if (ready) return;
            }
            await Task.Delay(pollInterval ?? TimeSpan.FromMilliseconds(300), cancellation);
        }
        throw new ImageAutomationException(saveDispatched ? "download_failed" : "download_dialog_unavailable");
    }
}
