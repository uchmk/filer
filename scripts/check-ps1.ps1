# Parse every PowerShell script under scripts/ and fail on the first syntax
# error, then check lane-marks.ps1's functions on a small sample. Nothing
# else is run.
#
# The Windows lanes' scheduled task runs scripts/auto-wintest.ps1 from a
# worktree that the script itself moves to origin/main. A copy that does not
# parse never gets as far as moving it, so one syntax error on `main` stopped
# both lanes for three days, long after the fix had landed (v0.78.167 to
# v0.80.13). CI runs this on every change to scripts/, and scripts/verify.sh
# does when pwsh is installed.
#
#   pwsh -NoProfile -File scripts/check-ps1.ps1

$bad = 0
foreach ($f in Get-ChildItem -Path (Join-Path $PSScriptRoot '*.ps1') | Sort-Object Name) {
    $errors = $null
    [void][System.Management.Automation.Language.Parser]::ParseFile($f.FullName, [ref]$null, [ref]$errors)
    foreach ($e in $errors) {
        Write-Output ("{0}:{1}: {2}" -f $f.Name, $e.Extent.StartLineNumber, $e.Message)
        $bad++
    }
}
if ($bad) { Write-Output "$bad PowerShell syntax error(s)."; exit 1 }
Write-Output 'PowerShell scripts parse.'

# auto-wintest.ps1's marks made again on a main that moved (lane-marks.ps1):
# 2.1 ticked (main left it alone), 2.2 ticked (main reworded it: dropped),
# 2.3 `[~]` (main ticked it too: kept, not dropped), a key ticked; main added
# 2.5 and has CRLF.
. (Join-Path $PSScriptRoot 'lane-marks.ps1')
function Expect([string]$What, $Got, $Want) {
    if ("$Got" -ne "$Want") { Write-Output "lane-marks: $What is '$Got', not '$Want'"; $script:bad++ }
}
$base = "## 2. Panes`n`n- [ ] **2.1** a split`n- [ ] **2.2** another`n- [ ] **2.3** a third`n- [x] **2.4** done`n- [ ] ``F2`` rename"
$ran = "## 2. Panes`n`n- [x] **2.1** a split`n- [x] **2.2** another`n- [~] **2.3** a third`n- [x] **2.4** done`n- [x] ``F2`` rename"
$marks = @(Get-LaneMarks -Base $base -Branch $ran)
Expect 'the marks a run set' (($marks | ForEach-Object { "$($_.Mark) $($_.Rest)" }) -join '|') 'x **2.1** a split|x **2.2** another|~ **2.3** a third|x `F2` rename'
$main = "## 2. Panes`r`n`r`n- [ ] **2.1** a split`r`n- [ ] **2.2** another, reworded`r`n- [~] **2.3** a third`r`n- [x] **2.4** done`r`n- [ ] **2.5** new`r`n- [ ] ``F2`` rename`r`n"
$r = Set-LaneMarks -Main $main -Marks $marks
Expect 'the marks on main' $r.Text "## 2. Panes`r`n`r`n- [x] **2.1** a split`r`n- [ ] **2.2** another, reworded`r`n- [~] **2.3** a third`r`n- [x] **2.4** done`r`n- [ ] **2.5** new`r`n- [x] ``F2`` rename`r`n"
Expect 'the marks dropped' ($r.Dropped -join ',') '2.2'
$r = Set-LaneMarks -Main "- [ ] ``F3`` other`n" -Marks @(Get-LaneMarks -Base '- [ ] `F2` rename' -Branch '- [x] `F2` rename')
Expect 'a key dropped' "$($r.Dropped)|$($r.Text)" "``F2`` rename|- [ ] ``F3`` other`n"
Expect 'no marks' (Set-LaneMarks -Main "a`nb" -Marks @()).Text "a`nb"
if ($bad) { exit 1 }
Write-Output 'Lane marks are made again as they should be.'
