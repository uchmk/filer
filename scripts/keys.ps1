<#
Run kura once with `--keys` and print what it wrote -- the Windows lanes' way of
pressing keys and reading the result (.claude/windows-role.md). Linux has
scripts/xrun.sh for the same job.

  scripts\keys.ps1 -Dir C:\fixtures -Keys '<Tab>C<State:spot><Quit>' [-Out R:\run1] [-Launches]

Gives the run its own folder (KURA_KEYS_DONE, `<State:name>` and `<Shot:name>`
files all land there), starts kura, waits for it, then prints the done file and
every `*.txt` beside it, one block per file. Returns the folder's path.

  -Launches   the keys start a program (sections 22, 26, 32, 37, `<F12>`). `-Wait`
              waits for descendants too and would never return while the editor or
              browser is open (#234), so this waits on kura's own id instead.
  -Timeout    seconds to wait for kura (default 120); a run that outlives it is
              stopped and reported as timed out, not as a result.
  -Kura      the binary (default target\release\kura.exe beside this script's repo).

End the keys with `<Quit>` where `q` means something else (#236). A done file
whose last line is not `keys: done` (`keys: stalled`, `keys: refused`) is not a
result: say so in the report with its lines.

Exit code: 0 for a result; 1 when the done file is missing or does not end with
`keys: done`; 2 when kura outlived -Timeout. The output is printed either way.
#>
param(
    [Parameter(Mandatory)][string]$Dir,
    [Parameter(Mandatory)][string]$Keys,
    [string]$Out,
    [string]$Kura,
    [int]$Timeout = 120,
    [switch]$Launches
)
$ErrorActionPreference = 'Stop'
if (-not $Kura) { $Kura = Join-Path (Split-Path $PSScriptRoot -Parent) 'target\release\kura.exe' }
if (-not (Test-Path -LiteralPath $Kura)) { throw "keys.ps1: $Kura does not exist (cargo build --release, or pass -Kura)" }
if (-not $Out) { $Out = Join-Path ([IO.Path]::GetTempPath()) ('kura-keys-' + [guid]::NewGuid().ToString('N').Substring(0, 8)) }
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$done = Join-Path $Out 'keys.done'
Remove-Item -LiteralPath $done -ErrorAction SilentlyContinue
$env:KURA_KEYS_DONE = $done
$code = 0

# One argument list, quoted by us: a `<Wait:1500>` or a space-bearing path survives.
$argList = @('"' + $(if ($Dir.Length -gt 3) { $Dir.TrimEnd('\') } else { $Dir }) + '"', '--keys', ('"' + $Keys.Replace('"', '\"') + '"'))
$p = Start-Process -FilePath $Kura -ArgumentList $argList -PassThru
if (-not $p.WaitForExit($Timeout * 1000)) {
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    $code = 2
    Write-Warning "keys.ps1: kura was still running after $Timeout s and was stopped; what follows is NOT a result"
}
# -Launches: the run may have started programs; kura's own exit is all we wait for
# (Start-Process -Wait would wait for them too).
if (-not $Launches) { Start-Sleep -Milliseconds 300 }

if (-not (Test-Path -LiteralPath $done)) {
    if ($code -eq 0) { $code = 1 }
    Write-Warning 'keys.ps1: no done file -- kura vanished before writing one; not a result'
} else {
    $last = (Get-Content -LiteralPath $done | Select-Object -Last 1)
    if ($last -ne 'keys: done') { if ($code -eq 0) { $code = 1 }; Write-Warning "keys.ps1: the done file ends with '$last', not 'keys: done'; not a result" }
}
foreach ($f in @($done) + @(Get-ChildItem -LiteralPath $Out -Filter *.txt | Sort-Object Name | ForEach-Object FullName)) {
    if (Test-Path -LiteralPath $f) {
        "--- $(Split-Path $f -Leaf)"
        Get-Content -LiteralPath $f
    }
}
$Out
exit $code
