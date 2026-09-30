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
#   pwsh -File scripts\keyprobe.ps1 -Query    # send tcell's startup queries and
#                                             # show the replies as they arrive
#   ... -Log probe.txt                        # also append every line to a file
#   ... -AltScreen                            # switch to the alternate screen,
#                                             # as a full-screen program does
#
# Every line starts with the time it was printed, in milliseconds since the
# Unix epoch -- the clock `FILER_PTY_LOG`'s `== pane opened` line also gives,
# so a key's `in key` line there and its records here can be set side by side
# and the time between them read off. -AltScreen is what makes filer forward
# `<S-End>` and the other scroll keys to the program instead of scrolling the
# pane itself, which is how a burst of them was seen to hold lazygit up (#93).
#
# Press q to stop.
#
# Records are printed in the order they arrive. Characters with no virtual key
# are joined into one `chars:` line until a record with a virtual key comes
# between them, because a sequence arrives one record per character and a line
# for each would bury it. `\e` is ESC.
#
# Do not pipe or redirect this script's output: the queries are written to the
# console, and with stdout redirected they would go into the pipe instead of
# reaching the terminal. Use -Log for a copy on disk.

param([switch]$Win32, [switch]$Query, [switch]$AltScreen, [string]$Log)

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

function Say([string]$line) {
    $line = '{0} {1}' -f [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds(), $line
    $line
    if ($Log) { Add-Content -Path $Log -Value $line }
}

function Show([int]$c) {
    if ($c -eq 27) { return '\e' }
    if ($c -eq 92) { return '\\' }
    if ($c -ge 32 -and $c -lt 127) { return [string][char]$c }
    return ('\x{0:X2}' -f $c)
}

$in = [KeyProbe]::GetStdHandle(-10)
$old = [uint32]0
[void][KeyProbe]::GetConsoleMode($in, [ref]$old)
# ENABLE_VIRTUAL_TERMINAL_INPUT | ENABLE_WINDOW_INPUT | ENABLE_EXTENDED_FLAGS.
# No ENABLE_PROCESSED_INPUT, so Ctrl+C arrives as a record rather than a signal.
[void][KeyProbe]::SetConsoleMode($in, 0x0200 -bor 0x0008 -bor 0x0080)

$mode = 'VT input'
if ($Win32) { $mode += ' + win32-input-mode' }
if ($Query) { $mode += ' + tcell startup queries' }
if ($AltScreen) { $mode += ' + alternate screen' }
Say "keyprobe: $mode. Press keys, then q to stop."

if ($AltScreen) { [Console]::Out.Write("$esc[?1049h") }
if ($Win32) { [Console]::Out.Write("$esc[?9001h") }
if ($Query) {
    # What tcell v3.5.0 sends at startup on Windows, in its order
    # (tscreen.go): DECRQM for resize reports, mouse buttons, SGR mouse and
    # win32-input-mode; the kitty keyboard query; XTVERSION; primary DA last.
    $queries = "$esc[?2048`$p$esc[?1000`$p$esc[?1006`$p$esc[?9001`$p$esc[?u$esc[>q$esc[c"
    [Console]::Out.Write($queries)
    [Console]::Out.Flush()
    Say ('sent:  ' + (($queries.ToCharArray() | ForEach-Object { Show ([int]$_) }) -join ''))
}

$buf = [KeyProbe+INPUT_RECORD[]]::new(64)
$seen = ''
try {
    while ($true) {
        $n = [uint32]0
        [void][KeyProbe]::ReadConsoleInputW($in, $buf, 64, [ref]$n)
        $chars = ''
        for ($i = 0; $i -lt $n; $i++) {
            $r = $buf[$i]
            if ($r.EventType -ne 1) { continue }
            $k = $r.Key
            $c = [int]$k.UnicodeChar
            if ($k.wVirtualKeyCode -eq 0 -and $k.wVirtualScanCode -eq 0) {
                if ($k.bKeyDown) { $chars += (Show $c) } else { $chars += '{up:' + (Show $c) + '}' }
            } else {
                if ($chars) { Say "chars: $chars"; $chars = '' }
                $dir = if ($k.bKeyDown) { 'down' } else { 'up  ' }
                Say ('{0} vk={1,3} sc={2,3} ch={3,-6} ctrl=0x{4:X}' -f $dir, $k.wVirtualKeyCode,
                    $k.wVirtualScanCode, (Show $c), $k.dwControlKeyState)
            }
            if ($k.bKeyDown) { $seen += [char]$c }
        }
        if ($chars) { Say "chars: $chars" }
        # `q` as a plain character, or as a win32-input-mode record for VK_Q (81).
        if ($seen -match 'q' -or $seen -match '\[81;') { break }
        if ($seen.Length -gt 200) { $seen = $seen.Substring($seen.Length - 50) }
    }
} finally {
    if ($Win32) { [Console]::Out.Write("$esc[?9001l") }
    if ($AltScreen) { [Console]::Out.Write("$esc[?1049l") }
    [void][KeyProbe]::SetConsoleMode($in, $old)
}
