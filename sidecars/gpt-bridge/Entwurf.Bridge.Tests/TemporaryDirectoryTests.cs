using System.Diagnostics;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class TemporaryDirectoryTests
{
    [Fact]
    public async Task Cleanup_waits_for_a_transient_directory_handle_to_close()
    {
        var directory = Directory.CreateDirectory(Path.Combine(Path.GetTempPath(), "entwurf-cleanup-" + Guid.NewGuid())).FullName;
        using var handle = OpenDirectory(directory);
        try
        {
            var cleanup = TemporaryDirectory.DeleteAsync(directory);
            Assert.False(cleanup.IsCompleted);
            handle.Dispose();
            await cleanup.WaitAsync(TimeSpan.FromSeconds(3));
            Assert.False(Directory.Exists(directory));
        }
        finally
        {
            handle.Dispose();
            if (Directory.Exists(directory)) Directory.Delete(directory, true);
        }
    }

    [Fact]
    public async Task Cleanup_reports_a_retained_handle_within_its_deadline()
    {
        var directory = Directory.CreateDirectory(Path.Combine(Path.GetTempPath(), "entwurf-cleanup-" + Guid.NewGuid())).FullName;
        using var handle = OpenDirectory(directory);
        try
        {
            var elapsed = Stopwatch.StartNew();
            var error = await Assert.ThrowsAsync<IOException>(() => TemporaryDirectory.DeleteAsync(directory, TimeSpan.FromMilliseconds(100)));
            Assert.Equal(32, error.HResult & 0xffff);
            Assert.InRange(elapsed.Elapsed, TimeSpan.FromMilliseconds(100), TimeSpan.FromSeconds(2));
            Assert.True(Directory.Exists(directory));
        }
        finally { handle.Dispose(); if (Directory.Exists(directory)) Directory.Delete(directory, true); }
    }

    [Fact]
    public async Task Cleanup_does_not_suppress_nonsharing_errors()
    {
        var absent = Path.Combine(Path.GetTempPath(), "entwurf-absent-" + Guid.NewGuid());
        await Assert.ThrowsAsync<DirectoryNotFoundException>(() => TemporaryDirectory.DeleteAsync(absent));
    }

    // A real Windows handle without FILE_SHARE_DELETE reproduces ERROR_SHARING_VIOLATION.
    static SafeFileHandle OpenDirectory(string directory)
    {
        var handle = CreateFileW(directory, 0x80000000, FileShare.ReadWrite, IntPtr.Zero, FileMode.Open, 0x02000000, IntPtr.Zero);
        Assert.False(handle.IsInvalid, $"CreateFileW failed: {Marshal.GetLastWin32Error()}");
        return handle;
    }
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true, ExactSpelling = true)]
    static extern SafeFileHandle CreateFileW(string name, uint access, FileShare share, IntPtr security, FileMode mode, uint flags, IntPtr template);
}
