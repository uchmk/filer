<#
Check a release's two Windows zips as a person downloads them: TESTING.md
section 48, rows 48.1-48.5, in one command (#192 proposal 3).

  scripts\verify-release.ps1 -Tag v0.80.2 [-Out C:\dev\rel] [-Repo uchmk/kura]

Downloads kura-<tag>-windows-x64.zip and -arm64.zip from the release page into
-Out (a new folder under TEMP by default), extracts each into an empty folder,
and checks:

  48.1  each zip holds one folder, kura-<tag>-windows-<arch>, with exactly the
        five files kura.exe, kura.com, conpty.dll, OpenConsole.exe and
        ConPTY-LICENSE.txt
  48.2  kura.exe and kura.com --version say `kura <version> (x86_64)` / `(aarch64)`; a zip
        this machine cannot run (the ARM64 one on an x64 machine) is skipped,
        and said to be
  48.3  the PE machine of the four binaries: 8664 in the x64 zip, AA64 in the
        ARM64 one
  48.4  ConPTY-LICENSE.txt names the version scripts/fetch-conpty.ps1 pins on
        the tag's commit, and no `{VERSION}` is left in it
  48.5  every zip and every extracted file against the SHA-256 table at the end
        of the release page (hash, and bytes for the files); a page without the
        table says so -- the `sums` job did not run

48.6 and 48.7 start kura and read its modules: scripts\run48.ps1 does 48.6.

Prints one `ok` / `FAIL` / `skip` line per check and a last line
`verify-release: N ok, N failed, N skipped`. Returns nothing.

Exit code: 0 when nothing failed; 1 when a check failed; 2 when the release
could not be read at all (no such tag, a download failed).

Written on Linux and run there with PowerShell 7 against v0.72.2 (all ok, 48.2
skipped) and v0.79.0 (48.5 fails: no table): not yet run on Windows, so 48.2
has never run. The first lane run that uses it checks it against the rows by
hand once.
#>
param(
    [Parameter(Mandatory)][string]$Tag,
    [string]$Out,
    [string]$Repo = 'uchmk/kura'
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
if ($Tag -notmatch '^v\d+\.\d+\.\d+$') { throw "verify-release.ps1: -Tag is vX.Y.Z, not '$Tag'" }
$version = $Tag.Substring(1)
if (-not $Out) { $Out = Join-Path ([IO.Path]::GetTempPath()) ("kura-release-$Tag-" + [guid]::NewGuid().ToString('N').Substring(0, 6)) }
New-Item -ItemType Directory -Force -Path $Out | Out-Null

$script:ok = 0; $script:failed = 0; $script:skipped = 0
function Say([string]$what, [string]$row, [string]$text) {
    switch ($what) {
        'ok' { $script:ok++ }
        'FAIL' { $script:failed++ }
        'skip' { $script:skipped++ }
    }
    Write-Output ('{0,-4} {1}  {2}' -f $what, $row, $text)
}

function Machine([string]$f) {
    $b = [IO.File]::ReadAllBytes($f)
    '{0:X4}' -f [BitConverter]::ToUInt16($b, [BitConverter]::ToInt32($b, 0x3C) + 4)
}

# The release page and the pin, before anything is downloaded: a wrong tag
# stops here.
try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/tags/$Tag" -Headers @{ 'User-Agent' = 'kura-verify-release' }
} catch {
    Write-Output "verify-release: cannot read the release $Tag of $Repo -- $($_.Exception.Message)"
    exit 2
}
$body = [string]$release.body

# The table release-sums.sh writes: `| `file` | `hash` |` for the assets, then
# `#### Inside `zip`` and `| `path` | bytes | `hash` |` per zip.
$assetSums = @{}
$inside = @{}
$zipNow = $null
$hasTable = $body -match '(?m)^### SHA-256\s*$'
foreach ($line in ($body -split "`r?`n")) {
    if ($line -match '^#### Inside `([^`]+)`') { $zipNow = $Matches[1]; $inside[$zipNow] = @{}; continue }
    if ($line -match '^\| `([^`]+)` \| (\d+) \| `([0-9a-f]{64})` \|$' -and $zipNow) {
        $inside[$zipNow][$Matches[1]] = @{ Bytes = [int64]$Matches[2]; Hash = $Matches[3] }
        continue
    }
    if ($line -match '^\| `([^`]+)` \| `([0-9a-f]{64})` \|$' -and -not $zipNow) { $assetSums[$Matches[1]] = $Matches[2] }
}

$pin = $null
try {
    $fetch = Invoke-RestMethod -Uri "https://raw.githubusercontent.com/$Repo/$Tag/scripts/fetch-conpty.ps1" -Headers @{ 'User-Agent' = 'kura-verify-release' }
    if ($fetch -match "(?m)^\`$version = '([^']+)'") { $pin = $Matches[1] }
} catch { }

$onWindows = [Environment]::OSVersion.Platform -eq 'Win32NT'
$canRun = @{
    'x64' = $onWindows
    'arm64' = $onWindows -and ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64' -or $env:PROCESSOR_ARCHITEW6432 -eq 'ARM64')
}
$five = @('ConPTY-LICENSE.txt', 'OpenConsole.exe', 'conpty.dll', 'kura.com', 'kura.exe')

foreach ($pair in @(@('x64', '8664', 'x86_64'), @('arm64', 'AA64', 'aarch64'))) {
    $arch, $pe, $triple = $pair
    $name = "kura-$Tag-windows-$arch"
    $zip = Join-Path $Out "$name.zip"
    $url = "https://github.com/$Repo/releases/download/$Tag/$name.zip"
    try {
        Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
    } catch {
        Write-Output "verify-release: cannot download $url -- $($_.Exception.Message)"
        exit 2
    }
    $dest = Join-Path $Out $arch
    if (Test-Path -LiteralPath $dest) { Remove-Item -LiteralPath $dest -Recurse -Force }
    Expand-Archive -LiteralPath $zip -DestinationPath $dest

    # 48.1: one folder at the top, five files in it, nothing else.
    $top = @(Get-ChildItem -LiteralPath $dest -Force)
    $folder = Join-Path $dest $name
    if ($top.Count -eq 1 -and $top[0].PSIsContainer -and $top[0].Name -eq $name) {
        $files = @(Get-ChildItem -LiteralPath $folder -Recurse -Force -File | ForEach-Object { $_.FullName.Substring($folder.Length + 1) } | Sort-Object)
        $extra = @(Get-ChildItem -LiteralPath $folder -Recurse -Force -Directory)
        if ((Compare-Object $files $five -CaseSensitive) -or $extra.Count) {
            Say 'FAIL' '48.1' "$name holds $($files -join ', ')$(if ($extra.Count) { ' and folders' })"
        } else {
            Say 'ok' '48.1' "$name holds the five files"
        }
    } else {
        Say 'FAIL' '48.1' "$name.zip's top level is $(($top | ForEach-Object Name) -join ', '), not the one folder $name"
        continue
    }

    # 48.2: the row's kura.exe, piped so that it is waited for, and the
    # console front kura.com that a plain `kura --version` finds first.
    if ($canRun[$arch]) {
        $want = "kura $version ($triple)"
        foreach ($bin in @('kura.exe', 'kura.com')) {
            $said = (& (Join-Path $folder $bin) --version 2>&1 | Out-String).Trim()
            if ($said -eq $want) { Say 'ok' '48.2' "$arch $bin says $said" } else { Say 'FAIL' '48.2' "$arch $bin says '$said', not '$want'" }
        }
    } else {
        Say 'skip' '48.2' "${arch}: this machine cannot run it (the ARM64 lane presses it)"
    }

    # 48.3: all four binaries of one machine.
    foreach ($bin in @('kura.exe', 'kura.com', 'conpty.dll', 'OpenConsole.exe')) {
        $m = Machine (Join-Path $folder $bin)
        if ($m -eq $pe) { Say 'ok' '48.3' "$arch $bin is $m" } else { Say 'FAIL' '48.3' "$arch $bin is $m, not $pe" }
    }

    # 48.4: the licence names the pinned ConPTY.
    $licence = Get-Content -LiteralPath (Join-Path $folder 'ConPTY-LICENSE.txt') -Raw
    if ($licence -match '\{VERSION\}') {
        Say 'FAIL' '48.4' "$arch ConPTY-LICENSE.txt still has {VERSION} in it"
    } elseif (-not $pin) {
        Say 'skip' '48.4' "${arch}: could not read the pin from fetch-conpty.ps1 at $Tag"
    } elseif ($licence.Contains($pin)) {
        Say 'ok' '48.4' "$arch ConPTY-LICENSE.txt names $pin"
    } else {
        Say 'FAIL' '48.4' "$arch ConPTY-LICENSE.txt does not name $pin, the version fetch-conpty.ps1 pins at $Tag"
    }

    # 48.5: the zip and each file against the release page's table.
    if (-not $hasTable) {
        Say 'FAIL' '48.5' "the release page has no SHA-256 table: the sums job did not run"
        continue
    }
    $zipHash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
    if (-not $assetSums.ContainsKey("$name.zip")) {
        Say 'FAIL' '48.5' "$name.zip has no row in the table"
    } elseif ($assetSums["$name.zip"] -eq $zipHash) {
        Say 'ok' '48.5' "$name.zip matches the table"
    } else {
        Say 'FAIL' '48.5' "$name.zip is $zipHash, the table says $($assetSums["$name.zip"])"
    }
    $rows = $inside["$name.zip"]
    if (-not $rows) {
        Say 'FAIL' '48.5' "the table has no 'Inside $name.zip' section"
        continue
    }
    foreach ($f in $five) {
        $key = "$name/$f"
        $path = Join-Path $folder $f
        $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        $bytes = (Get-Item -LiteralPath $path).Length
        if (-not $rows.ContainsKey($key)) {
            Say 'FAIL' '48.5' "$key has no row in the table"
        } elseif ($rows[$key].Hash -ne $hash -or $rows[$key].Bytes -ne $bytes) {
            Say 'FAIL' '48.5' "$key is $bytes bytes $hash, the table says $($rows[$key].Bytes) bytes $($rows[$key].Hash)"
        } else {
            Say 'ok' '48.5' "$key matches the table"
        }
    }
}

Write-Output "verify-release: $script:ok ok, $script:failed failed, $script:skipped skipped ($Out)"
if ($script:failed) { exit 1 }
exit 0
