<#
TESTING.md 45.11: build two trees that differ only in where a symlink points,
and optionally compare them in kura (#192 kept this script in its evidence
folder; #198 / #199 asked for it here).

  scripts\fx45.ps1 [-Out C:\dev\fx45] [-Run] [-Kura target\release\kura.exe]

Under -Out (a new folder under TEMP by default) it writes

  a\t1.txt  a\t2.txt  a\ln -> t1.txt
  b\t1.txt  b\t2.txt  b\ln -> t2.txt

t1.txt and t2.txt hold the same bytes, in both trees: a compare that follows
the link reads `ln` as matching, so the row passes only when the links
themselves are compared (the bug v0.54.5 fixed). Returns -Out.

A symbolic link needs Developer Mode or an elevated shell on Windows; without
either New-Item fails and this script stops there, saying so.

  -Run    then press the row through scripts\keys.ps1: select a and b, `<A-d>`,
          and take a screenshot of the view (`fx45.png` in keys.ps1's folder).
          The `ln` row must read as differing (`~`), not `=`.

Written on Linux, where it cannot make Windows links: not yet run on Windows.
The first lane run that uses it checks it against the row by hand once.
#>
param(
    [string]$Out,
    [switch]$Run,
    [string]$Kura
)
$ErrorActionPreference = 'Stop'
if (-not $Out) { $Out = Join-Path ([IO.Path]::GetTempPath()) ('kura-fx45-' + [guid]::NewGuid().ToString('N').Substring(0, 6)) }
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Out = (Resolve-Path -LiteralPath $Out).Path

foreach ($side in @(@('a', 't1.txt'), @('b', 't2.txt'))) {
    $name, $target = $side
    $dir = Join-Path $Out $name
    if (Test-Path -LiteralPath $dir) { Remove-Item -LiteralPath $dir -Recurse -Force }
    New-Item -ItemType Directory -Path $dir | Out-Null
    foreach ($t in @('t1.txt', 't2.txt')) { Set-Content -LiteralPath (Join-Path $dir $t) -Value 'the same bytes on both sides' -NoNewline }
    try {
        # A relative target, so that the two links differ in their text only.
        New-Item -ItemType SymbolicLink -Path (Join-Path $dir 'ln') -Target $target | Out-Null
    } catch {
        throw "fx45.ps1: cannot make a symbolic link in $dir -- turn on Developer Mode or run elevated ($($_.Exception.Message))"
    }
}
Get-ChildItem -LiteralPath (Join-Path $Out 'a'), (Join-Path $Out 'b') -Force |
    Where-Object LinkType | ForEach-Object { Write-Output "$($_.FullName) -> $($_.Target)" }

if ($Run) {
    $keysArgs = @{ Dir = $Out; Keys = '<Space><Space><A-d><Wait:2000><Shot:fx45><Esc><Quit>' }
    if ($Kura) { $keysArgs.Kura = $Kura }
    & (Join-Path $PSScriptRoot 'keys.ps1') @keysArgs
}
$Out
