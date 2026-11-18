#requires -Version 7.4
[CmdletBinding()]
param([Parameter(Mandatory)][string]$RepositoryRoot, [Parameter(Mandatory)][string]$EvidenceRoot)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'CapacityL6.psm1') -Force

# These are inert local fixtures, never an authenticated profile or L6 execution.
$area = Join-Path $EvidenceRoot ('runner-selftest-' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $area
$dedicatedDirectory = Join-Path $area 'dedicated'
$codexDirectory = Join-Path $area 'dedicated-codex'
foreach ($directory in @($dedicatedDirectory,$codexDirectory,'appdata','localappdata','temp')) {
    $path = if ([IO.Path]::IsPathFullyQualified($directory)) { $directory } else { Join-Path $dedicatedDirectory $directory }
    $null = New-Item -ItemType Directory -Path $path
}
function Encode([string]$Value) { [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($Value)) }
$environment = [ordered]@{
    HOME=$dedicatedDirectory; USERPROFILE=$dedicatedDirectory; CODEX_HOME=$codexDirectory
    APPDATA=(Join-Path $dedicatedDirectory 'appdata'); LOCALAPPDATA=(Join-Path $dedicatedDirectory 'localappdata')
    TEMP=(Join-Path $dedicatedDirectory 'temp'); TMP=(Join-Path $dedicatedDirectory 'temp')
}
$adapter = Join-Path $area 'inert-adapter.js'
[IO.File]::WriteAllText($adapter, "throw new Error('validator fixture must never execute');")
$profile = [ordered]@{
    format=1
    launch=[ordered]@{
        executable_utf16le_base64=(Encode (Get-Command node -CommandType Application | Select-Object -First 1).Source)
        arguments_utf16le_base64=@((Encode $adapter))
        environment=@($environment.GetEnumerator() | ForEach-Object {
            @{ name_utf16le_base64=(Encode $_.Key); value_utf16le_base64=(Encode $_.Value) }
        })
    }
    recipes=@(@{ wire=2; mcp_servers=@(); additional_directories=@(); extra=@{} })
}
$profilePath = Join-Path $area 'inert-profile.json'
$configPath = Join-Path $area 'config.json'
[IO.File]::WriteAllText($profilePath, ($profile | ConvertTo-Json -Depth 12))
$config = [ordered]@{
    format=1; runRoot=(Join-Path $RepositoryRoot ('.tmp/capacity-request-tester/never-run-' + [Guid]::NewGuid().ToString('N')))
    launchProfile=$profilePath; dedicatedHome=$dedicatedDirectory; dedicatedCodexHome=$codexDirectory
    defaultLimit=50; idleSeconds=1800; scenarioFiles=@{}
}
function Write-Config { [IO.File]::WriteAllText($configPath, ($config | ConvertTo-Json -Depth 12)) }
function Write-Profile { [IO.File]::WriteAllText($profilePath, ($profile | ConvertTo-Json -Depth 12)) }
$checks = [Collections.Generic.List[string]]::new()
function Reject([string]$Name, [scriptblock]$Action) {
    $rejected = $false
    try { & $Action > $null } catch { $rejected = $true }
    if (-not $rejected) { throw "expected rejection: $Name" }
    $checks.Add($Name)
}
function Validate { Test-CapacityL6Configuration -Config $configPath -RepositoryRoot $RepositoryRoot }
Write-Config
$good = Validate
if ($good.Summary.acceptancePassed -or $good.Summary.allowModelRequests -or
    $good.Summary.missingScenarioCases.Count -ne 4 -or (Test-Path -LiteralPath $config.runRoot)) {
    throw 'validation created a run or claimed acceptance/authentication'
}
$checks.Add('valid inert configuration remains preflight; default model=false; four missing scenarios')
$config.idleSeconds = 1
Write-Config
Reject 'cannot shorten real idle interval' { Validate }
$config.idleSeconds = 1800
$config.defaultLimit = 1
Write-Config
Reject 'cannot reduce real capacity limit' { Validate }
$config.defaultLimit = 50
$config.allowModelRequests = 'false'
Write-Config
Reject 'model consent cannot be coerced from string' { Validate }
$config.Remove('allowModelRequests')
$originalRun = $config.runRoot
$config.runRoot = Join-Path $area 'wrong-owner'
Write-Config
Reject 'run ownership must be tester root' { Validate }
$config.runRoot = $dedicatedDirectory
Write-Config
Reject 'existing authenticated directory cannot be runRoot' { Validate }
$config.runRoot = $originalRun
$config.dedicatedHome = $env:USERPROFILE
Write-Config
Reject 'default identity directory is rejected' { Validate }
$config.dedicatedHome = $dedicatedDirectory
$config.scenarioFiles = @{ fake='do-not-read' }
Write-Config
Reject 'unknown scenario key cannot be silently ignored' { Validate }
$config.scenarioFiles = @{}
$config.webControl = @{ enabled=$true; timeoutSeconds=0 }
Write-Config
Reject 'browser control timeout must be positive' { Validate }
$config.Remove('webControl')
Write-Config
$text = [IO.File]::ReadAllText($configPath)
[IO.File]::WriteAllText($configPath, $text.Replace('"format": 1','"format": 1, "format": 1'))
Reject 'duplicate config key' { Validate }
Write-Config
$profile.launch.environment += @{ name_utf16le_base64=(Encode 'OPENAI_API_KEY'); value_utf16le_base64=(Encode 'inert-not-a-key') }
Write-Profile
Reject 'token environment is rejected' { Validate }
$profile.launch.environment = @($profile.launch.environment | Select-Object -SkipLast 1)
$profile.launch.environment += @{ name_utf16le_base64=(Encode 'INITIAL_AGENT_MODE'); value_utf16le_base64=(Encode 'agent') }
Write-Profile
Reject 'arbitrary INITIAL_AGENT_MODE is rejected' { Validate }
$profile.launch.environment = @($profile.launch.environment | Select-Object -SkipLast 1)
$profile.launch.environment += @{ name_utf16le_base64=(Encode 'INITIAL_AGENT_MODE'); value_utf16le_base64=(Encode 'read-only') }
$profile.launch.environment += @{ name_utf16le_base64=(Encode 'NO_BROWSER'); value_utf16le_base64=(Encode '0') }
Write-Profile
Reject 'non-1 NO_BROWSER is rejected' { Validate }
$profile.launch.environment = @($profile.launch.environment | Select-Object -SkipLast 1)
$profile.launch.environment += @{ name_utf16le_base64=(Encode 'NO_BROWSER'); value_utf16le_base64=(Encode '1') }
Write-Profile
$null = Validate
$checks.Add('explicit INITIAL_AGENT_MODE read-only and NO_BROWSER=1 are accepted while disallowed values reject')
$profile.launch.environment = @($profile.launch.environment | Select-Object -SkipLast 2)
$originalTemp = $profile.launch.environment[5].value_utf16le_base64
$profile.launch.environment[5].value_utf16le_base64 = Encode $env:TEMP
Write-Profile
Reject 'default temp cannot be used' { Validate }
$profile.launch.environment[5].value_utf16le_base64 = $originalTemp
$profile.recipes[0].extra = @{ unreviewed='inert' }
Write-Profile
Reject 'extra launch surface is not silently accepted' { Validate }
$profile.recipes[0].extra = @{}
Write-Profile
$good = Validate
[IO.File]::AppendAllText($adapter, "`n// changed")
Reject 'input change invalidates validation' { Assert-CapacityL6Inputs $good }
$caseArea = Join-Path $area 'cp011'
$null = New-Item -ItemType Directory -Path $caseArea
$resultPath = Join-Path $caseArea 'cp011.result.json'
$record = Read-CapacityL6Result -RunRoot $area -ExitCode 0 -Case cp011
if ($record.status -ne 'failed') { throw 'missing Rust result must fail even with libtest exit0' }
$checks.Add('libtest pass cannot substitute for Rust result')
[IO.File]::WriteAllText($resultPath, '{"status":"blocked","steps":[]}')
$record = Read-CapacityL6Result -RunRoot $area -ExitCode 101 -Case cp011
if ($record.status -ne 'blocked') { throw 'blocked status was lost' }
$checks.Add('blocked remains blocked')
$record = Read-CapacityL6Result -RunRoot $area -ExitCode 0 -Case cp011
if ($record.status -ne 'failed') { throw 'blocked cannot masquerade as libtest success' }
$checks.Add('blocked plus exit0 is rejected')
[IO.File]::WriteAllText($resultPath, '{"status":"passed","steps":[]}')
$record = Read-CapacityL6Result -RunRoot $area -ExitCode 101 -Case cp011
if ($record.status -ne 'failed') { throw 'Rust pass cannot mask failing libtest' }
$checks.Add('Rust pass plus nonzero exit is rejected')
[IO.File]::WriteAllText($resultPath, '{"status":"passed","steps":[],"missingBranches":["unexecuted"]}')
$record = Read-CapacityL6Result -RunRoot $area -ExitCode 0 -Case cp011
if ($record.status -ne 'failed') { throw 'missing branches cannot pass' }
$checks.Add('missing branches cannot pass even with exit0')
[IO.File]::WriteAllText($resultPath, '{"status":"passed","steps":[],"missingBranches":[]}')
$record = Read-CapacityL6Result -RunRoot $area -ExitCode 0 -Case cp011
if ($record.status -ne 'failed') { throw 'bare status cannot prove cleanup or branch coverage' }
$checks.Add('bare status cannot prove cleanup or branch coverage')
$matrix = Get-CapacityL6Matrix
$complete = @{
    format=1; case='cp011'; status='passed'; steps=@(); missingBranches=@()
    defaultLimit=50; idleSeconds=1800; realHarness=$true; fullCpPassed=$true; cleanupJoined=$false
    entryImplementationComplete=$true; jobActiveProcesses=0; portClosed=$true; branchMatrixSha256=$matrix.Sha256
    requiredBranches=@($matrix.Document.cases.cp011 | ForEach-Object { @{ id=$_; status='passed' } })
}
[IO.File]::WriteAllText($resultPath, ($complete | ConvertTo-Json -Depth 6))
$record = Read-CapacityL6Result -RunRoot $area -ExitCode 0 -Case cp011
if ($record.status -ne 'failed') { throw 'unjoined cleanup must fail' }
$checks.Add('unjoined cleanup cannot pass')
$complete.cleanupJoined = $true
$complete.requiredBranches[0].status = 'blocked'
[IO.File]::WriteAllText($resultPath, ($complete | ConvertTo-Json -Depth 6))
$record = Read-CapacityL6Result -RunRoot $area -ExitCode 0 -Case cp011
if ($record.status -ne 'failed') { throw 'unconfirmed required branch must fail' }
$checks.Add('unconfirmed required branch cannot pass')
$complete.requiredBranches[0].status = 'passed'
$allBranches = $complete.requiredBranches
$complete.requiredBranches = @($allBranches | Select-Object -SkipLast 1)
[IO.File]::WriteAllText($resultPath, ($complete | ConvertTo-Json -Depth 6))
$record = Read-CapacityL6Result -RunRoot $area -ExitCode 0 -Case cp011
if ($record.status -ne 'failed') { throw 'omitted matrix branch must fail' }
$checks.Add('omitting one expected branch cannot pass')
$complete.requiredBranches = $allBranches
$complete.branchMatrixSha256 = '0' * 64
[IO.File]::WriteAllText($resultPath, ($complete | ConvertTo-Json -Depth 6))
$record = Read-CapacityL6Result -RunRoot $area -ExitCode 0 -Case cp011
if ($record.status -ne 'failed') { throw 'foreign matrix must fail' }
$checks.Add('foreign matrix digest cannot pass')
$complete.branchMatrixSha256 = $matrix.Sha256
$complete.jobActiveProcesses = 1
[IO.File]::WriteAllText($resultPath, ($complete | ConvertTo-Json -Depth 6))
Reject 'live owned Job process cannot pass' { Read-CapacityL6Result -RunRoot $area -ExitCode 0 -Case cp011 }
$complete.jobActiveProcesses = 0
$complete.portClosed = $false
[IO.File]::WriteAllText($resultPath, ($complete | ConvertTo-Json -Depth 6))
$record = Read-CapacityL6Result -RunRoot $area -ExitCode 0 -Case cp011
if ($record.status -ne 'failed') { throw 'open owned port must fail' }
$checks.Add('open owned port cannot pass')
$complete.portClosed = $true
[IO.File]::WriteAllText($resultPath, ($complete | ConvertTo-Json -Depth 6))
$record = Read-CapacityL6Result -RunRoot $area -ExitCode 0 -Case cp011
if ($record.status -ne 'blocked') { throw 'legacy result must not bypass independent review' }
$checks.Add('legacy full-contract result cannot bypass independent review')
$badConfig = Join-Path $area 'malformed.json'
[IO.File]::WriteAllText($badConfig, '{"secret-sentinel-DO-NOT-PRINT": this_is_invalid}')
$runner = Join-Path $PSScriptRoot 'Invoke-CapacityL6.ps1'
$safeError = & pwsh -NoProfile -File $runner -Mode Validate -Config $badConfig -RepositoryRoot $RepositoryRoot
if ($LASTEXITCODE -ne 1 -or ($safeError -join "`n").Contains('secret-sentinel-DO-NOT-PRINT')) {
    throw 'public error output leaked input or accepted malformed JSON'
}
$checks.Add('public catch suppresses malformed JSON content')
if (Test-CapacityL6Completion -Requested @() -Results @()) { throw 'empty cases cannot pass' }
$checks.Add('empty request and result set cannot pass')
if (Test-CapacityL6Completion -Requested @('cp011') -Results @()) { throw 'missing results cannot pass' }
$checks.Add('nonempty request with empty results cannot pass')
if (Test-CapacityL6Completion -Requested @('cp011','cp023') `
        -Results @(@{case='cp011';status='passed'},@{case='cp011';status='passed'})) {
    throw 'duplicate result cannot replace a requested case'
}
$checks.Add('duplicate result cannot replace requested case')
if (Test-CapacityL6Completion -Requested @('cp011') -Results @(@{case='cp023';status='passed'})) {
    throw 'foreign result cannot satisfy requested case'
}
$checks.Add('foreign result cannot satisfy requested case')
if (-not (Test-CapacityL6Completion -Requested @('cp011') -Results @(@{case='cp011';status='passed'}))) {
    throw 'inert exact matching result should map to complete'
}
$checks.Add('inert exact requested/result case mapping only')
$emptyProbe = Join-Path $area 'empty-cases-probe.ps1'
[IO.File]::WriteAllText($emptyProbe, @'
param([string]$Runner,[string]$Config,[string]$RepositoryRoot)
& $Runner -Mode Run -Cases @() -Config $Config -RepositoryRoot $RepositoryRoot
exit $LASTEXITCODE
'@)
$emptyResult = & pwsh -NoProfile -File $emptyProbe -Runner $runner -Config $configPath -RepositoryRoot $RepositoryRoot
$emptyExit = $LASTEXITCODE
$emptyJson = $emptyResult | ConvertFrom-Json
if ($emptyExit -ne 1 -or $emptyJson.acceptancePassed -ne $false) {
    throw 'public CLI must reject an explicitly empty case array'
}
$checks.Add('real CLI rejects explicit empty case array before starting any executable')
[ordered]@{ status='passed'; checks=$checks.Count; names=$checks; acceptancePassed=$false; fixtureArea=$area
    note='Only validator/result-mapping checks; no profile/test executable or model was run.' } |
    ConvertTo-Json -Depth 6
