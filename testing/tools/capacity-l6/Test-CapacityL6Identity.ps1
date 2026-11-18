#requires -Version 7.4
param([Parameter(Mandatory)][string]$EvidenceRoot)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'CapacityL6.psm1') -Force
$testRoot = Join-Path $EvidenceRoot ('l6-identity-inert-' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $testRoot
$helper = Join-Path $testRoot 'capture.ps1'
[IO.File]::WriteAllText($helper, @'
param([string]$OutputFile)
[IO.File]::WriteAllText($OutputFile, (@{
    defaultDirectories=$env:ACP_CAPACITY_L6_DEFAULT_IDENTITY_DIRECTORIES
    dedicatedHome=$env:HOME
    dedicatedProfile=$env:USERPROFILE
    dedicatedCodex=$env:CODEX_HOME
    config=$env:ACP_CAPACITY_L6_CONFIG
} | ConvertTo-Json -Compress))
'@)
$expected = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
foreach ($value in @($env:HOME, $env:USERPROFILE, $env:CODEX_HOME)) {
    if ($value -and [IO.Path]::IsPathFullyQualified($value)) {
        $null = $expected.Add([IO.Path]::GetFullPath($value).TrimEnd('\','/'))
    }
}
if ($env:USERPROFILE -and [IO.Path]::IsPathFullyQualified($env:USERPROFILE)) {
    $null = $expected.Add([IO.Path]::GetFullPath([IO.Path]::Combine($env:USERPROFILE, '.codex')).TrimEnd('\','/'))
}
if ($expected.Count -eq 0) { throw 'original default directory facts unavailable for test' }
$dedicated = Join-Path $testRoot 'INERT-dedicated-home'
$dedicatedCodex = Join-Path $testRoot 'INERT-dedicated-codex'
$validated = @{ConfigPath=(Join-Path $testRoot 'INERT-config.json')
    Environment=@{HOME=$dedicated; USERPROFILE=$dedicated; CODEX_HOME=$dedicatedCodex}}
$powerShellExe = (Get-Process -Id $PID).Path
$checks = [Collections.Generic.List[string]]::new()
foreach ($listOnly in @($false, $true)) {
    $outputFile = Join-Path $testRoot "capture-$listOnly.json"
    $result = Invoke-CapacityL6Process -Executable $powerShellExe `
        -Arguments @('-NoProfile','-File',$helper,'-OutputFile',$outputFile) `
        -Validated $validated -ListOnly:$listOnly
    if ($result.ExitCode -ne 0 -or $result.TimedOut) { throw 'inert identity child failed' }
    $observed = Get-Content -LiteralPath $outputFile -Raw | ConvertFrom-Json -AsHashtable
    if ($observed.dedicatedHome -cne $dedicated -or
        $observed.dedicatedProfile -cne $dedicated -or
        $observed.dedicatedCodex -cne $dedicatedCodex) {
        throw 'explicit dedicated environment was not preserved'
    }
    $checks.Add("explicit dedicated environment preserved (ListOnly=$listOnly)")
    if ($listOnly) {
        if ($observed.defaultDirectories -or $observed.config) {
            throw 'enumeration unexpectedly received identity execution facts'
        }
        $checks.Add('enumeration receives neither original identity facts nor run config')
    } else {
        if (-not $observed.defaultDirectories) { throw 'original default identity facts missing from actual child' }
        $actual = $observed.defaultDirectories | ConvertFrom-Json -NoEnumerate
        if ($actual -isnot [array] -or -not $expected.SetEquals([string[]]$actual)) {
            throw 'original default identity facts were replaced by dedicated child paths'
        }
        if ($actual -contains $dedicated -or $actual -contains $dedicatedCodex) {
            throw 'dedicated child paths were recorded as original defaults'
        }
        $checks.Add('actual child receives exact original defaults before environment replacement')
    }
}
[ordered]@{status='passed'; acceptancePassed=$false; checks=$checks.Count
    scope='inert PowerShell child only; no authentication reads, Node, Harness or model'
    evidenceRoot=$testRoot; verified=$checks} | ConvertTo-Json -Depth 8
