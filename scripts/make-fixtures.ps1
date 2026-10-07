<#
.SYNOPSIS
    Build the files TESTING.md's checklist needs, under one directory.

.DESCRIPTION
    Everything the manual checks want to point filer at: an archive of each
    supported format, a picture big enough to zoom into, a file long enough for
    the minimap to compress, two files that differ in one line, a git
    repository with every state a sign is drawn for, and the awkward names that
    have broken things before.

    Safe to run again: the directory is emptied first, and it only ever writes
    inside it.

.PARAMETER Path
    Where to build them. Defaults to a `filer-fixtures` folder on the desktop,
    which is somewhere you can find it and somewhere you will not mind losing.

.EXAMPLE
    .\make-fixtures.ps1
    .\make-fixtures.ps1 -Path D:\tmp\fixtures
#>
[CmdletBinding()]
param(
    [string] $Path = (Join-Path ([Environment]::GetFolderPath('Desktop')) 'filer-fixtures')
)

$ErrorActionPreference = 'Stop'

if (Test-Path -LiteralPath $Path) {
    Write-Host "Clearing $Path"
    Remove-Item -LiteralPath $Path -Recurse -Force
}
New-Item -ItemType Directory -Path $Path | Out-Null
$root = (Resolve-Path -LiteralPath $Path).Path
Write-Host "Building fixtures in $root`n"

# What a group actually left on disk, against what it meant to make. The
# script used to print only its intentions, and `awkward names\` was silently
# three files short for many runs (#111).
$script:short = 0
function Test-Count([string] $dir, [int] $want, [string] $label, [string] $why = '') {
    $got = @(Get-ChildItem -LiteralPath $dir -Force).Count
    if ($got -ne $want) {
        $script:short++
        Write-Warning "${label}: $got entries on disk, expected $want$why"
    }
}

function New-Dir([string] $name) {
    $p = Join-Path $root $name
    New-Item -ItemType Directory -Path $p -Force | Out-Null
    return $p
}

# --- text: long enough that the minimap has to compress, and shaped so the
# --- bands are recognisable (a comment header, indented blocks, blank runs).
$long = New-Object System.Text.StringBuilder
foreach ($i in 1..4000) {
    switch ($i % 40) {
        0  { [void]$long.AppendLine('') }
        1  { [void]$long.AppendLine("// ---- section $([math]::Floor($i / 40)) " + ('-' * 50)) }
        2  { [void]$long.AppendLine("fn block_$i() {") }
        39 { [void]$long.AppendLine('}') }
        default {
            $depth = 1 + ($i % 3)
            [void]$long.AppendLine((' ' * (4 * $depth)) + "let value_$i = compute($i);")
        }
    }
}
Set-Content -LiteralPath (Join-Path $root 'long.rs') -Value $long.ToString() -Encoding UTF8
Write-Host '  long.rs              4000 lines, for the minimap and preview scrolling'

# --- two files a line apart, for the compare view (<A-d>).
$base = 1..200 | ForEach-Object { "line $_" }
Set-Content -LiteralPath (Join-Path $root 'compare-left.txt') -Value $base -Encoding UTF8
$right = $base.Clone()
$right[99] = 'line 100 -- CHANGED'
$right = @($right[0..149]) + @('line 150.5 -- INSERTED') + @($right[150..199])
Set-Content -LiteralPath (Join-Path $root 'compare-right.txt') -Value $right -Encoding UTF8
Write-Host '  compare-left/right   200 lines, one changed and one inserted'

# Word-level compare rows (5.11): one word changed, a Japanese word changed, a
# line with nothing in common, and one that is the same. The Japanese is built
# from code points so Windows PowerShell 5.1 reads this file right without a BOM.
$taro = [string][char]0x592A + [char]0x90CE
$hanako = [string][char]0x82B1 + [char]0x5B50
$to = [string][char]0x5B9B + [char]0x5148 + [char]0x306F
$san = [string][char]0x3055 + [char]0x3093 + [char]0x3067 + [char]0x3059   # sandesu, after a space
Set-Content -LiteralPath (Join-Path $root 'words-left.txt') -Value @('the price is firm', "$to $taro $san", 'alpha beta', 'unchanged line') -Encoding UTF8
Set-Content -LiteralPath (Join-Path $root 'words-right.txt') -Value @('the cost is firm', "$to $hanako $san", 'gamma delta', 'unchanged line') -Encoding UTF8
Write-Host '  words-left/right     4 lines: one word, one Japanese word, nothing in common, same'

