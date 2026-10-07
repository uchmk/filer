<#
Count how many `--keys` runs vanish -- filer exits without writing `keys: done`
(#259, #260, #291). The Windows lanes used to copy a `loop.ps1` / `mk3.ps1` pair
from one machine's evidence folder; this is that, in the repo.

  scripts\count-vanishing.ps1 -Runs 20 -Out C:\dev\filer-evidence\flake [-Filer path\filer.exe] [-Timeout 120]

Builds a tree in `<Out>\tree` (`a.txt`, `b.zip`, `c.pdf` of three pages), a
`filer.toml` with the `pdftoppm` `[[preview]]` rule (in `<Out>\config`, passed
as FILER_CONFIG_HOME so the machine's own config is not read),
then runs filer `-Runs` times with a heavy key list: the PDF's three pages, the
archive, the text, each with a `<State:>` and a `<Shot:>`. Each run has its own
folder and its own FILER_KEYS_DONE.

Prints the report's table: how many runs ended `keys: done`, the exit codes,
how many reached `picture: 3`, the pngs, `.panic` files, seconds per run, and
the `filer.exe` processes left behind. Needs `pdftoppm` on PATH and a
`System.IO.Compression` (any PowerShell 5.1+).
#>
param(
    [int]$Runs = 20,
    [Parameter(Mandatory)][string]$Out,
    [string]$Filer,
    [int]$Timeout = 120
)
$ErrorActionPreference = 'Stop'
if (-not $Filer) { $Filer = Join-Path (Split-Path $PSScriptRoot -Parent) 'target\release\filer.exe' }
if (-not (Test-Path -LiteralPath $Filer)) { throw "count-vanishing.ps1: $Filer does not exist (cargo build --release, or pass -Filer)" }
if (-not (Get-Command pdftoppm -ErrorAction SilentlyContinue)) { throw 'count-vanishing.ps1: pdftoppm is not on PATH' }
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Out = (Resolve-Path -LiteralPath $Out).Path

# The tree: a text file, a zip, and a PDF of three pages (hand-written, no tool needed).
$tree = Join-Path $Out 'tree'
New-Item -ItemType Directory -Force -Path $tree | Out-Null
[IO.File]::WriteAllText((Join-Path $tree 'a.txt'), "alpha`nbeta`ngamma`n")
$zip = Join-Path $tree 'b.zip'
if (-not (Test-Path -LiteralPath $zip)) {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $z = [IO.Compression.ZipFile]::Open($zip, 'Create')
    $e = $z.CreateEntry('inside.txt')
    $w = New-Object IO.StreamWriter($e.Open()); $w.Write("inside the zip`n"); $w.Dispose()
    $z.Dispose()
}
$objs = New-Object System.Collections.Generic.List[string]
$objs.Add('<< /Type /Catalog /Pages 2 0 R >>')
$objs.Add('<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >>')
foreach ($i in 1..3) { $objs.Add('<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>') }
$sb = New-Object Text.StringBuilder
[void]$sb.Append("%PDF-1.4`n")
$offs = @()
for ($i = 0; $i -lt $objs.Count; $i++) {
    $offs += $sb.Length
    [void]$sb.Append("$($i + 1) 0 obj`n$($objs[$i])`nendobj`n")
}
$xref = $sb.Length
[void]$sb.Append("xref`n0 $($objs.Count + 1)`n0000000000 65535 f `n")
foreach ($o in $offs) { [void]$sb.Append(('{0:D10} 00000 n `n' -f $o)) }
[void]$sb.Append("trailer`n<< /Size $($objs.Count + 1) /Root 1 0 R >>`nstartxref`n$xref`n%%EOF`n")
[IO.File]::WriteAllText((Join-Path $tree 'c.pdf'), $sb.ToString(), [Text.Encoding]::ASCII)

# The config: the README's pdftoppm rule, in a folder of its own.
$cfg = Join-Path $Out 'config'
New-Item -ItemType Directory -Force -Path $cfg | Out-Null
[IO.File]::WriteAllText((Join-Path $cfg 'filer.toml'), @'
[[preview]]
match = "*.pdf"
run = 'pdftoppm -png -singlefile -r 120 -f {n} -l {n} {path} {out}'
first = 1
unit = "page {n}"
'@ + "`n")
$env:FILER_CONFIG_HOME = $cfg

# Cursor order is a.txt, b.zip, c.pdf. Three pages, then the archive, then the text.
$keys = 'jj<State:p1><Shot:p1><A-j><State:p2><Shot:p2><A-j><State:p3><Shot:p3>k<Enter><State:zip><Esc>k<State:txt><Shot:txt><Quit>'

$rows = @()
for ($n = 1; $n -le $Runs; $n++) {
    $run = Join-Path $Out ('run-{0:D2}' -f $n)
    Remove-Item -LiteralPath $run -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force -Path $run | Out-Null
    $done = Join-Path $run 'keys.done'
    $env:FILER_KEYS_DONE = $done
    $argList = @('"' + $tree + '"', '--keys', ('"' + $keys + '"'))
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process -FilePath $Filer -ArgumentList $argList -PassThru
    $timedOut = -not $p.WaitForExit($Timeout * 1000)
    if ($timedOut) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
    $sw.Stop()
    $last = if (Test-Path -LiteralPath $done) { Get-Content -LiteralPath $done | Select-Object -Last 1 } else { '' }
    $p3 = Join-Path $run 'p3.txt'
    $rows += [pscustomobject]@{
        Run      = $n
        Done     = ($last -eq 'keys: done')
        Exit     = if ($timedOut) { 'timeout' } else { $p.ExitCode }
        Picture3 = ((Test-Path -LiteralPath $p3) -and [bool](Select-String -LiteralPath $p3 -SimpleMatch 'picture: 3' -Quiet))
        Png      = @(Get-ChildItem -LiteralPath $run -Filter *.png -ErrorAction SilentlyContinue).Count
        Panic    = @(Get-ChildItem -LiteralPath $run -Filter *.panic -ErrorAction SilentlyContinue).Count
        Seconds  = [math]::Round($sw.Elapsed.TotalSeconds, 1)
    }
}
$rows | Format-Table -AutoSize | Out-String -Width 200 | Write-Output
$vanished = @($rows | Where-Object { -not $_.Done }).Count
"keys: done   $(@($rows | Where-Object Done).Count) / $Runs"
"vanished     $vanished / $Runs"
"exit codes   $((@($rows | Group-Object Exit | Sort-Object Name | ForEach-Object { "$($_.Name) x$($_.Count)" })) -join ', ')"
"picture: 3   $(@($rows | Where-Object Picture3).Count) / $Runs"
"png          $(($rows | Measure-Object Png -Sum).Sum) (3 per run expected)"
".panic       $(($rows | Measure-Object Panic -Sum).Sum)"
"seconds      min $(($rows | Measure-Object Seconds -Minimum).Minimum), max $(($rows | Measure-Object Seconds -Maximum).Maximum), avg $([math]::Round(($rows | Measure-Object Seconds -Average).Average, 1))"
"filer.exe left running: $(@(Get-Process filer -ErrorAction SilentlyContinue).Count)"
