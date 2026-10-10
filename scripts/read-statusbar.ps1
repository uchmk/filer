<#
Read サクラエディタ's status bar as text -- TESTING.md 32.9a reads the line and
column there (#196 found it readable; #198 / #199 / #235 asked for the script
here, as sb.ps1 was then called in the x64 lane's evidence folder).

  scripts\read-statusbar.ps1 [-Match 'notes\.md'] [-Process sakura]

Finds the top-level windows of -Process whose title matches the -Match regex
(every window of it by default), and for each prints its title and the text of
every part of its `msctls_statusbar32`, one `[n] text` line per part. The status
bar belongs to another process, so the text is fetched with SB_GETTEXTW into a
buffer allocated in that process (VirtualAllocEx) and read back with
ReadProcessMemory.

32.9a passes when one part reads `<the heading's line> 行 1 桁`.

Exit code: 0 when at least one status bar was read; 1 when no window matched or
none had a status bar.

Written on Linux, where it cannot run: not yet run on Windows. The first lane
run that uses it checks it against the row by hand once.
#>
param(
    [string]$Match = '',
    [string]$Process = 'sakura'
)
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class KuraStatusBar {
    const uint WM_USER = 0x0400;
    const uint SB_GETPARTS = WM_USER + 6;
    const uint SB_GETTEXTLENGTHW = WM_USER + 12;
    const uint SB_GETTEXTW = WM_USER + 13;
    const uint PROCESS_ACCESS = 0x0008 | 0x0010 | 0x0020 | 0x0400; // VM_OPERATION | VM_READ | VM_WRITE | QUERY_INFORMATION
    const uint MEM_COMMIT_RESERVE = 0x3000;
    const uint MEM_RELEASE = 0x8000;
    const uint PAGE_READWRITE = 0x04;

    delegate bool EnumProc(IntPtr hwnd, IntPtr lParam);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr lParam);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowTextW(IntPtr hwnd, StringBuilder text, int max);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern IntPtr FindWindowExW(IntPtr parent, IntPtr after, string cls, string title);
    [DllImport("user32.dll")] static extern IntPtr SendMessageW(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam);
    [DllImport("kernel32.dll", SetLastError = true)] static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
    [DllImport("kernel32.dll", SetLastError = true)] static extern IntPtr VirtualAllocEx(IntPtr proc, IntPtr addr, UIntPtr size, uint type, uint protect);
    [DllImport("kernel32.dll")] static extern bool VirtualFreeEx(IntPtr proc, IntPtr addr, UIntPtr size, uint type);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool ReadProcessMemory(IntPtr proc, IntPtr addr, byte[] buf, UIntPtr size, out UIntPtr read);

    // Top-level visible windows of the given process ids, with their titles.
    public static List<KeyValuePair<IntPtr, string>> Windows(uint[] pids) {
        var found = new List<KeyValuePair<IntPtr, string>>();
        var want = new HashSet<uint>(pids);
        EnumWindows((hwnd, _) => {
            uint pid;
            GetWindowThreadProcessId(hwnd, out pid);
            if (want.Contains(pid) && IsWindowVisible(hwnd)) {
                var sb = new StringBuilder(512);
                GetWindowTextW(hwnd, sb, sb.Capacity);
                found.Add(new KeyValuePair<IntPtr, string>(hwnd, sb.ToString()));
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }

    // The text of every part of the window's status bar; null when it has none.
    public static string[] Parts(IntPtr top) {
        IntPtr bar = FindWindowExW(top, IntPtr.Zero, "msctls_statusbar32", null);
        if (bar == IntPtr.Zero) return null;
        uint pid;
        GetWindowThreadProcessId(bar, out pid);
        IntPtr proc = OpenProcess(PROCESS_ACCESS, false, pid);
        if (proc == IntPtr.Zero) throw new InvalidOperationException("OpenProcess failed: " + Marshal.GetLastWin32Error());
        try {
            int count = (int)SendMessageW(bar, SB_GETPARTS, IntPtr.Zero, IntPtr.Zero);
            var parts = new string[Math.Max(count, 0)];
            for (int i = 0; i < parts.Length; i++) {
                int len = (int)((long)SendMessageW(bar, SB_GETTEXTLENGTHW, (IntPtr)i, IntPtr.Zero) & 0xFFFF);
                int bytes = (len + 1) * 2;
                IntPtr remote = VirtualAllocEx(proc, IntPtr.Zero, (UIntPtr)bytes, MEM_COMMIT_RESERVE, PAGE_READWRITE);
                if (remote == IntPtr.Zero) throw new InvalidOperationException("VirtualAllocEx failed: " + Marshal.GetLastWin32Error());
                try {
                    SendMessageW(bar, SB_GETTEXTW, (IntPtr)i, remote);
                    var buf = new byte[bytes];
                    UIntPtr read;
                    if (!ReadProcessMemory(proc, remote, buf, (UIntPtr)bytes, out read))
                        throw new InvalidOperationException("ReadProcessMemory failed: " + Marshal.GetLastWin32Error());
                    parts[i] = Encoding.Unicode.GetString(buf, 0, len * 2);
                } finally {
                    VirtualFreeEx(proc, remote, UIntPtr.Zero, MEM_RELEASE);
                }
            }
            return parts;
        } finally {
            CloseHandle(proc);
        }
    }
}
'@

$pids = @(Get-Process -Name $Process -ErrorAction SilentlyContinue | ForEach-Object { [uint32]$_.Id })
if (-not $pids.Count) {
    Write-Output "read-statusbar: no $Process process is running"
    exit 1
}
$read = 0
foreach ($w in [KuraStatusBar]::Windows($pids)) {
    if ($Match -and $w.Value -notmatch $Match) { continue }
    $parts = [KuraStatusBar]::Parts($w.Key)
    if ($null -eq $parts) { continue }
    $read++
    Write-Output $w.Value
    for ($i = 0; $i -lt $parts.Count; $i++) { Write-Output "  [$i] $($parts[$i])" }
}
if (-not $read) {
    Write-Output "read-statusbar: no $Process window$(if ($Match) { " matching '$Match'" }) with a status bar"
    exit 1
}
exit 0
