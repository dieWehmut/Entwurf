using System.Diagnostics;
using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativeImageSaveLifecycleTests
{
    [Fact]
    public async Task A_slow_original_write_succeeds_while_the_same_dialog_is_still_visible()
    {
        var directory = Path.Combine(Path.GetTempPath(), "entwurf-save-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        var destination = Path.Combine(directory, "owned-original.png");
        var bytes = new byte[] { 137, 80, 78, 71, 13, 10, 26, 10, 42 };
        FileStream? writer = null;
        var saveCalls = 0; var dialogReads = 0; var blockedReads = 0;
        try
        {
            NativeSaveDialog ReadDialog()
            {
                if (++dialogReads == 3) { writer!.Dispose(); writer = null; }
                return new("owned-dialog", () =>
                {
                    saveCalls++;
                    writer = new FileStream(destination, FileMode.CreateNew, FileAccess.Write, FileShare.None);
                    writer.Write(bytes); writer.Flush(true);
                });
            }
            bool FileReady()
            {
                try
                {
                    using var file = new FileStream(destination, FileMode.Open, FileAccess.Read, FileShare.None);
                    return file.Length == bytes.Length;
                }
                catch (IOException) { blockedReads++; return false; }
            }
            await NativeImageSaveLifecycle.WaitAsync(ReadDialog, FileReady, default,
                TimeSpan.FromSeconds(5), TimeSpan.FromMilliseconds(1));
            Assert.Equal(3, dialogReads);
            Assert.Equal(2, blockedReads);
            Assert.Equal(1, saveCalls);
            Assert.Equal(bytes, await File.ReadAllBytesAsync(destination));
        }
        finally { writer?.Dispose(); Directory.Delete(directory, true); }
    }

    [Fact]
    public async Task A_dialog_that_closes_before_file_delivery_does_not_trigger_a_second_save()
    {
        var dialogReads = 0; var fileReads = 0; var saveCalls = 0;
        await NativeImageSaveLifecycle.WaitAsync(
            () => ++dialogReads == 1 ? new("owned-dialog", () => saveCalls++) : null,
            () => ++fileReads == 3, default, TimeSpan.FromSeconds(5), TimeSpan.FromMilliseconds(1));
        Assert.Equal(3, dialogReads);
        Assert.Equal(1, saveCalls);
    }

    [Fact]
    public async Task A_still_visible_dialog_waits_until_the_deadline_without_repeating_save()
    {
        var saves = 0; var elapsed = Stopwatch.StartNew();
        var timeout = TimeSpan.FromMilliseconds(100);
        var error = await Assert.ThrowsAsync<ImageAutomationException>(() => NativeImageSaveLifecycle.WaitAsync(
            () => new("owned-dialog", () => saves++), () => false, default, timeout, TimeSpan.FromMilliseconds(1)));
        Assert.Equal("download_failed", error.Reason);
        Assert.True(elapsed.Elapsed >= timeout);
        Assert.Equal(1, saves);
    }

    [Fact]
    public async Task A_replacement_dialog_is_refused_without_writing_or_saving_in_it()
    {
        var reads = 0; var ownedSaves = 0; var replacementSaves = 0;
        var error = await Assert.ThrowsAsync<ImageAutomationException>(() => NativeImageSaveLifecycle.WaitAsync(
            () => ++reads == 1 ? new("owned-dialog", () => ownedSaves++) : new("replacement-dialog", () => replacementSaves++),
            () => false, default, TimeSpan.FromSeconds(5), TimeSpan.FromMilliseconds(1)));
        Assert.Equal("download_failed", error.Reason);
        Assert.Equal(1, ownedSaves);
        Assert.Equal(0, replacementSaves);
    }

    [Fact]
    public async Task The_same_dialog_waits_for_its_controls_before_saving_or_reading_the_file()
    {
        var reads = 0; var saves = 0; var fileReads = 0;
        await NativeImageSaveLifecycle.WaitAsync(
            () => new("owned-dialog", () => saves++, Ready: ++reads == 3),
            () => { fileReads++; Assert.Equal(3, reads); return true; }, default,
            TimeSpan.FromSeconds(5), TimeSpan.FromMilliseconds(1));
        Assert.Equal(3, reads);
        Assert.Equal(1, saves);
        Assert.Equal(1, fileReads);
    }

    [Fact]
    public async Task A_replacement_before_controls_are_ready_is_refused_without_saving()
    {
        var reads = 0; var saves = 0; var fileReads = 0;
        var error = await Assert.ThrowsAsync<ImageAutomationException>(() => NativeImageSaveLifecycle.WaitAsync(
            () => new(++reads == 1 ? "owned-dialog" : "replacement-dialog", () => saves++, Ready: reads > 1),
            () => { fileReads++; return true; }, default, TimeSpan.FromSeconds(5), TimeSpan.FromMilliseconds(1)));
        Assert.Equal("download_dialog_unavailable", error.Reason);
        Assert.Equal(0, saves);
        Assert.Equal(0, fileReads);
    }

    [Fact]
    public async Task A_dialog_that_never_becomes_ready_times_out_without_saving()
    {
        var saves = 0;
        var error = await Assert.ThrowsAsync<ImageAutomationException>(() => NativeImageSaveLifecycle.WaitAsync(
            () => new("owned-dialog", () => saves++, Ready: false), () => true, default,
            TimeSpan.FromMilliseconds(100), TimeSpan.FromMilliseconds(1)));
        Assert.Equal("download_dialog_unavailable", error.Reason);
        Assert.Equal(0, saves);
    }

    [Fact]
    public async Task File_availability_alone_cannot_succeed_before_an_owned_save()
    {
        var fileReads = 0;
        var error = await Assert.ThrowsAsync<ImageAutomationException>(() => NativeImageSaveLifecycle.WaitAsync(
            () => null, () => { fileReads++; return true; }, default,
            TimeSpan.FromMilliseconds(100), TimeSpan.FromMilliseconds(1)));
        Assert.Equal("download_dialog_unavailable", error.Reason);
        Assert.Equal(0, fileReads);
    }

    [Fact]
    public async Task Cancellation_after_dialog_observation_prevents_save_dispatch()
    {
        using var cancellation = new CancellationTokenSource(); var saves = 0;
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => NativeImageSaveLifecycle.WaitAsync(
            () => { cancellation.Cancel(); return new("owned-dialog", () => saves++); }, () => true, cancellation.Token));
        Assert.Equal(0, saves);
    }

    [Fact]
    public async Task Cancellation_during_file_observation_cannot_report_success()
    {
        using var cancellation = new CancellationTokenSource(); var saves = 0;
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => NativeImageSaveLifecycle.WaitAsync(
            () => new("owned-dialog", () => saves++), () => { cancellation.Cancel(); return true; }, cancellation.Token));
        Assert.Equal(1, saves);
    }
}
