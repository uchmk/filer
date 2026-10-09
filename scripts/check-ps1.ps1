# Parse every PowerShell script under scripts/ and fail on the first syntax
# error. Nothing is run.
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
