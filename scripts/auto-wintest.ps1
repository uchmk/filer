# Start the Windows test session by itself when there is new work for it.
#
# The session on the Windows machine (.claude/windows-role.md) is the only one
# that can run filer, and until now a person had to start it by hand for every
# section. This looks once, and starts one unattended run if:
#
#   - origin/main has changed TESTING.md, TESTING-CHECKS.md or
#     .claude/windows-role.md since the last run it started -- a merged run
#     changes the last two, so merging one pull request is what starts the next;
#   - no test/win-* pull request is still open (that section is not done yet,
#     and a second run would take the same one: it happened on 2026-09-28);
#   - the screen is not locked (SendInput does nothing on a locked desktop, and
#     the run would record every key as having done nothing);
#   - the working copy is clean (a dirty one is a run that was cut off, and
#     only a person can say what to do with it).
#
# It is meant to be run by Task Scheduler every 15 minutes, as you, "only when
# the user is logged on" -- the run drives a real window:
#
#   $a = New-ScheduledTaskAction -Execute pwsh -Argument '-NoProfile -WindowStyle Hidden -File C:\dev\filer\scripts\auto-wintest.ps1'
#   $t = New-ScheduledTaskTrigger -Once -At (Get-Date) -RepetitionInterval (New-TimeSpan -Minutes 15)
#   $s = New-ScheduledTaskSettingsSet -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Hours 4)
#   Register-ScheduledTask -TaskName filer-auto-wintest -Action $a -Trigger $t -Settings $s
#
#   Unregister-ScheduledTask -TaskName filer-auto-wintest    # to stop it
#
# By hand:
#
#   pwsh -File scripts\auto-wintest.ps1          # look once, run if there is work
#   pwsh -File scripts\auto-wintest.ps1 -Force   # run even if nothing changed
#   ... -LogDir R:\Temp                           # the log on the RAM disk
#
# Log: %LOCALAPPDATA%\filer-wintest\auto-wintest.log, or in -LogDir. The log
# may go on the RAM disk (-LogDir R:\Temp): it is for reading what a run did,
# not evidence. The state file stays in %LOCALAPPDATA% whatever -LogDir says --
# on R: it would be gone after a reboot, and the next firing would start a run
# for a trigger that was already used.
#
# The run works in its own worktree ($Work), not in the checkout you use, so
# it never meets your uncommitted changes and you can keep working while it
# runs. Needs `claude` and an authenticated `gh` on PATH.
#
# What the run may do is below in $Tools. It is broad on purpose -- a test run
# writes and runs its own PowerShell to drive the window -- so the guard is the
# role definition, plus the few commands it must never run, in $Denied.

param(
    [string]$Work = 'C:\dev\filer-wintest',
    [string]$LogDir,
    [switch]$Force
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

$state = Join-Path $env:LOCALAPPDATA 'filer-wintest'
New-Item -ItemType Directory -Force -Path $state | Out-Null
if (-not $LogDir) { $LogDir = $state }
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
$log = Join-Path $LogDir 'auto-wintest.log'
$last = Join-Path $state 'last-trigger'

function Say([string]$line) {
    $stamped = '[{0:yyyy-MM-dd HH:mm:ss}] {1}' -f (Get-Date), $line
    $stamped
    Add-Content -Path $log -Value $stamped
}

# Task Scheduler's IgnoreNew already keeps its own firings apart; this also
# covers one started by hand while a scheduled one is running.
$mutex = [Threading.Mutex]::new($false, 'Local\filer-auto-wintest')
if (-not $mutex.WaitOne(0)) { Say 'A run is already going. Nothing to do.'; exit 0 }

try {
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

    $trigger = (git -C $Work log -1 --format=%H origin/main -- $watched).Trim()
    $seen = if (Test-Path $last) { (Get-Content -Raw $last).Trim() } else { '' }
    if (-not $Force -and $trigger -eq $seen) { exit 0 }   # quiet: this is most runs

    $open = gh pr list --repo $repo --state open --json headRefName --jq '.[].headRefName' |
        Where-Object { $_ -like 'test/win-*' }
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
    Say "Starting a run on $head (trigger $($trigger.Substring(0, 7)))."

    $prompt = "無人実行です。人は見ていません。.claude/windows-role.md を読み、その「Unattended runs」の節に従って、順番表の次の節を 1 つだけ進めてください。チェックアウトは $Work です（役割定義に出てくる C:\dev\filer は、すべてここに読み替えてください）。"

    Push-Location $Work
    try {
        $out = claude -p $prompt --permission-mode acceptEdits --allowedTools $Tools --disallowedTools $Denied 2>&1 | Out-String
        $code = $LASTEXITCODE
    } finally {
        Pop-Location
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
