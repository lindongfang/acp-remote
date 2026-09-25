param([string]$FixtureRoot = '')
$ErrorActionPreference = 'Stop'
$OutputEncoding = [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new()
$schemaSource = Split-Path $PSScriptRoot -Parent
$projectRoot = [IO.Path]::GetFullPath((Join-Path $schemaSource '../../../../'))
$tempParent = [IO.Path]::GetFullPath((Join-Path $projectRoot '.tmp'))
# 宿主可传入已就绪的 fixture（预装项目本地引擎，使 npx --quiet --no-install openspec 可离线解析）；缺省自建临时目录。
if ([string]::IsNullOrWhiteSpace($FixtureRoot)) {
    $fixtureName = 'agentic-regression-' + [guid]::NewGuid().ToString('N')
    $fixtureRoot = Join-Path $tempParent $fixtureName
    $ownsFixture = $true
} else {
    $fixtureRoot = [IO.Path]::GetFullPath($FixtureRoot)
    $fixtureName = Split-Path $fixtureRoot -Leaf
    $ownsFixture = $false
}
$utf8 = [System.Text.UTF8Encoding]::new($false)
$passed = 0
$scenario = 'CLI-SETUP'

function Start-Scenario([string]$Id) { $script:scenario = $Id }
function Complete-Scenario {
    $script:passed++
    Write-Output "PASS [$script:scenario]"
}

function Assert-That($Condition, [string]$Message) {
    if (-not $Condition) { throw "[$script:scenario] $Message" }
}

function Assert-PathEqual([string]$Actual, [string]$Expected, [string]$Field) {
    Assert-That (-not [string]::IsNullOrWhiteSpace($Actual)) "$Field missing"
    Assert-That ([IO.Path]::IsPathRooted($Actual)) "$Field is not absolute: $Actual"
    Assert-That (([IO.Path]::GetFullPath($Actual).TrimEnd('\', '/')) -eq
        ([IO.Path]::GetFullPath($Expected).TrimEnd('\', '/'))) "$Field resolved outside expected fixture location: $Actual"
}

function Invoke-OpenSpecJson([string[]]$Arguments) {
    $raw = & npx --quiet --no-install openspec @Arguments
    if ($LASTEXITCODE -ne 0) { throw "[$script:scenario] openspec failed: $($Arguments -join ' ')" }
    return (($raw -join "`n") | ConvertFrom-Json)
}

# 本包自身的 bin 不进入 node_modules/.bin，扩展 CLI 按路径定位。
function Invoke-OpenSpecAgenticJson([string[]]$Arguments) {
    $raw = & node (Join-Path $projectRoot 'bin/openspec-agentic.mjs') @Arguments
    if ($LASTEXITCODE -ne 0) { throw "[$script:scenario] openspec-agentic failed: $($Arguments -join ' ')" }
    return (($raw -join "`n") | ConvertFrom-Json)
}

function Set-FixtureFile([string]$RelativePath, [string]$Content) {
    $target = Join-Path $fixtureRoot $RelativePath
    [IO.Directory]::CreateDirectory((Split-Path $target -Parent)) | Out-Null
    [IO.File]::WriteAllText($target, $Content, $utf8)
}

try {
    $cliVersion = & node (Join-Path $projectRoot 'bin/openspec-agentic.mjs') --version
    Assert-That ($LASTEXITCODE -eq 0) 'Cannot read CLI version'
    Write-Output "openspec-agentic: $($cliVersion -join ' ')"
    New-Item -ItemType Directory -Path (Join-Path $fixtureRoot 'openspec/schemas') -Force | Out-Null
    Copy-Item -LiteralPath $schemaSource -Destination (Join-Path $fixtureRoot 'openspec/schemas/agentic') -Recurse
    Copy-Item -LiteralPath (Join-Path $projectRoot 'assets/openspec/config.yaml') -Destination (Join-Path $fixtureRoot 'openspec/config.yaml')
    # Distinct fixture markers prove input routing without matching Chinese prose.
    $fixtureConfig = Get-Content -LiteralPath (Join-Path $fixtureRoot 'openspec/config.yaml') -Raw -Encoding UTF8
    $fixtureConfig = $fixtureConfig -replace '(?m)^context: \|\r?$', "context: |`n  CLI_CONTEXT_SENTINEL"
    $fixtureConfig = $fixtureConfig -replace '(?m)(  apply:\r?\n    guidance:)', "`$1`n      - CLI_APPLY_SENTINEL"
    $fixtureConfig = $fixtureConfig -replace '(?m)(  archive:\r?\n    guidance:)', "`$1`n      - CLI_ARCHIVE_SENTINEL"
    # 夹具项目级 E2E 入口：CLI-13 用它验证流程内执行能真的跑起来。
    $fixtureConfig = $fixtureConfig -replace '(?m)^(    command: )""', '$1"node --version"'
    Set-FixtureFile 'openspec/config.yaml' $fixtureConfig
    Push-Location $fixtureRoot
    try {
        Start-Scenario 'CLI-01'
        $validation = Invoke-OpenSpecJson @('schema', 'validate', 'agentic', '--json')
        Assert-That $validation.valid 'Schema validation failed'
        Complete-Scenario

        Start-Scenario 'CLI-02'
        # Existing tasks alone must not let an old change start implementation.
        $changePath = 'openspec/changes/regression'
        Set-FixtureFile "$changePath/.openspec.yaml" "schema: agentic`nskip_specs: true`n"
        Set-FixtureFile "$changePath/tasks.md" "- [ ] 1.1 Implement and verify`n"
        $apply = Invoke-OpenSpecJson @('instructions', 'apply', '--change', 'regression', '--json')
        Assert-That ($apply.state -eq 'blocked') 'Tasks alone unexpectedly unlocked apply'
        foreach ($required in @('proposal', 'design', 'plan')) {
            Assert-That ($apply.missingArtifacts -contains $required) "Missing dependency not reported: $required"
        }
        Complete-Scenario

        Start-Scenario 'CLI-03'
        Set-FixtureFile "$changePath/proposal.md" "## Why`nDocumentation-only regression fixture.`n"
        Set-FixtureFile "$changePath/design.md" "## Decisions`nNo product behavior changes.`n"
        Set-FixtureFile "$changePath/plan.md" "## Verification Strategy`n### Main E2E`nmode: required`n"
        $status = Invoke-OpenSpecJson @('status', '--change', 'regression', '--json')
        Assert-That (($status.artifacts | Where-Object id -eq 'specs').status -eq 'skipped') 'skip_specs was not respected'
        $apply = Invoke-OpenSpecJson @('instructions', 'apply', '--change', 'regression', '--json')
        Assert-That ($apply.state -eq 'ready') 'Valid skip_specs change did not unlock apply'
        Assert-PathEqual $apply.changeDir (Join-Path $fixtureRoot $changePath) 'apply.changeDir'
        $proposal = Invoke-OpenSpecJson @('instructions', 'proposal', '--change', 'regression', '--json')
        Assert-PathEqual $proposal.changeDir (Join-Path $fixtureRoot $changePath) 'proposal.changeDir'
        Assert-PathEqual $proposal.planningHome.root $fixtureRoot 'proposal.planningHome.root'
        $readyInstruction = [string]$apply.instruction
        foreach ($token in @('roles/reviewer.md', 'roles/tester.md', 'agentic-verify')) {
            Assert-That ($readyInstruction.Contains($token)) "Ready apply lost custom routing: $token"
        }
        Assert-That ($apply.context -match 'CLI_CONTEXT_SENTINEL') 'Ready context lost fixture marker'
        Assert-That (($apply.operationGuidance -join ' ') -match 'CLI_APPLY_SENTINEL' -and ($apply.operationGuidance -join ' ') -match 'agentic-verify') 'Ready apply guidance lost marker or acceptance routing'
        Assert-That (@($apply.contextFiles.'plan' | Where-Object { (Split-Path $_ -Leaf) -eq 'plan.md' }).Count -eq 1) 'Apply did not resolve the renamed plan file'
        $planInstructions = Invoke-OpenSpecJson @('instructions', 'plan', '--change', 'regression', '--json')
        Assert-That ($planInstructions.artifactId -eq 'plan' -and $planInstructions.outputPath -eq 'plan.md') 'Plan filename changed the artifact ID or output contract'
        Assert-That ($planInstructions.template -match '## Scope and Contracts') 'Renamed plan template was not loaded'
        Complete-Scenario

        Start-Scenario 'CLI-04'
        # Without the marker, absent specs must block apply even when tasks exist.
        Set-FixtureFile "$changePath/.openspec.yaml" "schema: agentic`n"
        $apply = Invoke-OpenSpecJson @('instructions', 'apply', '--change', 'regression', '--json')
        Assert-That ($apply.state -eq 'blocked' -and $apply.missingArtifacts -contains 'specs') 'Absent required specs did not block apply'
        Complete-Scenario
        Set-FixtureFile "$changePath/.openspec.yaml" "schema: agentic`nskip_specs: true`n"

        Start-Scenario 'CLI-05'
        # Preserve the upstream limitation explicitly: checked boxes do not validate evidence.
        Set-FixtureFile "$changePath/tasks.md" "- [x] 1.1 Main E2E`n- [x] 1.2 [final-verification] Final acceptance`n"
        Set-FixtureFile "$changePath/verification.md" "## Main E2E`nFAIL: deliberate fixture failure.`n## Final Assessment`nBLOCKED`n"
        $apply = Invoke-OpenSpecJson @('instructions', 'apply', '--change', 'regression', '--json')
        Assert-That ($apply.state -eq 'all_done') 'CLI all_done contract changed; review the dedicated entrypoint'
        Assert-That (-not [string]::IsNullOrWhiteSpace($apply.instruction) -and $apply.instruction -ne $readyInstruction) 'all_done no longer replaces custom apply instruction; review README contract'
        Assert-That ($apply.instruction -notmatch 'templates/reviewer\.md') 'all_done still includes custom apply instruction; review README contract'
        Assert-That ($null -eq $apply.contextFiles.PSObject.Properties['verification']) 'CLI now exposes verification; review integration assumptions'
        Assert-That ($apply.context -match 'CLI_CONTEXT_SENTINEL') 'Project context lost in all_done'
        Assert-That (($apply.operationGuidance -join ' ') -match 'CLI_APPLY_SENTINEL' -and ($apply.operationGuidance -join ' ') -match 'agentic-verify') 'Apply guidance marker or acceptance routing lost in all_done'
        $archive = Invoke-OpenSpecJson @('instructions', 'archive', '--change', 'regression', '--json')
        Assert-That (($archive.operationGuidance -join ' ') -match 'CLI_ARCHIVE_SENTINEL' -and ($archive.operationGuidance -join ' ') -match 'agentic-verify') 'Archive guidance marker or acceptance routing missing'
        Assert-That (($archive.operationGuidance -join ' ') -notmatch 'CLI_APPLY_SENTINEL') 'Archive received apply-only guidance'
        Complete-Scenario

        Start-Scenario 'CLI-06'
        # Entering final verification must remain possible before its own checkbox is checked.
        Set-FixtureFile "$changePath/tasks.md" "- [x] 1.1 Main E2E`n- [ ] 1.2 [final-verification] Final acceptance`n"
        $apply = Invoke-OpenSpecJson @('instructions', 'apply', '--change', 'regression', '--json')
        Assert-That ($apply.state -eq 'ready' -and $apply.progress.remaining -eq 1) 'Final task could not remain pending'
        Complete-Scenario
        Start-Scenario 'CLI-07'
        # Test design is apply work, not an additional planning artifact required before coding.
        Set-FixtureFile "$changePath/tasks.md" "- [ ] 1.1 WP1 Implement feature`n- [ ] 2.1 TP1 Design E2E cases`n- [ ] 2.2 TP2 Design E2E cases`n- [ ] 3.1 [final-verification] Final acceptance`n"
        $apply = Invoke-OpenSpecJson @('instructions', 'apply', '--change', 'regression', '--json')
        Assert-That ($apply.state -eq 'ready' -and $apply.progress.remaining -eq 4) 'Pending test design unexpectedly blocked coding entry'
        $status = Invoke-OpenSpecJson @('status', '--change', 'regression', '--json')
        Assert-That (@($status.artifacts | Where-Object { $_.id -match 'test|e2e' }).Count -eq 0) 'Test design became a pre-apply artifact'
        $resolvedSchema = Invoke-OpenSpecJson @('schema', 'which', 'agentic', '--json')
        Assert-That (Test-Path -LiteralPath (Join-Path $resolvedSchema.path 'roles/tester.md')) 'Test-agent instructions are absent from the resolved schema'
        Assert-That (Test-Path -LiteralPath (Join-Path $resolvedSchema.path 'roles/integrator.md')) 'Integration-agent instructions are absent from the resolved schema'
        Assert-That ($apply.instruction -match 'roles/integrator\.md') 'Apply did not deliver the integrator entrypoint'
        Assert-That (Test-Path -LiteralPath (Join-Path $resolvedSchema.path 'roles/environment.md')) 'Environment-agent instructions are absent from the resolved schema'
        Assert-That ($apply.instruction -match 'roles/environment\.md' -and $apply.instruction -match 'recon phase' -and $apply.instruction -match 'fork_turns') 'Apply did not preserve the isolated environment handoff contract'
        Assert-That (Test-Path -LiteralPath (Join-Path $resolvedSchema.path 'roles/reviewer.md')) 'Reviewer instructions are absent from the resolved schema'
        Assert-That (Test-Path -LiteralPath (Join-Path $resolvedSchema.path 'procedures/acceptance.md')) 'Final verification procedure is absent from the resolved schema'
        Assert-That ($apply.instruction -match 'roles/reviewer\.md' -and $apply.instruction -match 'roles/tester\.md' -and $apply.instruction -match 'procedures/acceptance\.md') 'Apply did not deliver the separated role and procedure paths'
        foreach ($role in @('coder', 'validator')) {
            Assert-That (Test-Path -LiteralPath (Join-Path $resolvedSchema.path "roles/$role.md")) "Role instructions are absent from the resolved schema: $role"
            Assert-That ($apply.instruction.Contains("roles/$role.md")) "Apply did not deliver role instructions: $role"
        }
        # Apply 指令的关键义务标记：只断言稳定标识符（隔离参数名、状态词、证据文件名），不锁措辞。
        # 合法改写不应误报，但整段调度义务被删除时必须失败。
        foreach ($token in @('fork_turns', 'BLOCKED', 'verification.md')) {
            Assert-That ($apply.instruction.Contains($token)) "Apply instruction lost a dispatch obligation marker: $token"
        }
        Complete-Scenario
        Start-Scenario 'CLI-08'
        # Specs and design are parallel siblings; plan needs both, not detailed test cases.
        $convergencePath = 'openspec/changes/convergence'
        Set-FixtureFile "$convergencePath/.openspec.yaml" "schema: agentic`n"
        Set-FixtureFile "$convergencePath/proposal.md" "## Why`nConvergence regression fixture.`n"
        $status = Invoke-OpenSpecJson @('status', '--change', 'convergence', '--json')
        foreach ($sibling in @('specs', 'design')) {
            Assert-That (($status.artifacts | Where-Object id -eq $sibling).status -eq 'ready') "$sibling cannot start in parallel"
        }
        Assert-That (($status.artifacts | Where-Object id -eq 'plan').status -eq 'blocked') 'Plan unlocked without contracts'
        Set-FixtureFile "$convergencePath/design.md" "## Decisions`nInterface contract.`n"
        $status = Invoke-OpenSpecJson @('status', '--change', 'convergence', '--json')
        Assert-That (($status.artifacts | Where-Object id -eq 'plan').status -eq 'blocked') 'Design alone unlocked plan'
        Set-FixtureFile "$convergencePath/specs/example/spec.md" "## ADDED Requirements`n### Requirement: Example`nSystem SHALL respond.`n#### Scenario: Response`n- **WHEN** invoked`n- **THEN** responds`n"
        $status = Invoke-OpenSpecJson @('status', '--change', 'convergence', '--json')
        Assert-That (($status.artifacts | Where-Object id -eq 'plan').status -eq 'ready') 'Both contracts did not unlock plan'
        Assert-That (($status.artifacts | Where-Object id -eq 'tasks').status -eq 'blocked') 'Tasks unlocked without plan'
        Set-FixtureFile "$convergencePath/plan.md" "## Work Packages`nFixture plan.`n"
        $status = Invoke-OpenSpecJson @('status', '--change', 'convergence', '--json')
        Assert-That (($status.artifacts | Where-Object id -eq 'tasks').status -eq 'ready') 'Plan artifact did not unlock tasks'
        Complete-Scenario

        Start-Scenario 'CLI-09'
        $specsOnlyPath = 'openspec/changes/specs-only'
        Set-FixtureFile "$specsOnlyPath/.openspec.yaml" "schema: agentic`n"
        Set-FixtureFile "$specsOnlyPath/proposal.md" "## Why`nSymmetric dependency fixture.`n"
        Set-FixtureFile "$specsOnlyPath/specs/example/spec.md" "## ADDED Requirements`n### Requirement: Example`nSystem SHALL respond.`n#### Scenario: Response`n- **WHEN** invoked`n- **THEN** responds`n"
        $status = Invoke-OpenSpecJson @('status', '--change', 'specs-only', '--json')
        Assert-That (($status.artifacts | Where-Object id -eq 'plan').status -eq 'blocked') 'Specs alone unlocked plan'
        Assert-That (($status.artifacts | Where-Object id -eq 'tasks').status -eq 'blocked') 'Specs alone unlocked tasks'
        Complete-Scenario

        # 角色模型是宿主无关的 agentic 扩展；每次解析都直接读取项目配置。
        Set-Location -LiteralPath $fixtureRoot
        $null = & node (Join-Path $projectRoot 'bin/openspec-agentic.mjs') roles set coder fixture/coder
        $null = & node (Join-Path $projectRoot 'bin/openspec-agentic.mjs') roles set reviewer fixture/reviewer
        Start-Scenario 'CLI-10'
        Assert-That ($LASTEXITCODE -eq 0) 'Role model config write failed'
        $roles = Invoke-OpenSpecAgenticJson @('roles', '--json')
        Assert-That (@($roles.roles).Count -eq 7) 'Role registry size changed'
        $tester = @($roles.roles | Where-Object { $_.id -eq 'tester' })[0]
        Assert-That ($tester.model -eq '@current' -and $tester.inheritCurrent) 'Default role did not inherit the current session model'
        $reviewer = @($roles.roles | Where-Object { $_.id -eq 'reviewer' })[0]
        Assert-That ($reviewer.model -eq 'fixture/reviewer') 'Object model form was not resolved'
        Assert-That (@($roles.errors).Count -eq 0) 'Valid roles config reported errors'
        # 角色模型不是新的规划门槛：instructions 仍是合法 JSON 且保留原有调度义务。
        $apply = Invoke-OpenSpecJson @('instructions', 'apply', '--change', 'regression', '--json')
        Assert-That ($apply.instruction -match 'openspec-agentic roles') 'Apply instruction lost the role-model dispatch obligation'
        Assert-That ($apply.instruction.Contains('fork_turns')) 'Role-model paragraph displaced the isolation obligation'
        # 修改配置后下一次解析必须直接得到新值，不生成宿主文件。
        Set-Location -LiteralPath $fixtureRoot
        $null = & node (Join-Path $projectRoot 'bin/openspec-agentic.mjs') roles set reviewer fixture/reviewer-v2
        Assert-That ($LASTEXITCODE -eq 0) 'Updated role model config write failed'
        $updated = Invoke-OpenSpecAgenticJson @('roles', '--json')
        Assert-That ((@($updated.roles | Where-Object { $_.id -eq 'reviewer' })[0]).model -eq 'fixture/reviewer-v2') 'Role config change was cached instead of reread'
        Complete-Scenario
        Start-Scenario 'CLI-11'
        # 项目级 E2E 开关：从磁盘实时读取，缺省即开启；判据正文在 schema 与 acceptance。
        $e2eDefault = Invoke-OpenSpecAgenticJson @('e2e', '--json')
        Assert-That ($e2eDefault.enabled -eq $true) 'E2E switch is not enabled by default'
        Assert-That ($e2eDefault.command -eq 'node --version') 'Fixture E2E command was not read from config'
        Assert-That (@($e2eDefault.errors).Count -eq 0) 'Valid E2E switch config reported errors'
        $e2eConfigPath = Join-Path $fixtureRoot 'openspec/config.yaml'
        $e2eEnabledText = [IO.File]::ReadAllText($e2eConfigPath, $utf8)
        $e2eDisabledText = $e2eEnabledText -replace '(?m)^    enabled: true', '    enabled: false'
        Assert-That ($e2eDisabledText -ne $e2eEnabledText) 'Fixture config lost the seeded e2e switch'
        [IO.File]::WriteAllText($e2eConfigPath, $e2eDisabledText, $utf8)
        $e2eOff = Invoke-OpenSpecAgenticJson @('e2e', '--json')
        Assert-That ($e2eOff.enabled -eq $false) 'E2E switch change was cached instead of reread'
        $apply = Invoke-OpenSpecJson @('instructions', 'apply', '--change', 'regression', '--json')
        Assert-That ($apply.instruction -match 'openspec-agentic e2e' -and $apply.instruction -match 'downgrade_approval') 'Apply instruction lost the E2E switch obligation'
        $planFixture = Invoke-OpenSpecJson @('instructions', 'plan', '--change', 'regression', '--json')
        Assert-That ($planFixture.template -match 'downgrade_approval') 'Plan template lost the downgrade approval field'
        [IO.File]::WriteAllText($e2eConfigPath, $e2eEnabledText, $utf8)
        Complete-Scenario
        Start-Scenario 'CLI-12'
        # 一致性检查：开关开启时，已完成变更写 not-applicable 且无批准记录必须 FAIL，不得放行归档。
        Set-FixtureFile "$changePath/tasks.md" "- [x] 1.1 已完成`n"
        $planNoApproval = @'
## Verification Strategy

### Main E2E

```yaml
mode: not-applicable
reason: "夹具理由"
basis: "夹具依据"
alternative_checks: ["人工核对"]
```
'@
        Set-FixtureFile "$changePath/plan.md" $planNoApproval
        $rawCheck = & node (Join-Path $projectRoot 'bin/openspec-agentic.mjs') e2e check --change regression --json
        Assert-That ($LASTEXITCODE -ne 0) 'E2E check passed an unapproved downgrade'
        $unapproved = ($rawCheck -join "`n") | ConvertFrom-Json
        Assert-That ($unapproved.result -eq 'FAIL' -and $unapproved.enabled -eq $true) 'E2E check verdict changed'
        Assert-That ($unapproved.changes[0].reason -match 'downgrade_approval') 'E2E check reason lost the approval requirement'
        $approvedPlan = $planNoApproval -replace 'alternative_checks: \["人工核对"\]', "alternative_checks: [`"人工核对`"]`ndowngrade_approval: `"用户在夹具会话中批准`""
        Set-FixtureFile "$changePath/plan.md" $approvedPlan
        $approved = Invoke-OpenSpecAgenticJson @('e2e', 'check', '--change', 'regression', '--json')
        Assert-That ($approved.result -eq 'PASS' -and $approved.changes[0].mode -eq 'not-applicable') 'Recorded approval did not clear the check'
        Complete-Scenario
        Start-Scenario 'CLI-13'
        # apply 内的 E2E 任务：先确认指令把执行与检查写成义务，再验证完成条件。
        Set-FixtureFile "$changePath/tasks.md" "- [x] 1.1 已完成`n- [ ] 1.2 [final-verification] 最终验收`n"
        $planRequired = @'
## Verification Strategy

### Main E2E

```yaml
mode: required
```
'@
        Set-FixtureFile "$changePath/plan.md" $planRequired
        $applyDuringWork = Invoke-OpenSpecJson @('instructions', 'apply', '--change', 'regression', '--json')
        Assert-That ($applyDuringWork.instruction -match 'openspec-agentic e2e run' -and $applyDuringWork.instruction -match 'openspec-agentic e2e check') 'apply 指令未把执行与检查写成 E2E 任务义务'
        # 任务全勾后：未跑过 E2E 时检查失败；用 e2e run 真跑并留证后才通过。
        Set-FixtureFile "$changePath/tasks.md" "- [x] 1.1 已完成`n"
        $beforeRun = & node (Join-Path $projectRoot 'bin/openspec-agentic.mjs') e2e check --change regression --json
        Assert-That ($LASTEXITCODE -ne 0) 'required 变更在未执行 E2E 时通过了检查'
        $beforeVerdict = ($beforeRun -join "`n") | ConvertFrom-Json
        Assert-That ($beforeVerdict.changes[0].reason -match '执行记录') '未指出缺少执行记录'
        $runOutput = & node (Join-Path $projectRoot 'bin/openspec-agentic.mjs') e2e run --change regression --stage final
        Assert-That ($LASTEXITCODE -eq 0) "E2E run 未成功：$($runOutput -join ' ')"
        Assert-That (($runOutput -join ' ') -match 'E2E run: PASS') 'E2E run 未报告 PASS'
        $records = Get-ChildItem -LiteralPath (Join-Path $fixtureRoot 'openspec/changes/regression/e2e') -Filter 'run-*.json'
        Assert-That ($records.Count -ge 1) '执行记录未写入变更目录'
        $record = ($records | Sort-Object Name | Select-Object -Last 1 | Get-Content -Raw -Encoding UTF8) | ConvertFrom-Json
        Assert-That ($record.kind -eq 'automated' -and $record.exitCode -eq 0 -and $record.command -match 'node') '执行记录内容不完整'
        $afterRun = Invoke-OpenSpecAgenticJson @('e2e', 'check', '--change', 'regression', '--json')
        Assert-That ($afterRun.result -eq 'PASS' -and $afterRun.changes[0].reason -match '自动执行记录') '执行并留证后检查仍未通过'
        Complete-Scenario
        Start-Scenario 'CLI-14'
        # 行级单一所有者：判 PASS 时自动勾选带 [e2e-owned] 标记的最终 E2E 任务行（CLI-13 已留下成功记录）。
        # 真实模板形状：最终 E2E 行由扩展拥有，最终验收行（[final-verification]）在验收期间必须待办。
        Set-FixtureFile "$changePath/tasks.md" "- [x] 1.1 已完成`n- [ ] 7.1 [e2e-owned] 最终 E2E`n- [ ] 8.1 [final-verification] 最终验收`n"
        $owned = Invoke-OpenSpecAgenticJson @('e2e', 'check', '--change', 'regression', '--json')
        Assert-That ($owned.result -eq 'PASS' -and $owned.changes[0].marked -eq $true) 'PASS 时未回写机器拥有的 E2E 任务行'
        $ownedTasks = [IO.File]::ReadAllText((Join-Path $fixtureRoot "$changePath/tasks.md"), $utf8)
        Assert-That ($ownedTasks -match '\[x\] 7\.1 \[e2e-owned\]') 'tasks.md 的 [e2e-owned] 行未被勾选'
        Assert-That ($ownedTasks -match '\[ \] 8\.1 \[final-verification\]') '最终验收行不应被回写，应保持待办'
        $ownedAgain = Invoke-OpenSpecAgenticJson @('e2e', 'check', '--change', 'regression', '--json')
        Assert-That ($ownedAgain.result -eq 'PASS' -and $ownedAgain.changes[0].marked -eq $false) '重复检查不应再次回写'
        Complete-Scenario
        Start-Scenario 'CLI-15'
        # 结构回归只验证共用契约被投递且验收入口明确消费；拒收/重开判断由 BEH-116..119 验证。
        Assert-That ($readyInstruction.Contains('roles/handoff.md') -and $readyInstruction.Contains('handoff_index')) 'Apply lost the shared handoff contract'
        $handoff = Get-Content -LiteralPath (Join-Path $resolvedSchema.path 'roles/handoff.md') -Raw -Encoding UTF8
        foreach ($field in @('task_id', 'role', 'phase', 'stage', 'target_revision', 'evidence_type', 'evidence_id', 'report_path', 'result', 'evidence_status', 'applicability_basis', 'source_evidence')) {
            Assert-That ($handoff.Contains($field)) "Shared handoff contract lost field: $field"
        }
        foreach ($state in @('NEW', 'REUSED', 'INVALID', 'PENDING')) {
            Assert-That ($handoff.Contains($state)) "Shared handoff contract lost evidence state: $state"
        }
        foreach ($role in @('coder', 'tester', 'reviewer', 'integrator', 'validator', 'environment')) {
            $instructions = Get-Content -LiteralPath (Join-Path $resolvedSchema.path "roles/$role.md") -Raw -Encoding UTF8
            Assert-That ($instructions.Contains('roles/handoff.md') -and $instructions.Contains('handoff_index')) "Role lost shared index instruction: $role"
        }
        $acceptance = Get-Content -LiteralPath (Join-Path $resolvedSchema.path 'procedures/acceptance.md') -Raw -Encoding UTF8
        $verificationTemplate = Get-Content -LiteralPath (Join-Path $resolvedSchema.path 'templates/verification.md') -Raw -Encoding UTF8
        Assert-That ($acceptance.Contains('Handoff Traceability') -and $acceptance.Contains('重开受影响的任务')) 'Acceptance lost handoff audit or task reopening'
        Assert-That ($verificationTemplate.Contains('## Handoff Index')) 'Verification template lost the indexed evidence section'
        Complete-Scenario
        Write-Output "PASS: $passed CLI regression scenarios. Agent evidence decisions require separate behavioral validation."
    }
    finally { Pop-Location }
}
finally {
    # 只清理本次自建、且直接位于临时父目录下、名称匹配的 fixture。
    if ($ownsFixture -and (Test-Path -LiteralPath $fixtureRoot)) {
        $resolved = (Resolve-Path -LiteralPath $fixtureRoot).Path
        if ((Split-Path $resolved -Parent) -ne $tempParent -or (Split-Path $resolved -Leaf) -ne $fixtureName) {
            throw "Refusing cleanup outside the owned fixture directory: $resolved"
        }
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
