#requires -Version 7.4
[CmdletBinding()]
param(
    [ValidateSet('Validate','List','Run')][string]$Mode = 'Validate',
    [Parameter(Mandatory)][string]$Config,
    [Parameter(Mandatory)][string]$RepositoryRoot,
    [string]$TestExecutable,
    [string]$TestExecutableSha256,
    [string]$SourceManifest,
    [ValidateRange(1801,86400)][int]$CaseMaxSeconds=10800,
    [ValidateSet('cp011','cp023','cp026','cp032')][string[]]$Cases = @('cp011','cp023','cp026','cp032')
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'CapacityL6.psm1') -Force

try {
    if ($null -eq $Cases -or $Cases.Count -eq 0 -or
        @($Cases | Select-Object -Unique).Count -ne $Cases.Count) {
        throw 'at least one unique requested case is required'
    }
    $validated = Test-CapacityL6Configuration -Config $Config -RepositoryRoot $RepositoryRoot
    if ($Mode -eq 'Validate') {
        $validated.Summary | ConvertTo-Json -Depth 8
        exit 0
    }
    if (-not [IO.Path]::IsPathFullyQualified($TestExecutable) -or
        -not [IO.File]::Exists($TestExecutable) -or
        [IO.Path]::GetExtension($TestExecutable) -ine '.exe' -or
        $TestExecutableSha256 -cnotmatch '^[0-9a-f]{64}$' -or
        (Get-FileHash -LiteralPath $TestExecutable -Algorithm SHA256).Hash.ToLowerInvariant() -cne $TestExecutableSha256) {
        throw 'test executable requires an explicit existing path and matching lowercase SHA256'
    }
    $names = Get-CapacityL6Cases
    $enumeration = Invoke-CapacityL6Process -Executable $TestExecutable `
        -Arguments @('--list','--ignored','--format','terse') -Validated $validated -ListOnly
    Assert-CapacityL6Inputs $validated
    if ((Get-FileHash -LiteralPath $TestExecutable -Algorithm SHA256).Hash.ToLowerInvariant() -cne $TestExecutableSha256) {
        throw 'test executable changed during enumeration'
    }
    if ($enumeration.TimedOut -or $enumeration.ExitCode -ne 0) { throw 'test enumeration failed' }
    foreach ($case in $Cases) {
        if ($enumeration.Listing -notmatch ('(?m)^' + [regex]::Escape($names[$case]) + ': test\r?$')) {
            throw 'the selected ignored L6 entrypoint is absent from this executable'
        }
    }
    if ($Mode -eq 'List') {
        [ordered]@{ status='listed'; acceptancePassed=$false; testExecutableSha256=$TestExecutableSha256
            tests=@($Cases | ForEach-Object { $names[$_] }) } | ConvertTo-Json -Depth 4
        exit 0
    }
    Assert-CapacityL6Inputs $validated
    $runBinding = New-CapacityL6ExecutionContext -SourceManifest $SourceManifest `
        -RepositoryRoot $RepositoryRoot -TestExecutableSha256 $TestExecutableSha256 -Validated $validated
    $results = @()
    $executions = @()
    foreach ($case in ($Cases | Select-Object -Unique)) {
        Assert-CapacityL6Inputs $validated
        if ((Get-FileHash -LiteralPath $TestExecutable -Algorithm SHA256).Hash.ToLowerInvariant() -cne $TestExecutableSha256) {
            throw 'test executable changed after enumeration'
        }
        $resultPath = Join-Path $validated.RunRoot "$case/$case.result.json"
        if (Test-Path -LiteralPath $resultPath) { throw 'refusing a pre-existing case result' }
        $run = Invoke-CapacityL6Process -Executable $TestExecutable `
            -Arguments @($names[$case],'--exact','--ignored','--test-threads=1') -Validated $validated `
            -CaseMaxSeconds $CaseMaxSeconds -RunBinding $runBinding
        Assert-CapacityL6Inputs $validated
        Assert-CapacityL6ExecutionContext $runBinding
        if ((Get-FileHash -LiteralPath $TestExecutable -Algorithm SHA256).Hash.ToLowerInvariant() -cne $TestExecutableSha256) {
            throw 'test executable changed during execution'
        }
        $executionHash = if (Test-Path -LiteralPath $resultPath -PathType Leaf) {
            (Get-FileHash -LiteralPath $resultPath -Algorithm SHA256).Hash.ToLowerInvariant()
        } else { $null }
        $executions += [ordered]@{ case=$case; executionFile="$case/$case.result.json"
            executionSha256=$executionHash; exitCode=$run.ExitCode; timedOut=[bool]$run.TimedOut }
        if ($run.TimedOut) {
            $results += [ordered]@{ case=$case; status='failed'; reason='case deadline exceeded'
                cleanupConfirmed=$false; exitCode=$run.ExitCode }
            break
        }
        $results += Read-CapacityL6Result -RunRoot $validated.RunRoot -ExitCode $run.ExitCode -Case $case
    }
    $manifestHash = Write-CapacityL6ExecutionManifest $runBinding $validated.RunRoot $executions
    $passed = Test-CapacityL6Completion -Requested $Cases -Results $results
    [ordered]@{ status=$(if ($passed) { 'passed' } else { 'incomplete' }); acceptancePassed=$passed
        testExecutableSha256=$TestExecutableSha256; executionManifestSha256=$manifestHash
        cases=$results } | ConvertTo-Json -Depth 6
    if ($passed) { exit 0 }
    if (@($results | Where-Object status -CEQ 'failed').Count -gt 0) { exit 1 }
    exit 2
} catch {
    # Never echo exception messages: JSON, paths, native payloads and credentials can appear in them.
    [ordered]@{ status='failed'; acceptancePassed=$false; errorCategory='runner_validation_or_execution'
        detail='Input, ownership, enumeration, or result validation failed; raw error contents were suppressed.' } |
        ConvertTo-Json -Compress
    exit 1
}
