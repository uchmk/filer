# Start the Windows test session by itself when there is new work for it.
#
# The session on the Windows machine (.claude/windows-role.md) is the only one
# that can run filer, and until now a person had to start it by hand for every
# section. This looks once, and starts one unattended run if:
#
#   - origin/main has changed TESTING.md, TESTING-CHECKS.md or
#     .claude/windows-role.md since the last run it started -- a merged run
#     changes the last two, so merging one pull request is what starts the next;
#   - no pull request from this lane (test/win-* or test/arm-*) is still open
#     (that section is not done yet, and a second run would take the same one:
#     it happened on 2026-09-28);
#   - the screen is not locked (SendInput does nothing on a locked desktop, and
#     the run would record every key as having done nothing);
#   - the working copy is clean (a dirty one is a run that was cut off, and
#     only a person can say what to do with it).
#
# It is meant to be run by Task Scheduler every 15 minutes, as you, "only when
# the user is logged on" -- the run drives a real window:
#
#   $a = New-ScheduledTaskAction -Execute pwsh -Argument '-NoProfile -WindowStyle Hidden -File C:\dev\filer-wintest\scripts\auto-wintest.ps1'
#   $t = New-ScheduledTaskTrigger -Once -At (Get-Date) -RepetitionInterval (New-TimeSpan -Minutes 15)
#   $s = New-ScheduledTaskSettingsSet -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Hours 4)
#   Register-ScheduledTask -TaskName filer-auto-wintest -Action $a -Trigger $t -Settings $s
#
#   Unregister-ScheduledTask -TaskName filer-auto-wintest    # to stop it
#
# Run the worktree's copy, not this one: register the task with
# -File C:\dev\filer-wintest\scripts\auto-wintest.ps1 (C:\dev\filer-armtest\...
# for -Lane arm). The worktree is moved to origin/main at every firing, so the
# script that runs is always the newest, and the checkout you work in is never
# touched. Until v0.73.24 the task ran the copy in that checkout, which only
# moved when someone pulled: the ARM64 laptop ran v0.51.1 for days (#201). The
# very first time, before the worktree exists, run this copy once by hand to
# make it (without -Force it makes the worktree and stops if there is nothing
# to run). To move an existing task over:
#
#   # x64:   filer-auto-wintest,     C:\dev\filer-wintest
#   # ARM64: filer-auto-wintest-arm, C:\dev\filer-armtest
#   $t = Get-ScheduledTask filer-auto-wintest
#   $a = $t.Actions[0]
#   $a.Arguments = $a.Arguments -replace [regex]::Escape('C:\dev\filer\scripts'), 'C:\dev\filer-wintest\scripts'
#   Set-ScheduledTask -TaskName $t.TaskName -Action $a
#
# By hand:
#
#   pwsh -File scripts\auto-wintest.ps1          # look once, run if there is work
#   pwsh -File scripts\auto-wintest.ps1 -Force   # run even if nothing changed
#   ... -LogDir R:\Temp                           # the log on the RAM disk
#   ... -Lane arm                                 # the ARM64 machine's lane
#   ... -TargetOnDisk                             # keep the build output on C:
#
# Lanes. The x64 machine runs lane `win` (the default), the ARM64 laptop lane
# `arm`. Each has its own queue in windows-role.md, its own branch prefix
# (test/win-, test/arm-), its own worktree and state, and waits only on its
# own pull requests -- so the two machines never take the same section and
# never hold each other up.
#
# Scratch space. Each run gets a folder of its own, run-<time>, under -Scratch,
# which defaults to R:\Temp when there is an R: drive (the RAM disk on the x64
# machine) and to %TEMP%\filer-scratch otherwise. TEMP and TMP point into it and
# the prompt says where it is; `cargo test` builds its test trees under TEMP, so
# they follow. Only the newest three run-* folders are kept: on the ARM64
# laptop, with no RAM disk to empty itself, ten runs' leftovers had piled up and
# an old run's script error dialog was still open in the middle of the screen
# (#103). Nothing else under -Scratch is touched.
#
# The screen saver is held off for the length of a run (-KeepScreenSaver
# leaves it alone). A screen saver owns the input desktop, and SendInput then
# goes nowhere without an error: on the ARM64 laptop three runs in a row lost
# every mouse row that way (#93, #100). Three layers, because the laptop's saver
# is not Windows' own -- ASUS OLED Care starts `OLED Care Screensaver.scr` by
# itself, with Windows' screen saver set to (None):
#
#   - SetThreadExecutionState(ES_DISPLAY_REQUIRED) for as long as the run lasts,
#     which is what keeps a video player's screen on;
#   - Windows' own saver switched off in memory only (SPI_SETSCREENSAVEACTIVE
#     with no SPIF_UPDATEINIFILE), so nothing is written to the profile, and
#     the old value put back afterwards;
#   - a watcher that stops any running `*.scr` every 5 seconds, for a saver
#     that heeds neither of the above.
#
# Everything is undone in `finally`. A run killed outright never reaches it, so
# the old value is also written to screensaver.json first, and the next firing
# puts it back before doing anything else.
#
# Log: %LOCALAPPDATA%\filer-wintest\auto-wintest.log, or in -LogDir. The log
# may go on the RAM disk (-LogDir R:\Temp): it is for reading what a run did,
# not evidence. The state file stays in %LOCALAPPDATA% whatever -LogDir says --
# on R: it would be gone after a reboot, and the next firing would start a run
# for a trigger that was already used.
#
# Build output. The worktree's `target` is made a junction to a folder on the
# RAM disk, R:\cargo-target\<worktree name> (or -TargetDir), so the role's
# paths (`target\release\filer.exe`) still work and the bytes live in memory.
# On 2026-10-03 C:\dev held 45 GB, and the two `target` folders were 33 GB of
# it. After a reboot the RAM disk is empty: the folder is made again and the
# first build takes a few minutes longer, nothing worse. With less than 8 GB
# free on R: the run builds on C: as before, and says so in the log.
# -TargetOnDisk turns all of this off. Incremental compilation is off for the
# run as well: an unattended run builds once, and its caches were 6 GB.
#
# The run works in its own worktree ($Work), not in the checkout you use, so
# it never meets your uncommitted changes and you can keep working while it
# runs. Needs `claude` and an authenticated `gh` on PATH.
#
# What the run may do is below in $Tools. It is broad on purpose -- a test run
# writes and runs its own PowerShell to drive the window -- so the guard is the
# role definition, plus the few commands it must never run, in $Denied.

