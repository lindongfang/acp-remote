#requires -Version 7.4
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$script:Core = Import-Module (Join-Path $PSScriptRoot 'CapacityL6.psm1') -Force -PassThru
$script:ReviewImplementationPath = $PSCommandPath

function Assert-ReviewText($Value) {
    if ($Value -isnot [string] -or [string]::IsNullOrWhiteSpace($Value)) { throw 'missing text' }
}
function Assert-ReviewHash($Value) {
    if ($Value -isnot [string] -or $Value -cnotmatch '^[0-9a-f]{64}$') { throw 'invalid digest' }
}
function Assert-ReviewTrue($Value) {
    if ($Value -isnot [bool] -or -not $Value) { throw 'unconfirmed fact' }
}
function Assert-ReviewFalse($Value) {
    if ($Value -isnot [bool] -or $Value) { throw 'unexpected true fact' }
}
function Assert-ReviewNumber($Value, [long]$Expected) {
    & $script:Core { param($v, $n) Assert-Constant $v $n 'review integer' } $Value $Expected
}
function Read-ReviewJson([string]$Path) {
    & $script:Core { param($p) Read-StrictJson $p 'review input' } $Path
}
function Get-ReviewHash([string]$Path) {
    & $script:Core { param($p) Get-Sha256 $p } $Path
}
function Get-ReviewFile([string]$Root, $Relative, $ExpectedHash) {
    Assert-ReviewText $Relative
    Assert-ReviewHash $ExpectedHash
    if ([IO.Path]::IsPathRooted($Relative) -or @($Relative -split '[/\\]' |
            Where-Object { $_ -in @('..', '.', '') }).Count -gt 0) { throw 'invalid relative file' }
    $path = [IO.Path]::GetFullPath((Join-Path $Root $Relative))
    $null = & $script:Core { param($p) Get-AbsolutePath $p 'review file' File } $path
    $inside = & $script:Core { param($p, $r) Test-Within $p $r } $path $Root
    if (-not $inside -or (Get-ReviewHash $path) -cne $ExpectedHash) { throw 'file identity changed' }
    return $path
}
function Assert-ReviewStrings($Actual, $Expected, [switch]$AllowEmpty) {
    if ($Actual -isnot [array] -or $Expected -isnot [array] -or
        (-not $AllowEmpty -and ($Actual.Count -eq 0 -or $Expected.Count -eq 0))) { throw 'invalid set' }
    $left = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    $right = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($item in $Actual) { Assert-ReviewText $item; if (-not $left.Add($item)) { throw 'duplicate set item' } }
    foreach ($item in $Expected) { Assert-ReviewText $item; if (-not $right.Add($item)) { throw 'duplicate set item' } }
    if (-not $left.SetEquals($right)) { throw 'set differs' }
}
function Assert-ReviewFingerprint($Actual, $Expected, [string]$MatrixHash) {
    foreach ($field in @('exeSha256', 'sourceManifestSha256', 'configSha256', 'branchMatrixSha256')) {
        Assert-ReviewHash $Actual[$field]
        Assert-ReviewHash $Expected[$field]
        if ($Actual[$field] -cne $Expected[$field]) { throw 'run fingerprint differs' }
    }
    if ($Actual.branchMatrixSha256 -cne $MatrixHash) { throw 'unreviewed branch matrix' }
}
function Assert-ReviewBinding($Actual, $Expected) {
    foreach ($field in @('nodeId', 'sourceId', 'url')) {
        Assert-ReviewText $Actual[$field]
        Assert-ReviewText $Expected[$field]
        if ($Actual[$field] -cne $Expected[$field]) { throw 'different original node or source' }
    }
}
function Assert-ReviewCleanup($Cleanup, [string]$Root, $Binding) {
    Assert-ReviewTrue $Cleanup.joined
    Assert-ReviewTrue $Cleanup.metadataConfirmed
    Assert-ReviewTrue $Cleanup.portClosed
    Assert-ReviewNumber $Cleanup.jobActiveProcesses 0
    $path = Get-ReviewFile $Root $Cleanup.sourceDiagnosticsFile $Cleanup.sourceDiagnosticsSha256
    $evidence = Read-ReviewJson $path
    Assert-ReviewBinding $evidence.binding $Binding
    Assert-ReviewTrue $evidence.joined
    Assert-ReviewTrue $evidence.metadataConfirmed
    Assert-ReviewTrue $evidence.portClosed
    Assert-ReviewNumber $evidence.jobActiveProcesses 0
    if ($evidence.sources -isnot [array] -or $evidence.sources.Count -eq 0) { throw 'missing original sources' }
    foreach ($source in $evidence.sources) {
        Assert-ReviewTrue $source.physicalPresent
        Assert-ReviewTrue $source.metadataConfirmed
        Assert-ReviewNumber $source.activeProcesses 0
    }
}
function Assert-ReviewPhase($Phase, $Review, [string]$Root) {
    foreach ($name in @('phaseId', 'nodeId', 'sourceId', 'url')) { Assert-ReviewText $Phase[$name] }
    if ($Review.phaseId -cne $Phase.phaseId) { throw 'different reviewed phase' }
    $documents = @{}
    foreach ($name in @('summary', 'capture', 'actions', 'captureSource')) {
        $hashName = $name + 'Sha256'
        if ($Review[$hashName] -cne $Phase[$hashName]) { throw 'different reviewed evidence' }
        $path = Get-ReviewFile $Root $Phase[$name + 'File'] $Phase[$hashName]
        if ((Get-Item -LiteralPath $path).Length -eq 0) { throw 'empty phase evidence' }
        if ($name -ne 'captureSource') { $documents[$name] = Read-ReviewJson $path }
    }
    $summary = $documents.summary
    if ($summary.status -cne 'pending_independent_review') { throw 'helper cannot sign DOM' }
    Assert-ReviewFalse $summary.acceptancePassed
    Assert-ReviewTrue $summary.browserIdentityRevoked
    Assert-ReviewText $summary.browserClientId
    foreach ($name in @('phaseId', 'nodeId', 'sourceId', 'url')) {
        if ($summary[$name] -cne $Phase[$name]) { throw 'summary belongs to another phase' }
    }
    Assert-ReviewStrings $summary.expected.nativeIds $Phase.nativeIds
    Assert-ReviewStrings $summary.expected.operationIds $Phase.operationIds -AllowEmpty
    foreach ($name in @('capture', 'actions', 'captureSource')) {
        if ($summary[$name + 'Sha256'] -cne $Phase[$name + 'Sha256']) { throw 'summary digest differs' }
    }
    $capture = $documents.capture
    $actions = $documents.actions
    if ($capture.kind -cne 'independent-browser-dom-capture' -or
        $actions.kind -cne 'independent-browser-actions') { throw 'wrong evidence kind' }
    foreach ($document in @($capture, $actions)) {
        if ($document.browserClientId -cne $summary.browserClientId) { throw 'different browser identity' }
        foreach ($name in @('phaseId', 'nodeId', 'sourceId', 'url')) {
            if ($document.binding[$name] -cne $Phase[$name]) { throw 'different captured binding' }
        }
        Assert-ReviewText $document.binding.nonce
        Assert-ReviewStrings $document.binding.expected.nativeIds $Phase.nativeIds
        Assert-ReviewStrings $document.binding.expected.operationIds $Phase.operationIds -AllowEmpty
        foreach ($field in @('occupied', 'mode', 'historyIncomplete')) {
            if ($summary.expected.Contains($field) -and
                ($document.binding.expected[$field] | ConvertTo-Json -Compress) -cne
                ($summary.expected[$field] | ConvertTo-Json -Compress)) { throw 'different captured expectation' }
        }
    }
    if ($capture.binding.nonce -cne $actions.binding.nonce) { throw 'different captured nonce' }
    if ($capture.dom -isnot [System.Collections.IDictionary] -or $capture.dom.Count -eq 0 -or
        $actions.actions -isnot [array] -or $actions.actions.Count -eq 0) {
        throw 'missing captured DOM or actions'
    }
}

