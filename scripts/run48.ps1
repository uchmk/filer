<#
TESTING.md 48.6: start a release zip's filer.exe with a decoy conpty.dll first
on the PATH, open the pane, and say which conpty.dll the process loaded -- it
must be the one beside filer.exe (#192 kept this script in its evidence folder;
#198 / #199 asked for it here).

  scripts\run48.ps1 -Folder C:\dev\rel\x64\filer-v0.80.2-windows-x64 -Decoy 'C:\Program Files\WezTerm\conpty.dll'

-Folder is an extracted zip's folder (scripts\verify-release.ps1 leaves both
under its -Out). -Decoy must be of the same PE machine as that filer.exe: an
ARM64 process cannot load WezTerm's 8664 copy at all, so the row would prove
nothing (#199); the script refuses a mismatched pair. On the ARM64 machine
Zed's conpty.dll is AA64.

Starts filer with `--keys '<C-t><Wait:3000><State:run48>'` and FILER_KEYS_DONE
in a folder of its own, waits for the done file, reads the modules, then stops
that filer (and only it). Prints the conpty.dll paths it found and an
`ok` / `FAIL` line.

  -Timeout    seconds to wait for the done file (default 60).

Exit code: 0 when the only conpty.dll loaded is the folder's own; 1 when it is
another copy or none; 2 when the run gave no result (no done file, or it does
not end with `keys: done`).

Written on Linux, where it cannot run: not yet run on Windows. The first lane
run that uses it checks it against the row by hand once.
#>
param(
    [Parameter(Mandatory)][string]$Folder,
    [Parameter(Mandatory)][string]$Decoy,
    [int]$Timeout = 60
)
$ErrorActionPreference = 'Stop'

function Machine([string]$f) {
    $b = [IO.File]::ReadAllBytes($f)
    '{0:X4}' -f [BitConverter]::ToUInt16($b, [BitConverter]::ToInt32($b, 0x3C) + 4)
}

$Folder = (Resolve-Path -LiteralPath $Folder).Path
$Decoy = (Resolve-Path -LiteralPath $Decoy).Path
$exe = Join-Path $Folder 'filer.exe'
$own = Join-Path $Folder 'conpty.dll'
foreach ($f in @($exe, $own)) {
    if (-not (Test-Path -LiteralPath $f)) { throw "run48.ps1: $f does not exist (is -Folder an extracted zip's folder?)" }
}
if ((Split-Path $Decoy -Leaf) -ne 'conpty.dll') { throw "run48.ps1: -Decoy is a conpty.dll, not $Decoy" }
$decoyDir = Split-Path $Decoy -Parent
if ($decoyDir -eq $Folder) { throw 'run48.ps1: the decoy is the zip''s own copy' }
$exeMachine = Machine $exe
$decoyMachine = Machine $Decoy
if ($exeMachine -ne $decoyMachine) {
    throw "run48.ps1: filer.exe is $exeMachine and the decoy is $decoyMachine -- take a decoy of the same machine (#199)"
}

$out = Join-Path ([IO.Path]::GetTempPath()) ('filer-run48-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Force -Path $out | Out-Null
$done = Join-Path $out 'keys.done'

# The decoy's folder goes first on the PATH of filer alone; this shell's PATH
# is put back afterwards.
$oldPath = $env:PATH
$oldDone = $env:FILER_KEYS_DONE
$env:PATH = "$decoyDir;$oldPath"
$env:FILER_KEYS_DONE = $done
try {
    $p = Start-Process -FilePath $exe -ArgumentList @('--keys', '"<C-t><Wait:3000><State:run48>"') -WorkingDirectory $Folder -PassThru
} finally {
    $env:PATH = $oldPath
    $env:FILER_KEYS_DONE = $oldDone
}

$deadline = (Get-Date).AddSeconds($Timeout)
while (-not (Test-Path -LiteralPath $done) -and -not $p.HasExited -and (Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 250
}
$lines = @()
if (Test-Path -LiteralPath $done) {
    # The done file is written in one go, but give the write a moment to end.
    Start-Sleep -Milliseconds 250
    $lines = @(Get-Content -LiteralPath $done)
}
$loaded = @()
if (-not $p.HasExited) {
    $p.Refresh()
    $loaded = @($p.Modules | Where-Object ModuleName -eq 'conpty.dll' | ForEach-Object FileName)
    Stop-Process -Id $p.Id -Force
}

Write-Output "filer.exe  $exe ($exeMachine)"
Write-Output "decoy      $Decoy ($decoyMachine), first on the PATH"
if (-not $lines.Count -or $lines[-1] -ne 'keys: done') {
    Write-Output 'done file:'
    $lines | ForEach-Object { Write-Output "  $_" }
    Write-Output "run48: no result -- the keys did not end with 'keys: done' within $Timeout s ($out)"
    exit 2
}
if (-not $loaded.Count) {
    Write-Output 'conpty.dll  none loaded'
    Write-Output 'FAIL 48.6  the pane runs on the ConPTY built into Windows, not the zip''s'
    exit 1
}
$loaded | ForEach-Object { Write-Output "conpty.dll  $_" }
if ($loaded.Count -eq 1 -and $loaded[0] -eq $own) {
    Write-Output "ok   48.6  the zip's own conpty.dll, beside filer.exe"
    exit 0
}
Write-Output "FAIL 48.6  not (only) $own"
exit 1