param(
    [ValidateSet('win', 'arm')] [string]$Lane = 'win',
    [string]$Work,
    [string]$LogDir,
    [string]$Scratch,
    [switch]$Force,
    [switch]$KeepScreenSaver,
    [string]$TargetDir,
    [switch]$TargetOnDisk
)

$ErrorActionPreference = 'Stop'

$repo = 'uchmk/filer'
$watched = @('TESTING.md', 'TESTING-CHECKS.md', '.claude/windows-role.md')
$Tools = 'Bash,PowerShell,Read,Edit,Write,Glob,Grep,TodoWrite'
$Denied = @(
    'Bash(git push origin main:*)', 'PowerShell(git push origin main:*)',
    'Bash(git push -f:*)', 'PowerShell(git push -f:*)',
    'Bash(git push --force:*)', 'PowerShell(git push --force:*)',
    'Bash(gh pr merge:*)', 'PowerShell(gh pr merge:*)',
    'Bash(cargo fmt:*)', 'PowerShell(cargo fmt:*)'
) -join ','

# The `win` lane keeps the names it had before lanes existed, so a machine
# already running it carries on from its state file.
$suffix = if ($Lane -eq 'win') { '' } else { "-$Lane" }
if (-not $Work) { $Work = if ($Lane -eq 'win') { 'C:\dev\filer-wintest' } else { "C:\dev\filer-$($Lane)test" } }
if (-not $Scratch) {
    $Scratch = if (Test-Path 'R:\') { 'R:\Temp' } else { Join-Path ([IO.Path]::GetTempPath()) 'filer-scratch' }
}
$queue = if ($Lane -eq 'win') { 'the queue in "Where the work is"' } else { 'the ARM64 queue in "The ARM64 lane"' }

$state = Join-Path $env:LOCALAPPDATA 'filer-wintest'
New-Item -ItemType Directory -Force -Path $state | Out-Null
if (-not $LogDir) { $LogDir = $state }
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
New-Item -ItemType Directory -Force -Path $Scratch | Out-Null
$log = Join-Path $LogDir "auto-wintest$suffix.log"
$last = Join-Path $state "last-trigger$suffix"

# Points $Work\target at the RAM disk (see the top). Returns where the build
# output goes, for the log.
function Set-BuildTarget {
    $link = Join-Path $Work 'target'
    if ($TargetOnDisk) { return $link }
    $dir = if ($TargetDir) { $TargetDir } elseif (Test-Path 'R:\') { Join-Path 'R:\cargo-target' (Split-Path -Leaf $Work) } else { $null }
    if (-not $dir) { return $link }
    $drive = Get-PSDrive -Name ($dir.Substring(0, 1)) -ErrorAction SilentlyContinue
    $item = Get-Item -LiteralPath $link -Force -ErrorAction SilentlyContinue
    $isLink = $item -and ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)
    if (-not $isLink -and $drive -and $drive.Free -lt 8GB) {
        # Out-Host: Say also writes to the output, which here is the return value.
        Say ("Only {0:N1} GB free on {1}: building in {2} this time." -f ($drive.Free / 1GB), $drive.Root, $link) | Out-Host
        return $link
    }
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    if ($isLink -and ("$($item.Target)" -eq $dir)) { return $dir }
    if ($isLink) {
        [IO.Directory]::Delete($link)                    # the junction only, not what it points at
    } elseif ($item) {
        $gb = (Get-ChildItem -LiteralPath $link -Recurse -File -Force -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum / 1GB
        Remove-Item -Recurse -Force -LiteralPath $link   # build output: the next build makes it again
        Say ("Removed {0} ({1:N1} GB) to build on {2} instead." -f $link, $gb, $dir) | Out-Host
    }
    New-Item -ItemType Junction -Path $link -Target $dir | Out-Null
    return $dir
}

function Say([string]$line) {
    $stamped = '[{0:yyyy-MM-dd HH:mm:ss}] {1}' -f (Get-Date), $line
    $stamped
    Add-Content -Path $log -Value $stamped
}

# The screen saver, held off while a run drives the window (see the top).
$saverFile = Join-Path $state "screensaver$suffix.json"
Add-Type -Namespace FilerWintest -Name Power -MemberDefinition @'
[DllImport("kernel32.dll")]
public static extern uint SetThreadExecutionState(uint flags);
[DllImport("user32.dll", SetLastError = true)]
public static extern bool SystemParametersInfo(uint action, uint param, ref bool value, uint winIni);
[DllImport("user32.dll", SetLastError = true)]
public static extern bool SystemParametersInfo(uint action, uint param, System.IntPtr value, uint winIni);
[DllImport("user32.dll", SetLastError = true)]
public static extern System.IntPtr OpenInputDesktop(uint flags, bool inherit, uint access);
[DllImport("user32.dll", SetLastError = true)]
public static extern bool CloseDesktop(System.IntPtr desktop);
[DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
public static extern bool GetUserObjectInformation(System.IntPtr obj, int index, System.Text.StringBuilder info, int length, out int needed);
'@
$ES_CONTINUOUS = [uint32]'0x80000000'
$ES_SYSTEM_REQUIRED = [uint32]1
$ES_DISPLAY_REQUIRED = [uint32]2
$SPI_GETSCREENSAVEACTIVE = [uint32]16
$SPI_SETSCREENSAVEACTIVE = [uint32]17

function Get-SaverActive {
    $on = $false
    [void][FilerWintest.Power]::SystemParametersInfo($SPI_GETSCREENSAVEACTIVE, 0, [ref]$on, 0)
    $on
}

# In memory only: winIni 0, so the profile keeps whatever the owner chose.
function Set-SaverActive([bool]$on) {
    [void][FilerWintest.Power]::SystemParametersInfo($SPI_SETSCREENSAVEACTIVE, [uint32][int]$on, [IntPtr]::Zero, 0)
}

# A run killed before its `finally` left the saver off; put it back first.
function Restore-LeftOverSaver {
    if (-not (Test-Path $saverFile)) { return }
    $was = (Get-Content -Raw $saverFile | ConvertFrom-Json).active
    Set-SaverActive $was
    Remove-Item $saverFile
    Say "Put the screen saver back (active = $was), left off by a run that was cut off."
}

# The desktop that receives input: `Default` when keys and clicks reach the
# windows on it, `Screen-saver` or `Winlogon` when they go nowhere. Asked
# rather than inferred from LogonUI, which a screen saver does not start (#88).
function Get-InputDesktop {
    $h = [FilerWintest.Power]::OpenInputDesktop(0, $false, 0x0001)   # DESKTOP_READOBJECTS
    if ($h -eq [IntPtr]::Zero) { return '(none: OpenInputDesktop failed)' }
    try {
        $name = [Text.StringBuilder]::new(256)
        $needed = 0
        [void][FilerWintest.Power]::GetUserObjectInformation($h, 2, $name, 512, [ref]$needed)   # UOI_NAME
        $name.ToString()
    } finally {
        [void][FilerWintest.Power]::CloseDesktop($h)
    }
}

function Stop-ScreenSavers {
    Get-Process | Where-Object { $_.Path -like '*.scr' } | ForEach-Object {
        Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
        $_.Path
    }
}

function Suspend-ScreenSaver {
    $was = Get-SaverActive
    @{ active = $was } | ConvertTo-Json | Set-Content -Path $saverFile
    Set-SaverActive $false
    [void][FilerWintest.Power]::SetThreadExecutionState($ES_CONTINUOUS -bor $ES_SYSTEM_REQUIRED -bor $ES_DISPLAY_REQUIRED)
    foreach ($p in Stop-ScreenSavers) { Say "Stopped a running screen saver: $p" }
    # The watcher. A thread job shares nothing with this script but what it is
    # given, so the function goes in as text.
    $body = ${function:Stop-ScreenSavers}.ToString()
    $script:saverWatch = Start-ThreadJob -ArgumentList $body -ScriptBlock {
        param($body)
        $stop = [scriptblock]::Create($body)
        while ($true) {
            foreach ($p in & $stop) { "{0:HH:mm:ss} stopped {1}" -f (Get-Date), $p }
            Start-Sleep -Seconds 5
        }
    }
    Say "Holding the screen saver off for the run (it was active = $was)."
}

function Resume-ScreenSaver {
    if ($script:saverWatch) {
        $stopped = Receive-Job $script:saverWatch -ErrorAction SilentlyContinue
        Remove-Job $script:saverWatch -Force
        $script:saverWatch = $null
        if ($stopped) { Say "During the run the watcher stopped a screen saver $(@($stopped).Count) time(s): $(@($stopped)[-1])" }
    }
    [void][FilerWintest.Power]::SetThreadExecutionState($ES_CONTINUOUS)
    if (Test-Path $saverFile) {
        $was = (Get-Content -Raw $saverFile | ConvertFrom-Json).active
        Set-SaverActive $was
        Remove-Item $saverFile
        Say "Gave the screen saver back (active = $was)."
    }
}

# Task Scheduler's IgnoreNew already keeps its own firings apart; this also
# covers one started by hand while a scheduled one is running.
$mutex = [Threading.Mutex]::new($false, "Local\filer-auto-wintest$suffix")
if (-not $mutex.WaitOne(0)) { Say 'A run is already going. Nothing to do.'; exit 0 }

try {
    Restore-LeftOverSaver

    if (Get-Process LogonUI -ErrorAction SilentlyContinue) {
        Say 'The screen is locked. Trying again next time.'
        exit 0
    }

    if (-not (Test-Path $Work)) {
        # The worktree hangs off the checkout this script is in.
        $main = Split-Path -Parent $PSScriptRoot
        git -C $main fetch -q origin main
        git -C $main worktree add -q --detach $Work origin/main
        Say "Made the worktree $Work."
    }

    git -C $Work fetch -q origin main
    if ($LASTEXITCODE -ne 0) { Say 'git fetch failed. Trying again next time.'; exit 0 }
    # Kept on origin/main at every firing, not only when a run starts, so the
    # copy of this script inside it is the newest by the next firing (see the
    # top: the task runs that copy). A worktree with changes in it is left to
    # the check further down.
    if (-not (git -C $Work status --porcelain)) { git -C $Work checkout -q --detach origin/main }

    $trigger = (git -C $Work log -1 --format=%H origin/main -- $watched).Trim()
    $seen = if (Test-Path $last) { (Get-Content -Raw $last).Trim() } else { '' }
    if (-not $Force -and $trigger -eq $seen) { exit 0 }   # quiet: this is most runs

    $open = gh pr list --repo $repo --state open --json headRefName --jq '.[].headRefName' |
        Where-Object { $_ -like "test/$Lane-*" }
    if ($LASTEXITCODE -ne 0) { Say 'gh pr list failed (is gh logged in?). Trying again next time.'; exit 0 }
    if ($open) {
        Say "Waiting: $($open -join ', ') is still open."
        exit 0
    }

    if (git -C $Work status --porcelain) {
        Say "$Work has uncommitted changes, left by a run that was cut off. Look at them, then clean it (git -C $Work stash -u, or git restore/clean) and run again."
        exit 1
    }

    git -C $Work checkout -q --detach origin/main
    $head = (git -C $Work rev-parse --short HEAD).Trim()
    Say "[$Lane] Starting a run on $head (trigger $($trigger.Substring(0, 7)))."
    # Which copy of this script is running, and how old it is: the ARM64
    # laptop ran one pinned at v0.51.1 for days, found only from a side effect
    # (#201).
    $self = (git -C $PSScriptRoot log -1 --format='%h %s' -- auto-wintest.ps1 2>$null) -join ''
    Say "Script: $PSCommandPath ($self)"
    $inWork = [IO.Path]::GetFullPath($PSScriptRoot).StartsWith([IO.Path]::GetFullPath($Work), [StringComparison]::OrdinalIgnoreCase)
    if (-not $inWork) {
        Say "This script is not the worktree's copy, so it does not follow origin/main. Point the task at $Work\scripts\auto-wintest.ps1 (see the top of the script)."
    }

    # This run's own scratch folder, and the oldest ones beyond three gone.
    Get-ChildItem -Directory -Path $Scratch -Filter 'run-*' -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending | Select-Object -Skip 2 |
        ForEach-Object { Remove-Item -Recurse -Force -LiteralPath $_.FullName -ErrorAction SilentlyContinue }
    $Scratch = Join-Path $Scratch ('run-{0:yyyyMMdd-HHmmss}' -f (Get-Date))
    New-Item -ItemType Directory -Force -Path $Scratch | Out-Null

    $prompt = "無人実行です。人は見ていません。.claude/windows-role.md を読み、その「Unattended runs」の節に従って、$queue の次の節を 1 つだけ進めてください。レーンは $Lane で、ブランチは test/$Lane-<節> です。チェックアウトは $Work です（役割定義に出てくる C:\dev\filer は、すべてここに読み替えてください）。作業用の一時ディレクトリは $Scratch で、TEMP / TMP も既にそこを指しています（役割定義に出てくる R:\Temp は、すべてここに読み替えてください）。"
    $env:TEMP = $Scratch
    $env:TMP = $Scratch
    $env:CARGO_INCREMENTAL = '0'
    Say "Build output: $(Set-BuildTarget)"

    if (-not $KeepScreenSaver) {
        Suspend-ScreenSaver
        $prompt += " スクリーンセーバーはこのスクリプトが実行の間だけ止めています（起動していれば 5 秒以内に止めます）。"
    }
    # Not a reason to stop: PostMessage and --keys still reach the window. The
    # run is told, so it does not record SendInput rows as having done nothing.
    Start-Sleep -Seconds 1
    $desk = Get-InputDesktop
    Say "Input desktop at the start: $desk"
    if ($desk -ne 'Default') {
        $prompt += " 起動時の入力デスクトップは `"$desk`" で、Default ではありません。SendInput のキーとマウスは届かないので、そういう行は測らずに理由を書いて残し、--keys と PostMessage で進められる行だけを進めてください。"
    }
    # claude writes UTF-8, and PowerShell decodes a native command's output
    # with the console's code page -- CP932 on a Japanese Windows -- so every
    # Japanese line of the run's answer reached the log as mojibake
    # (2026-10-03). Decoded as UTF-8 here, it is written to the log as it was.
    $utf8 = [Text.UTF8Encoding]::new($false)
    [Console]::OutputEncoding = $utf8
    $OutputEncoding = $utf8
    Push-Location $Work
    try {
        $out = claude -p $prompt --permission-mode acceptEdits --allowedTools $Tools --disallowedTools $Denied 2>&1 | Out-String
        $code = $LASTEXITCODE
    } finally {
        Pop-Location
        if (-not $KeepScreenSaver) { Resume-ScreenSaver }
    }
    Add-Content -Path $log -Value "===== exit=$code`n$out"

    $tail = ($out.TrimEnd() -split "`r?`n")[-1].Trim()
    if ($code -eq 0) {
        # Only a finished run uses the trigger up. A failed one is tried
        # again on the next firing, from the same commit.
        Set-Content -NoNewline -Path $last -Value $trigger
        Say "Done: $tail"
    } elseif ($out -match 'limit') {
        Say 'Hit a usage limit. Trying again next time.'
    } else {
        Say "The run failed (exit $code). See the log. Last line: $tail"
        exit 1
    }
} finally {
    $mutex.ReleaseMutex()
}