function Complete-CapacityL6Review {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$RunRoot,
        [Parameter(Mandatory)][string[]]$Cases,
        [Parameter(Mandatory)][string]$Assessment,
        [Parameter(Mandatory)][string]$AssessmentSha256,
        [switch]$ConfirmIndependentReview
    )
    $root = & $script:Core { param($p) Get-AbsolutePath $p 'review run root' Directory } $RunRoot
    $matrix = Get-CapacityL6Matrix
    Assert-ReviewStrings $Cases $Cases
    foreach ($case in $Cases) { if (-not $matrix.Document.cases.Contains($case)) { throw 'unknown case' } }
    $manifestPath = & $script:Core {
        param($p) Get-AbsolutePath $p 'execution manifest' File
    } (Join-Path $root 'execution-manifest.json')
    $manifestHash = Get-ReviewHash $manifestPath
    $manifest = Read-ReviewJson $manifestPath
    Assert-ReviewNumber $manifest.format 1
    if ($manifest.kind -cne 'capacity_l6_execution_manifest') { throw 'wrong manifest kind' }
    Assert-ReviewText $manifest.runId
    $reviewPath = Get-ReviewFile $root $Assessment $AssessmentSha256
    $review = Read-ReviewJson $reviewPath
    Assert-ReviewNumber $review.format 1
    if ($review.kind -cne 'capacity_l6_independent_assessment' -or
        $review.runId -cne $manifest.runId -or $review.executionManifestSha256 -cne $manifestHash) {
        throw 'review belongs to another execution'
    }
    if ($manifest.cases -isnot [array] -or $review.cases -isnot [array]) { throw 'cases must be arrays' }
    Assert-ReviewStrings @($manifest.cases | ForEach-Object { $_.case }) $Cases
    Assert-ReviewStrings @($review.cases | ForEach-Object { $_.case }) $Cases
    $outputs = @()
    $fixedFiles = @{}
    $fixedFiles[$manifestPath] = $manifestHash
    $fixedFiles[$reviewPath] = $AssessmentSha256
    $allPhases = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($entry in $manifest.cases) {
        Assert-ReviewNumber $entry.exitCode 0
        Assert-ReviewFalse $entry.timedOut
        $executionPath = Get-ReviewFile $root $entry.executionFile $entry.executionSha256
        $fixedFiles[$executionPath] = $entry.executionSha256
        $execution = Read-ReviewJson $executionPath
        Assert-ReviewNumber $execution.format 2
        if ($execution.kind -cne 'capacity_l6_execution' -or $execution.runId -cne $manifest.runId -or
            $execution.case -cne $entry.case -or $execution.status -cne 'pending_independent_review') {
            throw 'execution was not completed pending review'
        }
        Assert-ReviewFingerprint $execution.fingerprint $manifest.fingerprint $matrix.Sha256
        Assert-ReviewTrue $execution.entryImplementationComplete
        Assert-ReviewTrue $execution.executionComplete
        Assert-ReviewTrue $execution.realHarness
        Assert-ReviewFalse $execution.fullCpPassed
        foreach ($field in @('missingBranches', 'missingImplementationBranches')) {
            if ($execution[$field] -isnot [array] -or $execution[$field].Count -ne 0) {
                throw 'execution declares missing required branches'
            }
        }
        Assert-ReviewNumber $execution.defaultLimit 50
        Assert-ReviewNumber $execution.idleSeconds 1800
        Assert-ReviewStrings $execution.pendingReasons @('independent_review')
        if ($execution.requiredBranches -isnot [array]) { throw 'branches must be an array' }
        Assert-ReviewStrings @($execution.requiredBranches | ForEach-Object { $_.id }) @($matrix.Document.cases[$entry.case])
        foreach ($branch in $execution.requiredBranches) {
            $wanted = if ($branch.id -ceq 'same_node_real_harness_web') { 'pending_review' } else { 'passed' }
            if ($branch.status -cne $wanted) { throw 'review cannot repair a missing node branch' }
        }
        Assert-ReviewBinding $execution.binding $execution.binding
        Assert-ReviewCleanup $execution.cleanup $root $execution.binding
        $caseReview = @($review.cases | Where-Object { $_.case -ceq $entry.case })[0]
        if ($caseReview.executionSha256 -cne $entry.executionSha256 -or
            $caseReview.branch -cne 'same_node_real_harness_web' -or $caseReview.decision -cne 'approved') {
            throw 'case was not independently approved'
        }
        $reportPath = Get-ReviewFile $root $caseReview.reportFile $caseReview.reportSha256
        if ([string]::IsNullOrWhiteSpace([IO.File]::ReadAllText($reportPath))) { throw 'empty independent report' }
        $fixedFiles[$reportPath] = $caseReview.reportSha256
        if ($execution.webPhases -isnot [array] -or $caseReview.phases -isnot [array]) { throw 'phases must be arrays' }
        Assert-ReviewStrings @($execution.webPhases | ForEach-Object { $_.phaseId }) @($caseReview.phases | ForEach-Object { $_.phaseId })
        foreach ($phase in $execution.webPhases) {
            if (-not $allPhases.Add($phase.phaseId)) { throw 'phase reused across cases' }
            Assert-ReviewBinding $phase $execution.binding
            $phaseReview = @($caseReview.phases | Where-Object { $_.phaseId -ceq $phase.phaseId })[0]
            Assert-ReviewPhase $phase $phaseReview $root
            foreach ($name in @('summary', 'capture', 'actions', 'captureSource')) {
                $path = Get-ReviewFile $root $phase[$name + 'File'] $phase[$name + 'Sha256']
                $fixedFiles[$path] = $phase[$name + 'Sha256']
            }
        }
        $cleanupPath = Get-ReviewFile $root $execution.cleanup.sourceDiagnosticsFile $execution.cleanup.sourceDiagnosticsSha256
        $fixedFiles[$cleanupPath] = $execution.cleanup.sourceDiagnosticsSha256
        $outputs += [ordered]@{ case = $entry.case; executionSha256 = $entry.executionSha256
            reportSha256 = $caseReview.reportSha256; phases = @($caseReview.phases)
            nodeBasis = 'original_execution'; webBasis = 'independent_review' }
    }
    # A role string in JSON is not identity authentication. This explicit switch
    # is used only by the actual independent review workflow after reviewing DOM.
    if (-not $ConfirmIndependentReview) {
        return [ordered]@{ status = 'pending_independent_confirmation'; acceptancePassed = $false }
    }
    foreach ($path in $fixedFiles.Keys) {
        $null = & $script:Core { param($p) Get-AbsolutePath $p 'fixed review input' File } $path
        if ((Get-ReviewHash $path) -cne $fixedFiles[$path]) { throw 'evidence changed during review' }
    }
    $aggregate = [ordered]@{
        format = 1; kind = 'capacity_l6_reviewed_aggregate'; runId = $manifest.runId
        status = 'passed'; acceptancePassed = $true; executionManifestSha256 = $manifestHash
        assessmentSha256 = $AssessmentSha256; finalizerSha256 = (Get-ReviewHash $script:ReviewImplementationPath)
        approvalBoundary = 'Explicit confirmation by actual independent reviewer; JSON does not authenticate reviewer identity'
        cases = $outputs
    }
    $outputPath = Join-Path $root 'reviewed-aggregate.json'
    $bytes = [Text.Encoding]::UTF8.GetBytes(($aggregate | ConvertTo-Json -Depth 32) + "`n")
    $stream = [IO.File]::Open($outputPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $stream.Write($bytes); $stream.Flush($true) } finally { $stream.Dispose() }
    return [ordered]@{ status = 'passed'; acceptancePassed = $true; aggregateSha256 = (Get-ReviewHash $outputPath) }
}

Export-ModuleMember -Function Complete-CapacityL6Review
