# Show the console key records a program actually receives.
#
# Reads input the way tcell does (lazygit, gh-dash): the console in
# virtual-terminal input mode, records taken with ReadConsoleInputW. Each
# record is printed with its virtual key, scan code and character, so the
# same key pressed in filer's pane and in Windows Terminal can be compared.
#
#   pwsh -File scripts\keyprobe.ps1           # VT input, as tcell starts
#   pwsh -File scripts\keyprobe.ps1 -Win32    # also ask for win32-input-mode,
#                                             # as tcell does after negotiating
#
# Press the key under test (Esc), then `q` to stop.
#
# Characters that arrive with no virtual key are gathered into one `chars:`
# line per read, because that is how a sequence arrives -- one record per
# character -- and a line per record would bury it.

param([switch]$Win32)

Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class KeyProbe {
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct KEY_EVENT_RECORD {
        public int bKeyDown;
        public ushort wRepeatCount;
        public ushort wVirtualKeyCode;
        public ushort wVirtualScanCode;
        public char UnicodeChar;
        public uint dwControlKeyState;
    }
    [StructLayout(LayoutKind.Explicit, CharSet = CharSet.Unicode)]
    public struct INPUT_RECORD {
        [FieldOffset(0)] public ushort EventType;
        [FieldOffset(4)] public KEY_EVENT_RECORD Key;
    }
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern IntPtr GetStdHandle(int n);
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool GetConsoleMode(IntPtr h, out uint mode);
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool SetConsoleMode(IntPtr h, uint mode);
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    public static extern bool ReadConsoleInputW(IntPtr h, [Out] INPUT_RECORD[] buf, uint len, out uint read);
}
"@

$esc = [char]27
$in = [KeyProbe]::GetStdHandle(-10)
$old = [uint32]0
[void][KeyProbe]::GetConsoleMode($in, [ref]$old)
# ENABLE_VIRTUAL_TERMINAL_INPUT | ENABLE_WINDOW_INPUT | ENABLE_EXTENDED_FLAGS.
# No ENABLE_PROCESSED_INPUT, so Ctrl+C arrives as a record rather than a signal.
[void][KeyProbe]::SetConsoleMode($in, 0x0200 -bor 0x0008 -bor 0x0080)
if ($Win32) { [Console]::Out.Write("$esc[?9001h") }

$label = if ($Win32) { 'VT input + win32-input-mode' } else { 'VT input' }
"keyprobe: $label. Press Esc, then q to stop."

function Show([int]$c) {
    if ($c -eq 27) { return 'ESC' }
    if ($c -ge 32 -and $c -lt 127) { return [string][char]$c }
    return ('<0x{0:X2}>' -f $c)
}

$buf = [KeyProbe+INPUT_RECORD[]]::new(32)
$seen = ''
try {
    while ($true) {
        $n = [uint32]0
        [void][KeyProbe]::ReadConsoleInputW($in, $buf, 32, [ref]$n)
        $chars = ''
        for ($i = 0; $i -lt $n; $i++) {
            $r = $buf[$i]
            if ($r.EventType -ne 1) { continue }
            $k = $r.Key
            $c = [int]$k.UnicodeChar
            if ($k.wVirtualKeyCode -eq 0 -and $k.wVirtualScanCode -eq 0) {
                if ($k.bKeyDown) { $chars += (Show $c) }
            } else {
                $dir = if ($k.bKeyDown) { 'down' } else { 'up  ' }
                '{0} vk={1,3} sc={2,3} ch={3,-6} ctrl=0x{4:X}' -f $dir, $k.wVirtualKeyCode,
                    $k.wVirtualScanCode, (Show $c), $k.dwControlKeyState
            }
            if ($k.bKeyDown) { $seen += [char]$c }
        }
        if ($chars) { "chars: $chars" }
        # `q` as a plain character, or as a win32-input-mode record for VK_Q (81).
        if ($seen -match 'q' -or $seen -match '\[81;') { break }
        if ($seen.Length -gt 200) { $seen = $seen.Substring($seen.Length - 50) }
    }
} finally {
    if ($Win32) { [Console]::Out.Write("$esc[?9001l") }
    [void][KeyProbe]::SetConsoleMode($in, $old)
}