# A pair that only a byte comparison can tell apart, and an identical pair.
Set-Content -LiteralPath (Join-Path $root 'same-a.txt') -Value 'identical' -Encoding UTF8
Set-Content -LiteralPath (Join-Path $root 'same-b.txt') -Value 'identical' -Encoding UTF8
[System.IO.File]::WriteAllBytes((Join-Path $root 'binary.dat'), (1..512 | ForEach-Object { [byte]($_ % 256) }))
Write-Host '  same-a/b, binary.dat identical pair, and one that is not text'

# An extension no machine associates with an app (16.13). A machine with HKCR\.xyz cannot run that row.
Set-Content -LiteralPath (Join-Path $root 'unknown.xyz') -Value 'no default app for this' -Encoding UTF8
Write-Host '  unknown.xyz          no default app (16.13; needs HKCR\.xyz absent)'

# --- markdown, for the rendered/source toggle (`M`) and the outline.
@'
# Title

Body text that should wrap when the pane is narrow, and stay put when it is wide.

## A heading

- a list item
- another one

```rust
fn fenced() -> u32 { 42 }
```

## Another heading

> a quote

| a | b |
| --- | --- |
| 1 | 2 |
'@ | Set-Content -LiteralPath (Join-Path $root 'notes.md') -Encoding UTF8
Write-Host '  notes.md             rendered/source toggle, outline, no minimap when rendered'

