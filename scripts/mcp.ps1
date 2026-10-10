<#
Talk to `kura mcp` without Claude Code -- TESTING.md section 50 needs the MCP
server's answers, and #302 / #303 (ARM64 and x64) each wrote a throwaway
Start-Mcp / Rpc / Call helper to get them (ARM64's tripped over `$args`).

  scripts\mcp.ps1 [-Exe .\target\debug\kura.exe] [-Address \\.\pipe\x] [-Tool kura_state] [-Arguments '{}']
  scripts\mcp.ps1 -Tool kura_reveal -Arguments '{"path":"C:\\Windows"}'
  scripts\mcp.ps1 -List

Starts `<Exe> mcp`, sends `initialize`, `notifications/initialized`, then either
`tools/list` (-List) or `tools/call` for -Tool, prints the result as JSON on
stdout, and closes the server's input. -Address sets KURA_ADDRESS for the child
only, so a test window does not fight the owner's window for the door.

Exit code: 0 on a result, 1 when the tool answered isError or the server sent
a JSON-RPC error, 2 when the server gave no answer within -TimeoutSec.

Written on Linux, where it cannot run: not yet run on Windows. The first lane
run that uses it checks it against 50.1 / 50.2 by hand once.
#>
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\debug\kura.exe'),
    [string]$Address = '',
    [string]$Tool = 'kura_state',
    [string]$Arguments = '{}',
    [switch]$List,
    [int]$TimeoutSec = 20
)
$ErrorActionPreference = 'Stop'

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = (Resolve-Path $Exe).Path
$psi.Arguments = 'mcp'
$psi.UseShellExecute = $false
$psi.RedirectStandardInput = $true
$psi.RedirectStandardOutput = $true
$psi.CreateNoWindow = $true
if ($Address) { $psi.EnvironmentVariables['KURA_ADDRESS'] = $Address }
$proc = [System.Diagnostics.Process]::Start($psi)

function Send($obj) {
    $proc.StandardInput.WriteLine(($obj | ConvertTo-Json -Depth 10 -Compress))
    $proc.StandardInput.Flush()
}
# One request, then read lines until the reply with the same id (the server may
# send notifications in between).
function Rpc([int]$id, [string]$method, $params) {
    Send @{ jsonrpc = '2.0'; id = $id; method = $method; params = $params }
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSec)
    while ($true) {
        $left = ($deadline - [DateTime]::UtcNow).TotalMilliseconds
        if ($left -le 0) { return $null }
        $task = $proc.StandardOutput.ReadLineAsync()
        if (-not $task.Wait([int]$left)) { return $null }
        if ($null -eq $task.Result) { return $null }
        $msg = $task.Result | ConvertFrom-Json
        if ($msg.id -eq $id) { return $msg }
    }
}

$code = 0
try {
    $init = Rpc 1 'initialize' @{
        protocolVersion = '2024-11-05'
        capabilities    = @{}
        clientInfo      = @{ name = 'kura-mcp.ps1'; version = '1' }
    }
    if ($null -eq $init) { Write-Error 'no answer to initialize'; $code = 2 }
    else {
        Send @{ jsonrpc = '2.0'; method = 'notifications/initialized' }
        if ($List) { $r = Rpc 2 'tools/list' @{} }
        else {
            $argObj = $Arguments | ConvertFrom-Json
            if ($null -eq $argObj) { $argObj = @{} }
            $r = Rpc 2 'tools/call' @{ name = $Tool; arguments = $argObj }
        }
        if ($null -eq $r) { Write-Error 'no answer'; $code = 2 }
        else {
            $r | ConvertTo-Json -Depth 20
            if ($r.error -or $r.result.isError) { $code = 1 }
        }
    }
}
finally {
    try { $proc.StandardInput.Close() } catch { }
    if (-not $proc.WaitForExit(3000)) { $proc.Kill() }
}
exit $code
