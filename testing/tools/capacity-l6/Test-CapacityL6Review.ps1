#requires -Version 7.4
param([Parameter(Mandatory)][string]$EvidenceRoot)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'CapacityL6Review.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'CapacityL6.psm1') -Force
$testRoot = Join-Path $EvidenceRoot ('l6-review-inert-' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $testRoot
$checks = [Collections.Generic.List[string]]::new()

function Save-InertJson($Path, $Document) {
    [IO.File]::WriteAllText($Path, ($Document | ConvertTo-Json -Depth 40) + "`n")
}
function Hash-InertFile($Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function New-InertReview([scriptblock]$Mutation = {}) {
    $root = Join-Path $testRoot ([Guid]::NewGuid().ToString('N'))
    $null = New-Item -ItemType Directory -Path (Join-Path $root 'cp026/web-inert')
    $null = New-Item -ItemType Directory -Path (Join-Path $root 'independent-review')
    $matrix = Get-CapacityL6Matrix
    $binding = [ordered]@{ phaseId='inert-phase'; nodeId='inert-node'; sourceId='inert-source'
        url='https://localhost:12345'; nonce='inert-public-nonce'
        expected=[ordered]@{ nativeIds=@('inert-native'); operationIds=@('inert-op'); occupied=1 } }
    $phase = [ordered]@{ phaseId=$binding.phaseId; nodeId=$binding.nodeId; sourceId=$binding.sourceId
        url=$binding.url; nativeIds=@('inert-native'); operationIds=@('inert-op') }
    $summary = [ordered]@{ status='pending_independent_review'; acceptancePassed=$false
        browserIdentityRevoked=$true; browserClientId='inert-browser'
        phaseId=$binding.phaseId; nodeId=$binding.nodeId; sourceId=$binding.sourceId
        url=$binding.url; expected=$binding.expected }
    $capture = [ordered]@{ kind='independent-browser-dom-capture'; binding=$binding
        browserClientId='inert-browser'; dom=@{ scope='INERT: no browser ran' } }
    $actions = [ordered]@{ kind='independent-browser-actions'; binding=$binding
        browserClientId='inert-browser'; actions=@('INERT: no actions ran') }
    $cleanup = [ordered]@{ joined=$true; metadataConfirmed=$true; jobActiveProcesses=0
        portClosed=$true; sources=@(@{physicalPresent=$true; metadataConfirmed=$true; activeProcesses=0})
        binding=@{nodeId='inert-node'; sourceId='inert-source'; url='https://localhost:12345'} }
    $fingerprint = [ordered]@{ exeSha256=('a'*64); sourceManifestSha256=('b'*64)
        configSha256=('c'*64); branchMatrixSha256=$matrix.Sha256 }
    $execution = [ordered]@{ format=2; kind='capacity_l6_execution'; runId='inert-run'; case='cp026'
        fingerprint=$fingerprint; status='pending_independent_review'; entryImplementationComplete=$true
        executionComplete=$true; realHarness=$true; fullCpPassed=$false; defaultLimit=50; idleSeconds=1800
        missingBranches=@(); missingImplementationBranches=@(); steps=@()
        pendingReasons=@('independent_review'); cleanup=$cleanup; webPhases=@($phase)
        binding=@{nodeId='inert-node'; sourceId='inert-source'; url='https://localhost:12345'}
        requiredBranches=@($matrix.Document.cases.cp026 | ForEach-Object {
            @{id=$_; status=$(if ($_ -ceq 'same_node_real_harness_web') {'pending_review'} else {'passed'})}
        }
        ) }
    $entry = [ordered]@{case='cp026'; executionFile='cp026/cp026.result.json'; exitCode=0; timedOut=$false}
    $caseReview = [ordered]@{case='cp026'; branch='same_node_real_harness_web'; decision='approved'
        reportFile='independent-review/report.md'; phases=@()}
    $fixture = @{ Root=$root; Phase=$phase; Summary=$summary; Capture=$capture; Actions=$actions
        Cleanup=$cleanup; Execution=$execution; Entry=$entry; Review=$caseReview
        Report='INERT contract test. No independent browser or real Harness was executed.' }
    & $Mutation $fixture
    $prefix = 'cp026/web-inert/'
    foreach ($name in @('capture','actions','captureSource')) {
        $relative = $prefix + $(if ($name -eq 'captureSource') { 'capture-source.txt' } else { "$name.json" })
        $path = Join-Path $root $relative
        if ($name -eq 'captureSource') { [IO.File]::WriteAllText($path, 'INERT source; no browser') }
        else { Save-InertJson $path $fixture[$name] }
        $phase[$name+'File'] = $relative
        $phase[$name+'Sha256'] = Hash-InertFile $path
        $summary[$name+'Sha256'] = $phase[$name+'Sha256']
    }
    $phase.summaryFile = $prefix + 'summary.json'
    Save-InertJson (Join-Path $root $phase.summaryFile) $summary
    $phase.summarySha256 = Hash-InertFile (Join-Path $root $phase.summaryFile)
    $phaseReview = [ordered]@{phaseId=$phase.phaseId}
    foreach ($name in @('summary','capture','actions','captureSource')) {
        $phaseReview[$name+'Sha256'] = $phase[$name+'Sha256']
    }
    if ($fixture.Contains('ReviewPhases')) { $caseReview.phases=$fixture.ReviewPhases }
    else { $caseReview.phases=@($phaseReview) }
    $cleanup.sourceDiagnosticsFile='cp026/cleanup.json'
    Save-InertJson (Join-Path $root $cleanup.sourceDiagnosticsFile) $cleanup
    $cleanup.sourceDiagnosticsSha256=Hash-InertFile (Join-Path $root $cleanup.sourceDiagnosticsFile)
    Save-InertJson (Join-Path $root $entry.executionFile) $execution
    $entry.executionSha256=Hash-InertFile (Join-Path $root $entry.executionFile)
    $caseReview.executionSha256=$entry.executionSha256
    [IO.File]::WriteAllText((Join-Path $root $caseReview.reportFile), $fixture.Report)
    $caseReview.reportSha256=Hash-InertFile (Join-Path $root $caseReview.reportFile)
    $manifest=[ordered]@{format=1; kind='capacity_l6_execution_manifest'; runId='inert-run'
        fingerprint=$fingerprint; cases=@($entry)}
    Save-InertJson (Join-Path $root 'execution-manifest.json') $manifest
    $assessment=[ordered]@{format=1; kind='capacity_l6_independent_assessment'; runId='inert-run'
        executionManifestSha256=(Hash-InertFile (Join-Path $root 'execution-manifest.json')); cases=@($caseReview)}
    Save-InertJson (Join-Path $root 'independent-review/assessment.json') $assessment
    return @{RunRoot=$root; Cases=@('cp026'); Assessment='independent-review/assessment.json'
        AssessmentSha256=(Hash-InertFile (Join-Path $root 'independent-review/assessment.json'))}
}
function Reject-InertReview([string]$Name, [scriptblock]$Mutation) {
    $fixture = New-InertReview $Mutation
    $rejected = $false
    try { $null = Complete-CapacityL6Review @fixture -ConfirmIndependentReview }
    catch { $rejected = $true }
    if (-not $rejected) { throw "Missing refusal: $Name" }
    $checks.Add($Name)
}

$fixture = New-InertReview
$pending = Complete-CapacityL6Review @fixture
if ($pending.acceptancePassed -or (Test-Path (Join-Path $fixture.RunRoot 'reviewed-aggregate.json'))) {
    throw 'No explicit independent confirmation must remain pending'
}
$checks.Add('without explicit independent workflow confirmation remains pending')
$raw = Hash-InertFile (Join-Path $fixture.RunRoot 'cp026/cp026.result.json')
$passed = Complete-CapacityL6Review @fixture -ConfirmIndependentReview
if (-not $passed.acceptancePassed -or $raw -cne (Hash-InertFile (Join-Path $fixture.RunRoot 'cp026/cp026.result.json'))) {
    throw 'Inert complete combination must create only derived aggregate'
}
$checks.Add('inert complete combination creates new aggregate and leaves raw bytes intact')
$duplicate = $false
try { $null = Complete-CapacityL6Review @fixture -ConfirmIndependentReview } catch { $duplicate = $true }
if (-not $duplicate) { throw 'aggregate overwrite accepted' }
$checks.Add('existing aggregate cannot be overwritten')
Reject-InertReview 'missing node branch' { param($f) $f.Execution.requiredBranches=$f.Execution.requiredBranches[1..6] }
Reject-InertReview 'node failure cannot be approved by reviewer' { param($f) $f.Execution.requiredBranches[0].status='blocked' }
Reject-InertReview 'incomplete implementation' { param($f) $f.Execution.entryImplementationComplete=$false }
Reject-InertReview 'contradictory missing executed branch' {
    param($f) $f.Execution.missingBranches=@('unexecuted-required-branch')
}
Reject-InertReview 'contradictory missing implementation branch' {
    param($f) $f.Execution.missingImplementationBranches=@('unimplemented-required-branch')
}
Reject-InertReview 'missing branch declaration is required' { param($f) $f.Execution.Remove('missingBranches') }
Reject-InertReview 'failed execution state' { param($f) $f.Execution.status='failed' }
Reject-InertReview 'timeout' { param($f) $f.Entry.timedOut=$true }
Reject-InertReview 'failed process exit' { param($f) $f.Entry.exitCode=101 }
Reject-InertReview 'foreign run' { param($f) $f.Execution.runId='foreign' }
Reject-InertReview 'unknown cleanup' { param($f) $f.Cleanup.joined=$false }
Reject-InertReview 'empty original source cleanup' { param($f) $f.Cleanup.sources=@() }
Reject-InertReview 'active owned process' { param($f) $f.Cleanup.sources[0].activeProcesses=1 }
Reject-InertReview 'helper cannot self-sign DOM' { param($f) $f.Summary.acceptancePassed=$true }
Reject-InertReview 'foreign captured node' { param($f) $f.Phase.nodeId='different-node' }
Reject-InertReview 'internally consistent phase from a different actual node' {
    param($f)
    foreach ($document in @($f.Phase, $f.Summary, $f.Capture.binding, $f.Actions.binding)) {
        $document.nodeId='foreign-node'
        $document.sourceId='foreign-source'
        $document.url='https://localhost:54321'
    }
}
Reject-InertReview 'cleanup belongs to another original source' {
    param($f) $f.Cleanup.binding.sourceId='foreign-source'
}
Reject-InertReview 'empty DOM object' { param($f) $f.Capture.dom=@{} }
Reject-InertReview 'text is not DOM capture object' { param($f) $f.Capture.dom='INERT text' }
Reject-InertReview 'boolean is not DOM capture object' { param($f) $f.Capture.dom=$false }
Reject-InertReview 'foreign browser identity' { param($f) $f.Capture.browserClientId='different-browser' }
Reject-InertReview 'missing review phase' { param($f) $f.ReviewPhases=@() }
Reject-InertReview 'empty report' { param($f) $f.Report='' }
Reject-InertReview 'empty browser actions' { param($f) $f.Actions.actions=@() }
Reject-InertReview 'refused independent assessment' { param($f) $f.Review.decision='refused' }

foreach ($cases in @(@(), @('cp026','cp026'), @('cp011'))) {
    $fixture = New-InertReview
    $fixture.Cases = $cases
    $rejected = $false
    try { $null=Complete-CapacityL6Review @fixture -ConfirmIndependentReview } catch { $rejected=$true }
    if (-not $rejected) { throw 'invalid requested case collection accepted' }
    $checks.Add('empty duplicate or mismatched requested cases refused')
}
$fixture=New-InertReview
[IO.File]::AppendAllText((Join-Path $fixture.RunRoot 'cp026/web-inert/capture.json'), ' ')
$rejected=$false
try { $null=Complete-CapacityL6Review @fixture -ConfirmIndependentReview } catch { $rejected=$true }
if (-not $rejected) { throw 'changed capture bytes accepted' }
$checks.Add('changed original capture digest refused')

$fixture=New-InertReview
$pendingExecution=Read-CapacityL6Result -RunRoot $fixture.RunRoot -ExitCode 0 -Case cp026
if ($pendingExecution.status -cne 'pending_independent_review') {
    throw 'complete format2 execution must remain pending independent review'
}
$checks.Add('runner preserves format2 execution pending independent review')
foreach ($field in @('missingBranches', 'missingImplementationBranches')) {
    $fixture=New-InertReview
    $path=Join-Path $fixture.RunRoot 'cp026/cp026.result.json'
    $document=Get-Content -LiteralPath $path -Raw | ConvertFrom-Json -AsHashtable
    $document[$field]=@('unfulfilled-required-branch')
    Save-InertJson $path $document
    $rejected=$false
    try { $null=Read-CapacityL6Result -RunRoot $fixture.RunRoot -ExitCode 0 -Case cp026 }
    catch { $rejected=$true }
    if (-not $rejected) { throw 'runner accepted contradictory pending execution' }
    $checks.Add("runner refuses nonempty $field in format2 execution")
}

# Exercise the actual launcher environment with a harmless local PowerShell
# child. This does not start the product, an adapter, a browser or a model.
$processRoot = Join-Path $testRoot 'process-binding'
$null = New-Item -ItemType Directory -Path $processRoot
$helper = Join-Path $processRoot 'capture-binding.ps1'
[IO.File]::WriteAllText($helper, @'
param([string]$OutputFile)
$document = @{
    runId=$env:ACP_CAPACITY_L6_RUN_ID
    exeSha256=$env:ACP_CAPACITY_L6_EXE_SHA256
    sourceManifestSha256=$env:ACP_CAPACITY_L6_SOURCE_SHA256
    sourceManifest=$env:ACP_CAPACITY_L6_SOURCE_MANIFEST
    configSha256=$env:ACP_CAPACITY_L6_CONFIG_SHA256
}
[IO.File]::WriteAllText($OutputFile,($document | ConvertTo-Json -Compress))
'@)
$inertConfig = Join-Path $processRoot 'inert-config.json'
[IO.File]::WriteAllText($inertConfig, '{"scope":"inert binding check"}')
$inertSource = Join-Path $processRoot 'build-source.json'
$powerShellExe = (Get-Process -Id $PID).Path
$exeHash = Hash-InertFile $powerShellExe
Save-InertJson $inertSource @{testExecutableSha256=$exeHash
    files=@(@{path='capture-binding.ps1'; sha256=(Hash-InertFile $helper)})}
$validated = @{ConfigPath=$inertConfig; Environment=@{}}
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$runBinding = New-CapacityL6ExecutionContext -SourceManifest $inertSource `
    -RepositoryRoot $repository -TestExecutableSha256 $exeHash -Validated $validated
$outputFile = Join-Path $processRoot 'child-binding.json'
$processResult = Invoke-CapacityL6Process -Executable $powerShellExe `
    -Arguments @('-NoProfile','-File',$helper,'-OutputFile',$outputFile) `
    -Validated $validated -RunBinding $runBinding
if ($processResult.ExitCode -ne 0 -or $processResult.TimedOut) { throw 'inert binding child failed' }
$observed = Get-Content -LiteralPath $outputFile -Raw | ConvertFrom-Json -AsHashtable
if ($observed.runId -cne $runBinding.RunId -or $observed.sourceManifest -cne $inertSource) {
    throw 'run identity or source manifest not passed to child'
}
foreach ($field in @('exeSha256','sourceManifestSha256','configSha256')) {
    if ($observed[$field] -cne $runBinding.Fingerprint[$field]) { throw 'child binding differs' }
}
$checks.Add('actual inert child receives exact run and build/config fingerprints')
$manifestHash = Write-CapacityL6ExecutionManifest $runBinding $processRoot @(
    @{case='cp026'; executionFile='cp026/cp026.result.json'; executionSha256=('e'*64); exitCode=0; timedOut=$false})
if ($manifestHash -cne (Hash-InertFile (Join-Path $processRoot 'execution-manifest.json'))) {
    throw 'manifest hash differs'
}
$checks.Add('execution manifest is created with original run binding')
$rejected=$false
try { $null=Write-CapacityL6ExecutionManifest $runBinding $processRoot @(@{case='cp026'}) }
catch {$rejected=$true}
if (-not $rejected) {throw 'execution manifest was overwritten'}
$checks.Add('execution manifest cannot overwrite original')
[IO.File]::AppendAllText($inertConfig,' ')
$rejected=$false
try {Assert-CapacityL6ExecutionContext $runBinding} catch {$rejected=$true}
if (-not $rejected) {throw 'changed config binding accepted'}
$checks.Add('runner refuses changed config before subsequent execution or manifest')

[ordered]@{status='passed'; scope='Inert completion-algorithm development only; no real Harness or browser'
    checks=$checks.Count; names=@($checks); acceptancePassed=$false; evidenceRoot=$testRoot} |
    ConvertTo-Json -Depth 8