# --- a picture worth zooming into: a grid fine enough that a blurred decode
# --- is obvious at 1:1, and big enough that fit is well under 100%.
Add-Type -AssemblyName System.Drawing
$w, $h = 3200, 2400
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.Clear([System.Drawing.Color]::FromArgb(24, 26, 38))
$thin = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(90, 200, 255)), 1
for ($x = 0; $x -lt $w; $x += 8) { $g.DrawLine($thin, $x, 0, $x, $h) }
for ($y = 0; $y -lt $h; $y += 8) { $g.DrawLine($thin, 0, $y, $w, $y) }
$font = New-Object System.Drawing.Font 'Consolas', 120
$brush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::White)
$g.DrawString("3200 x 2400`n8px grid", $font, $brush, 60, 60)
$g.Dispose()
$bmp.Save((Join-Path $root 'zoom-me.png'), [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Host '  zoom-me.png          3200x2400 with an 8px grid: blur shows at 1:1'

# A small one, to check that fit never magnifies.
$small = New-Object System.Drawing.Bitmap 48, 48
$g = [System.Drawing.Graphics]::FromImage($small)
$g.Clear([System.Drawing.Color]::Goldenrod)
$g.Dispose()
$small.Save((Join-Path $root 'tiny.png'), [System.Drawing.Imaging.ImageFormat]::Png)
$small.Dispose()
Write-Host '  tiny.png             48x48: fit must leave it at 1:1, not blow it up'

# --- something to pack and unpack (`e` / `E`), plus a ready-made archive.
$pack = New-Dir 'to-pack'
1..5 | ForEach-Object {
    Set-Content -LiteralPath (Join-Path $pack "file$_.txt") -Value "contents $_" -Encoding UTF8
}
New-Item -ItemType Directory -Path (Join-Path $pack 'nested') -Force | Out-Null
Set-Content -LiteralPath (Join-Path $pack 'nested\deep.txt') -Value 'deep' -Encoding UTF8
Compress-Archive -Path (Join-Path $pack '*') -DestinationPath (Join-Path $root 'sample.zip') -Force
Write-Host '  to-pack\, sample.zip pack with E, unpack with e, and preview the listing'

# --- names that have broken things before.
$awkward = New-Dir 'awkward names'
foreach ($name in @(
    'a file with spaces.txt',
    'ひらがなとカタカナ.txt',
    "quote'in-name.txt",
    # In parentheses: inside an array literal `,` binds tighter than `+`, so
    # without them this was three files (`very-`, `long-` x 30, `name.txt`) and
    # the 163-character name 24.2 asks for was never made (#111).
    ('very-' + ('long-' * 30) + 'name.txt'),
    'UPPER.TXT',
    'upper.txt'
)) {
    $p = Join-Path $awkward $name
    if (-not (Test-Path -LiteralPath $p)) {
        Set-Content -LiteralPath $p -Value $name -Encoding UTF8
    }
}
Write-Host '  awkward names\       spaces, CJK, a quote, a very long one, case pairs'
Test-Count $awkward 6 'awkward names' ' (UPPER.TXT and upper.txt are one file in a case-insensitive folder; 24.3 needs `fsutil file setCaseSensitiveInfo <dir> enable`)'

# --- files to rename in bulk (`R`), including a pair to swap.
$ren = New-Dir 'bulk-rename'
1..12 | ForEach-Object {
    Set-Content -LiteralPath (Join-Path $ren ("IMG_{0:D4}.jpg" -f $_)) -Value 'x' -Encoding UTF8
}
# `ab` and `ba` so that one rule -- s/^([ab])([ab])/$2$1/ -- makes them trade
# names. That is the only shape that reaches the cycle handling, and it is worth
# reaching: a loop of `mv` fails on the second rename.
Set-Content -LiteralPath (Join-Path $ren 'ab.txt') -Value 'ab' -Encoding UTF8
Set-Content -LiteralPath (Join-Path $ren 'ba.txt') -Value 'ba' -Encoding UTF8
Set-Content -LiteralPath (Join-Path $ren 'in the way.txt') -Value 'blocker' -Encoding UTF8
Write-Host '  bulk-rename\         IMG_0001..0012, ab/ba to swap, a name in the way'
Test-Count $ren 15 'bulk-rename'

# --- a repository with every state the git signs are drawn for.
$repo = New-Dir 'repo'
Push-Location $repo
try {
    if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
        Write-Warning 'git is not on PATH: skipping the repository fixture'
    } else {
        git init --quiet 2>&1 | Out-Null
        git config user.email 'fixtures@example.invalid'
        git config user.name 'Fixtures'
        Set-Content -LiteralPath 'clean.txt' -Value 'committed and untouched' -Encoding UTF8
        Set-Content -LiteralPath 'modified.txt' -Value 'before' -Encoding UTF8
        Set-Content -LiteralPath 'deleted.txt' -Value 'to be removed' -Encoding UTF8
        New-Item -ItemType Directory -Path 'sub' -Force | Out-Null
        Set-Content -LiteralPath 'sub\inside.txt' -Value 'before' -Encoding UTF8
        git add -A 2>&1 | Out-Null
        git commit --quiet -m 'fixtures' 2>&1 | Out-Null

        Set-Content -LiteralPath 'modified.txt' -Value 'after' -Encoding UTF8
        Set-Content -LiteralPath 'sub\inside.txt' -Value 'after' -Encoding UTF8
        Set-Content -LiteralPath 'staged.txt' -Value 'new and staged' -Encoding UTF8
        git add staged.txt 2>&1 | Out-Null
        Remove-Item -LiteralPath 'deleted.txt' -Force
        Set-Content -LiteralPath 'untracked.txt' -Value 'never added' -Encoding UTF8
        New-Item -ItemType Directory -Path 'untracked-dir' -Force | Out-Null
        Set-Content -LiteralPath 'untracked-dir\a.txt' -Value 'a' -Encoding UTF8
        Set-Content -LiteralPath 'untracked-dir\b.txt' -Value 'b' -Encoding UTF8
        Write-Host '  repo\                clean / M / + / D / ? and an untracked directory'
    }
} finally {
    Pop-Location
}

# --- a directory big enough to scroll, for the list itself.
$many = New-Dir 'many'
1..500 | ForEach-Object {
    Set-Content -LiteralPath (Join-Path $many ("item-{0:D3}.txt" -f $_)) -Value "$_" -Encoding UTF8
}
Write-Host '  many\                500 entries, for scrolling and select-all'
Test-Count $many 500 'many'

if ($script:short -gt 0) {
    Write-Warning "$($script:short) group(s) did not come out as intended; see above"
}
Write-Host "`nDone. Point filer at:`n  $root"
Write-Host 'The checklist that uses these is TESTING.md in the repository root.'
