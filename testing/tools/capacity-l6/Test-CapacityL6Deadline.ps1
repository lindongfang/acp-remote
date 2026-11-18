#requires -Version 7.4
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$RepositoryRoot,
    [Parameter(Mandatory)][string]$InertConfig,
    [Parameter(Mandatory)][string]$EvidenceRoot
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'CapacityL6.psm1') -Force
$validated = Test-CapacityL6Configuration -Config $InertConfig -RepositoryRoot $RepositoryRoot
$area = Join-Path $EvidenceRoot ('runner-deadline-' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $area
$helper = Join-Path $area 'inert-list-helper.ps1'
$receipt = Join-Path $area 'owned-processes.json'
# A controlled enumeration helper, never Node, an adapter, or an L6 invocation.
# Its child has its own 90s upper bound even if the outer test is interrupted.
$text = @'
param([string]$Receipt)
$child = Start-Process -FilePath (Join-Path $PSHOME 'pwsh.exe') -WindowStyle Hidden -PassThru `
    -ArgumentList @('-NoProfile','-Command','Start-Sleep -Seconds 90')
@{ parentPid=$PID; childPid=$child.Id; childStartTicks=$child.StartTime.Ticks } |
    ConvertTo-Json | Set-Content -LiteralPath $Receipt
Start-Sleep -Seconds 90
'@
[IO.File]::WriteAllText($helper, $text)
$clock = [Diagnostics.Stopwatch]::StartNew()
$run = Invoke-CapacityL6Process -Executable (Join-Path $PSHOME 'pwsh.exe') `
    -Arguments @('-NoProfile','-File',$helper,'-Receipt',$receipt) -Validated $validated -ListOnly
$owned = Get-Content -LiteralPath $receipt -Raw | ConvertFrom-Json
$parentStillExists = $null -ne (Get-Process -Id $owned.parentPid -ErrorAction SilentlyContinue)
$child = Get-Process -Id $owned.childPid -ErrorAction SilentlyContinue
$childStillExists = $null -ne $child -and $child.StartTime.Ticks -eq $owned.childStartTicks
if (-not $run.TimedOut -or $parentStillExists -or $childStillExists -or $clock.Elapsed.TotalSeconds -gt 65) {
    throw 'owned enumeration deadline did not stop and join its process tree'
}
[ordered]@{
    status='passed'; acceptancePassed=$false; timeoutSeconds=30
    elapsedSeconds=$clock.Elapsed.TotalSeconds; parentAbsent=(-not $parentStillExists)
    childAbsent=(-not $childStillExists); noHarnessOrAuthentication=$true
    note='Tests the shared process timeout path through inert enumeration; does not shorten product idle time.'
} | ConvertTo-Json
