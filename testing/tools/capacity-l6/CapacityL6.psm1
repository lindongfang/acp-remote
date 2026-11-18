#requires -Version 7.4
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:CaseNames = [ordered]@{
    cp011 = 'node::tests::r3_source_wss_development::capacity_l6_cp011_real_fifty_and_thirty'
    cp023 = 'node::tests::r3_source_wss_development::capacity_l6_cp023_real_unopened_root_children'
    cp026 = 'node::tests::r3_source_wss_development::capacity_l6_cp026_real_replay_clock_and_pressure'
    cp032 = 'node::tests::r3_source_wss_development::capacity_l6_cp032_real_known_work_and_finish_fault'
}
$script:MatrixSha256 = '58fc0ded8e36e33a44e32a37afe8bdb24aa5f2aed601fd2a0094dd2ab878ba02'

function Get-CapacityL6Cases {
    $copy = [ordered]@{}
    foreach ($entry in $script:CaseNames.GetEnumerator()) { $copy[$entry.Key] = $entry.Value }
    return $copy
}

function Assert-Fields($Value, [string[]]$Required, [string[]]$Optional, [string]$Label) {
    if ($Value -isnot [System.Collections.IDictionary]) { throw "$Label must be an object" }
    foreach ($key in $Required) {
        if (-not $Value.Contains($key)) { throw "$Label is missing a required field" }
    }
    foreach ($key in $Value.Keys) {
        if ($key -cnotin ($Required + $Optional)) { throw "$Label has an unsupported field" }
    }
}

function Read-StrictJson([string]$Path, [string]$Label) {
    try {
        $text = [IO.File]::ReadAllText($Path)
        $document = [System.Text.Json.JsonDocument]::Parse($text)
        try { Assert-UniqueJson $document.RootElement } finally { $document.Dispose() }
        return ConvertFrom-Json -InputObject $text -AsHashtable -Depth 64
    } catch { throw "$Label is not valid JSON with unique fields" }
}

function Assert-UniqueJson([System.Text.Json.JsonElement]$Element) {
    if ($Element.ValueKind -eq [System.Text.Json.JsonValueKind]::Object) {
        $names = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        foreach ($field in $Element.EnumerateObject()) {
            if (-not $names.Add($field.Name)) { throw 'duplicate field' }
            Assert-UniqueJson $field.Value
        }
    } elseif ($Element.ValueKind -eq [System.Text.Json.JsonValueKind]::Array) {
        foreach ($item in $Element.EnumerateArray()) { Assert-UniqueJson $item }
    }
}

function Get-AbsolutePath($Value, [string]$Label, [ValidateSet('File','Directory','New')][string]$Kind, [switch]$AllowReparse) {
    if ($Value -isnot [string] -or [string]::IsNullOrWhiteSpace($Value) -or
        -not [IO.Path]::IsPathFullyQualified($Value)) { throw "$Label must be an explicit absolute path" }
    $path = [IO.Path]::GetFullPath($Value).TrimEnd('\', '/')
    if ($path -eq [IO.Path]::GetPathRoot($path).TrimEnd('\', '/')) { throw "$Label cannot be a volume root" }
    if ($Kind -eq 'File' -and -not [IO.File]::Exists($path)) { throw "$Label file does not exist" }
    if ($Kind -eq 'Directory' -and -not [IO.Directory]::Exists($path)) { throw "$Label directory does not exist" }
    if ($Kind -eq 'New' -and (Test-Path -LiteralPath $path)) { throw "$Label must be a new directory" }
    # Reject junction/symlink aliases without traversing or reading directory contents.
    $ancestor = $path
    while ($ancestor -and -not $AllowReparse) {
        if (Test-Path -LiteralPath $ancestor) {
            $item = Get-Item -LiteralPath $ancestor -Force
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "$Label cannot traverse a reparse point"
            }
        }
        $ancestor = [IO.Path]::GetDirectoryName($ancestor)
    }
    return $path
}

function Test-Within([string]$Path, [string]$Parent) {
    return $Path.Equals($Parent, [StringComparison]::OrdinalIgnoreCase) -or
        $Path.StartsWith($Parent.TrimEnd('\','/') + [IO.Path]::DirectorySeparatorChar,
            [StringComparison]::OrdinalIgnoreCase)
}

function Get-Sha256([string]$Path) {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-CapacityL6Matrix {
    $path = Join-Path $PSScriptRoot 'branch-matrix.v1.json'
    $bytes = [Text.Encoding]::UTF8.GetBytes([IO.File]::ReadAllText($path).Replace("`r`n", "`n"))
    $hash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
    if ($hash -cne $script:MatrixSha256) { throw 'branch matrix does not match the reviewed version' }
    return @{ Sha256=$hash; Document=(Read-StrictJson $path 'branch matrix'); Path=$path }
}

function Decode-Units($Value) {
    if ($Value -isnot [string]) { throw 'profile encoded field must be text' }
    try {
        $bytes = [Convert]::FromBase64String($Value)
        if ($bytes.Length % 2 -ne 0 -or [Convert]::ToBase64String($bytes) -cne $Value) {
            throw 'noncanonical encoding'
        }
        $encoding = [Text.UnicodeEncoding]::new($false, $false, $true)
        $decoded = $encoding.GetString($bytes)
        if ($decoded.Contains([char]0)) { throw 'NUL' }
        return $decoded
    } catch { throw 'profile has invalid UTF16LE base64' }
}

function Assert-Constant($Value, [long]$Expected, [string]$Label) {
    if (($Value -isnot [long] -and $Value -isnot [int]) -or $Value -ne $Expected) {
        throw "$Label must equal the fixed acceptance value"
    }
}

function Test-CapacityL6Configuration {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Config,
        [Parameter(Mandatory)][string]$RepositoryRoot
    )
    if (-not $IsWindows) { throw 'this runner requires Windows' }
    $repo = Get-AbsolutePath $RepositoryRoot 'repositoryRoot' Directory
    $matrix = Get-CapacityL6Matrix
    if (-not [IO.File]::Exists([IO.Path]::Combine($repo, 'app', 'Cargo.toml'))) {
        throw 'repositoryRoot does not identify this project'
    }
    $configPath = Get-AbsolutePath $Config 'config' File
    $value = Read-StrictJson $configPath 'config'
    Assert-Fields $value @('format','runRoot','launchProfile','dedicatedHome','dedicatedCodexHome',
        'scenarioFiles','defaultLimit','idleSeconds') @('allowModelRequests','webControl') 'config'
    Assert-Constant $value.format 1 'format'
    Assert-Constant $value.defaultLimit 50 'defaultLimit'
    Assert-Constant $value.idleSeconds 1800 'idleSeconds'
    if ($value.Contains('allowModelRequests') -and $value.allowModelRequests -isnot [bool]) {
        throw 'allowModelRequests must be boolean'
    }
    $allowModel = $value.Contains('allowModelRequests') -and $value.allowModelRequests
    $runRoot = Get-AbsolutePath $value.runRoot 'runRoot' New
    $testerRoot = [IO.Path]::Combine($repo, '.tmp', 'capacity-request-tester')
    if (-not (Test-Within $runRoot $testerRoot) -or $runRoot -eq $testerRoot) {
        throw 'runRoot must be a new child of the dedicated tester directory'
    }
    $dedicatedDirectory = Get-AbsolutePath $value.dedicatedHome 'dedicatedHome' Directory
    $codexDirectory = Get-AbsolutePath $value.dedicatedCodexHome 'dedicatedCodexHome' Directory
    foreach ($directory in @($dedicatedDirectory, $codexDirectory)) {
        if ((Test-Within $directory $runRoot) -or (Test-Within $runRoot $directory)) {
            throw 'authenticated directories and disposable runRoot must not overlap'
        }
        # Only compare environment strings. Never inspect default authentication files.
        foreach ($default in @($env:HOME, $env:USERPROFILE, $env:CODEX_HOME,
                $(if ($env:USERPROFILE) { [IO.Path]::Combine($env:USERPROFILE, '.codex') }))) {
            if ($default -and [IO.Path]::IsPathFullyQualified($default) -and
                [IO.Path]::GetFullPath($default).TrimEnd('\','/') -ieq $directory) {
                throw 'dedicated authentication directories cannot use a default location'
            }
        }
    }
    $profilePath = Get-AbsolutePath $value.launchProfile 'launchProfile' File
    if ((Test-Within $configPath $runRoot) -or (Test-Within $profilePath $runRoot)) {
        throw 'input documents cannot be inside disposable runRoot'
    }
    $profile = Read-StrictJson $profilePath 'launchProfile'
    Assert-Fields $profile @('format','launch','recipes') @() 'launchProfile'
    Assert-Constant $profile.format 1 'launchProfile.format'
    Assert-Fields $profile.launch @('executable_utf16le_base64','arguments_utf16le_base64','environment') @() 'launch'
    $program = Get-AbsolutePath (Decode-Units $profile.launch.executable_utf16le_base64) 'launch executable' File -AllowReparse
    if ([IO.Path]::GetFileName($program) -ine 'node.exe') { throw 'launch executable must be an explicitly installed node.exe' }
    if ($profile.launch.arguments_utf16le_base64 -isnot [array] -or
        $profile.launch.arguments_utf16le_base64.Count -ne 1) {
        throw 'this L6 runner requires one absolute installed adapter entrypoint argument'
    }
    $adapter = Get-AbsolutePath (Decode-Units $profile.launch.arguments_utf16le_base64[0]) 'adapter entrypoint' File -AllowReparse
    if ([IO.Path]::GetExtension($adapter) -inotmatch '^\.(js|cjs|mjs)$') {
        throw 'adapter entrypoint must be an installed JavaScript file'
    }
    if ($profile.launch.environment -isnot [array]) { throw 'profile environment must be an array' }
    $environment = [Collections.Generic.Dictionary[string,string]]::new([StringComparer]::OrdinalIgnoreCase)
    $allowed = @('HOME','USERPROFILE','CODEX_HOME','APPDATA','LOCALAPPDATA','TEMP','TMP',
        'CODEX_PATH','PATH','SystemRoot','WINDIR','ComSpec','PATHEXT',
        'INITIAL_AGENT_MODE','NO_BROWSER')
    foreach ($entry in $profile.launch.environment) {
        Assert-Fields $entry @('name_utf16le_base64','value_utf16le_base64') @() 'environment entry'
        $name = Decode-Units $entry.name_utf16le_base64
        if ($name -inotmatch '^[A-Z_]+$') {
            throw 'profile environment name is invalid'
        }
        if ($name -inotin $allowed -or $environment.ContainsKey($name)) {
            throw 'profile environment is not allowed or is duplicated'
        }
        $environment.Add($name, (Decode-Units $entry.value_utf16le_base64))
    }
    foreach ($name in @('HOME','USERPROFILE','CODEX_HOME','APPDATA','LOCALAPPDATA','TEMP','TMP')) {
        if (-not $environment.ContainsKey($name)) { throw 'profile must explicitly bind every dedicated environment directory' }
        $directory = Get-AbsolutePath $environment[$name] 'profile dedicated directory' Directory
        $expected = if ($name -eq 'CODEX_HOME') { $codexDirectory } else { $dedicatedDirectory }
        if ($name -in @('HOME','USERPROFILE','CODEX_HOME')) {
            if ($directory -ine $expected) { throw 'profile authentication directory does not match the explicit configuration' }
        } elseif (-not (Test-Within $directory $expected) -or $directory -ieq $expected) {
            throw 'profile temporary/application directories must be dedicated children of dedicatedHome'
        }
    }
    foreach ($name in @('SystemRoot','WINDIR','ComSpec','PATHEXT')) {
        if ($environment.ContainsKey($name) -and $environment[$name] -ine [Environment]::GetEnvironmentVariable($name)) {
            throw 'profile cannot replace the necessary operating-system environment'
        }
    }
    if ($environment.ContainsKey('INITIAL_AGENT_MODE') -and $environment['INITIAL_AGENT_MODE'] -cnotin @('read-only','workspace-write')) {
        throw 'profile INITIAL_AGENT_MODE must be read-only or workspace-write'
    }
    if ($environment.ContainsKey('NO_BROWSER') -and $environment['NO_BROWSER'] -cne '1') {
        throw 'profile NO_BROWSER must be 1'
    }
    $programs = @(
        [ordered]@{ role='node'; sha256=(Get-Sha256 $program); fileVersion=([Diagnostics.FileVersionInfo]::GetVersionInfo($program).FileVersion) },
        [ordered]@{ role='adapter-entrypoint'; sha256=(Get-Sha256 $adapter) }
    )
    if ($environment.ContainsKey('CODEX_PATH')) {
        $codexProgram = Get-AbsolutePath $environment['CODEX_PATH'] 'CODEX_PATH' File -AllowReparse
        $programs += [ordered]@{ role='codex'; sha256=(Get-Sha256 $codexProgram); fileVersion=([Diagnostics.FileVersionInfo]::GetVersionInfo($codexProgram).FileVersion) }
    }
    if ($profile.recipes -isnot [array] -or $profile.recipes.Count -eq 0) { throw 'profile recipes must be a nonempty array' }
    $wires = [Collections.Generic.HashSet[long]]::new()
    foreach ($recipe in $profile.recipes) {
        Assert-Fields $recipe @('wire','mcp_servers','additional_directories','extra') @() 'recipe'
        if (($recipe.wire -isnot [int] -and $recipe.wire -isnot [long]) -or
            $recipe.wire -notin @(1,2) -or -not $wires.Add($recipe.wire) -or $recipe.mcp_servers -isnot [array] -or
            $recipe.additional_directories -isnot [array] -or
            $recipe.extra -isnot [System.Collections.IDictionary]) { throw 'recipe shape is invalid' }
        # This runner cannot audit third-party MCP launches, credentials, or extra directories.
        if ($recipe.mcp_servers.Count -ne 0 -or $recipe.additional_directories.Count -ne 0 -or $recipe.extra.Count -ne 0) {
            throw 'this dedicated L6 profile must have empty MCP, additional directories and extra fields'
        }
    }
    Assert-Fields $value.scenarioFiles @() @($script:CaseNames.Keys) 'scenarioFiles'
    $scenarios = [ordered]@{}
    $inputs = [ordered]@{ $configPath=(Get-Sha256 $configPath); $profilePath=(Get-Sha256 $profilePath); $program=(Get-Sha256 $program); $adapter=(Get-Sha256 $adapter) }
    $inputs[$matrix.Path] = Get-Sha256 $matrix.Path
    if ($environment.ContainsKey('CODEX_PATH')) { $inputs[$codexProgram] = Get-Sha256 $codexProgram }
    foreach ($case in $script:CaseNames.Keys) {
        if ($value.scenarioFiles.Contains($case)) {
            $scenario = Get-AbsolutePath $value.scenarioFiles[$case] 'scenario file' File
            if (Test-Within $scenario $runRoot) { throw 'scenario inputs cannot be inside disposable runRoot' }
            # No prompt, approval value, or credentials are printed or copied.
            $scenarios[$case] = Get-Sha256 $scenario
            $inputs[$scenario] = $scenarios[$case]
        }
    }
    $webEnabled = $false
    if ($value.Contains('webControl')) {
        Assert-Fields $value.webControl @('enabled','timeoutSeconds') @() 'webControl'
        if ($value.webControl.enabled -isnot [bool] -or
            ($value.webControl.timeoutSeconds -isnot [long] -and $value.webControl.timeoutSeconds -isnot [int]) -or
            $value.webControl.timeoutSeconds -lt 1) { throw 'webControl requires a boolean and positive integer timeout' }
        $webEnabled = $value.webControl.enabled
    }
    return @{
        ConfigPath=$configPath; RunRoot=$runRoot; Environment=$environment; Inputs=$inputs
        Summary=[ordered]@{
            status='validated'; acceptancePassed=$false; format=1; defaultLimit=50; idleSeconds=1800
            authentication='not inspected or verified'; allowModelRequests=$allowModel
            branchMatrixSha256=$matrix.Sha256
            webControlEnabled=$webEnabled; configSha256=$inputs[$configPath]; profileSha256=$inputs[$profilePath]
            programs=$programs; scenarioHashes=$scenarios
            missingScenarioCases=@($script:CaseNames.Keys | Where-Object { -not $scenarios.Contains($_) })
        }
    }
}

function Assert-CapacityL6Inputs($Validated) {
    foreach ($entry in $Validated.Inputs.GetEnumerator()) {
        if (-not [IO.File]::Exists($entry.Key) -or (Get-Sha256 $entry.Key) -cne $entry.Value) {
            throw 'a validated input changed; refusing to continue'
        }
    }
}

function Test-CapacityL6Completion([string[]]$Requested, [object[]]$Results) {
    if ($null -eq $Requested -or $Requested.Count -eq 0 -or
        $null -eq $Results -or $Results.Count -ne $Requested.Count) { return $false }
    $expected = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($case in $Requested) {
        if ($case -cnotin @($script:CaseNames.Keys) -or -not $expected.Add($case)) { return $false }
    }
    $observed = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($result in $Results) {
        if ($result -isnot [System.Collections.IDictionary] -or -not $result.Contains('case') -or
            $result.case -isnot [string] -or -not $expected.Contains($result.case) -or
            -not $observed.Add($result.case) -or -not $result.Contains('status') -or
            $result.status -cne 'passed') { return $false }
    }
    return $expected.SetEquals($observed)
}

function Invoke-CapacityL6Process {
    param([string]$Executable, [string[]]$Arguments, $Validated, [switch]$ListOnly,
        [ValidateRange(1801,86400)][int]$CaseMaxSeconds=10800, $RunBinding)
    $defaultDirectories = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    if (-not $ListOnly) {
        foreach ($value in @($env:HOME, $env:USERPROFILE, $env:CODEX_HOME)) {
            if ($value -and [IO.Path]::IsPathFullyQualified($value)) {
                $null = $defaultDirectories.Add([IO.Path]::GetFullPath($value).TrimEnd('\','/'))
            }
        }
        if ($env:USERPROFILE -and [IO.Path]::IsPathFullyQualified($env:USERPROFILE)) {
            $null = $defaultDirectories.Add(
                [IO.Path]::GetFullPath([IO.Path]::Combine($env:USERPROFILE, '.codex')).TrimEnd('\','/'))
        }
        if ($defaultDirectories.Count -eq 0) { throw 'original default identity directory facts unavailable' }
    }
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $Executable
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.Environment.Clear()
    foreach ($name in @('SystemRoot','WINDIR','ComSpec','PATHEXT','PATH')) {
        $value = [Environment]::GetEnvironmentVariable($name)
        if ($value) { $start.Environment[$name] = $value }
    }
    foreach ($entry in $Validated.Environment.GetEnumerator()) { $start.Environment[$entry.Key] = $entry.Value }
    if (-not $ListOnly) {
        $start.Environment['ACP_CAPACITY_L6_CONFIG'] = $Validated.ConfigPath
        $start.Environment['ACP_CAPACITY_L6_DEFAULT_IDENTITY_DIRECTORIES'] =
            ConvertTo-Json -InputObject ([string[]]$defaultDirectories) -Compress
    }
    if (-not $ListOnly -and $null -ne $RunBinding) {
        Assert-CapacityL6ExecutionContext $RunBinding
        $start.Environment['ACP_CAPACITY_L6_RUN_ID'] = $RunBinding.RunId
        $start.Environment['ACP_CAPACITY_L6_EXE_SHA256'] = $RunBinding.Fingerprint.exeSha256
        $start.Environment['ACP_CAPACITY_L6_SOURCE_SHA256'] = $RunBinding.Fingerprint.sourceManifestSha256
        $start.Environment['ACP_CAPACITY_L6_SOURCE_MANIFEST'] = $RunBinding.SourceManifest
        $start.Environment['ACP_CAPACITY_L6_CONFIG_SHA256'] = $RunBinding.Fingerprint.configSha256
    }
    foreach ($argument in $Arguments) { $start.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    $started = $false
    try {
        if (-not $process.Start()) { throw 'test process did not start' }
        $started = $true
        # Drain both streams concurrently. Do not print failure output that might contain native text.
        if ($ListOnly) {
            $stdout = $process.StandardOutput.ReadToEndAsync()
        } else {
            $stdout = $process.StandardOutput.BaseStream.CopyToAsync([IO.Stream]::Null)
        }
        $stderr = $process.StandardError.BaseStream.CopyToAsync([IO.Stream]::Null)
        $budget = if ($ListOnly) { 30 } else { $CaseMaxSeconds }
        $elapsed = [Diagnostics.Stopwatch]::StartNew()
        $timedOut = $false
        while (-not $process.WaitForExit(200)) {
            if ($elapsed.Elapsed.TotalSeconds -ge $budget) {
                $timedOut = $true
                $process.Kill($true)
                if (-not $process.WaitForExit(30000)) { throw 'owned test tree did not exit within cleanup deadline' }
                break
            }
        }
        if (-not [Threading.Tasks.Task]::WhenAll([Threading.Tasks.Task[]]@($stdout,$stderr)).Wait(5000)) {
            throw 'owned output streams did not settle within cleanup deadline'
        }
        $listing = $null
        if ($ListOnly) { $listing = $stdout.GetAwaiter().GetResult() }
        else { $null = $stdout.GetAwaiter().GetResult() }
        $null = $stderr.GetAwaiter().GetResult()
        return @{ ExitCode=$process.ExitCode; TimedOut=$timedOut
            Listing=$(if ($ListOnly) { $listing } else { $null }) }
    } finally {
        if ($started -and -not $process.HasExited) {
            $process.Kill($true)
            if (-not $process.WaitForExit(30000)) { throw 'owned test tree cleanup remains unconfirmed' }
        }
        $process.Dispose()
    }
}

function Read-CapacityL6Result([string]$RunRoot, [int]$ExitCode, [string]$Case) {
    if ($Case -cnotin @($script:CaseNames.Keys)) { throw 'unknown result case' }
    $caseRoot = Join-Path $RunRoot $Case
    $path = Join-Path $caseRoot "$Case.result.json"
    if (-not [IO.File]::Exists($Path)) {
        return [ordered]@{ case=$Case; status='failed'; reason='missing structured Rust result'; exitCode=$ExitCode }
    }
    $null = Get-AbsolutePath $RunRoot 'result run root' Directory
    $null = Get-AbsolutePath $caseRoot 'result case root' Directory
    $null = Get-AbsolutePath $Path 'result file' File
    $result = Read-StrictJson $Path 'Rust result'
    if ($result -isnot [System.Collections.IDictionary] -or -not $result.Contains('status') -or
        $result.status -cnotin @('passed','blocked','failed','pending_independent_review') -or -not $result.Contains('steps') -or
        $result.steps -isnot [array]) {
        return [ordered]@{ case=$Case; status='failed'; reason='invalid structured Rust result'; exitCode=$ExitCode }
    }
    $pendingReview = $result.status -ceq 'pending_independent_review'
    if (($result.status -ceq 'passed' -or $pendingReview) -ne ($ExitCode -eq 0)) {
        return [ordered]@{ case=$Case; status='failed'; reason='Rust status and process exit disagree'; exitCode=$ExitCode }
    }
    if ($result.Contains('missingBranches') -and
        ($result.missingBranches -isnot [array] -or
            ($result.status -ceq 'passed' -and $result.missingBranches.Count -ne 0))) {
        return [ordered]@{ case=$Case; status='failed'; reason='incomplete branches cannot pass'; exitCode=$ExitCode }
    }
    if ($pendingReview) {
        Assert-Constant $result.format 2 'execution format'
        foreach ($field in @('missingBranches', 'missingImplementationBranches')) {
            if ($result[$field] -isnot [array] -or $result[$field].Count -ne 0) {
                throw 'pending review execution still declares missing branches'
            }
        }
        if ($result.kind -cne 'capacity_l6_execution' -or $result.case -cne $Case -or
            $result.executionComplete -isnot [bool] -or -not $result.executionComplete -or
            $result.entryImplementationComplete -isnot [bool] -or -not $result.entryImplementationComplete -or
            $result.fullCpPassed -isnot [bool] -or $result.fullCpPassed -or
            $result.pendingReasons -isnot [array] -or $result.pendingReasons.Count -ne 1 -or
            $result.pendingReasons[0] -cne 'independent_review') {
            throw 'invalid pending review execution'
        }
    }
    if ($result.status -ceq 'passed') {
        foreach ($field in @('format','case','defaultLimit','idleSeconds','realHarness','fullCpPassed',
                'cleanupJoined','missingBranches','requiredBranches','branchMatrixSha256',
                'entryImplementationComplete','jobActiveProcesses','portClosed')) {
            if (-not $result.Contains($field)) {
                return [ordered]@{ case=$Case; status='failed'; reason='pass evidence contract is incomplete'; exitCode=$ExitCode }
            }
        }
        Assert-Constant $result.format 1 'result format'
        Assert-Constant $result.defaultLimit 50 'result capacity'
        Assert-Constant $result.idleSeconds 1800 'result idle interval'
        $matrix = Get-CapacityL6Matrix
        Assert-Constant $result.jobActiveProcesses 0 'result owned Job active process count'
        if ($result.case -cne $Case -or $result.realHarness -isnot [bool] -or -not $result.realHarness -or
            $result.fullCpPassed -isnot [bool] -or -not $result.fullCpPassed -or
            $result.cleanupJoined -isnot [bool] -or -not $result.cleanupJoined -or
            $result.portClosed -isnot [bool] -or -not $result.portClosed -or
            $result.entryImplementationComplete -isnot [bool] -or -not $result.entryImplementationComplete -or
            $result.branchMatrixSha256 -cne $matrix.Sha256 -or
            $result.requiredBranches -isnot [array] -or $result.requiredBranches.Count -eq 0) {
            return [ordered]@{ case=$Case; status='failed'; reason='case, cleanup, or full-branch evidence is unconfirmed'; exitCode=$ExitCode }
        }
        $branchIds = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
        foreach ($branch in $result.requiredBranches) {
            if ($branch -isnot [System.Collections.IDictionary] -or -not $branch.Contains('id') -or
                $branch.id -isnot [string] -or [string]::IsNullOrWhiteSpace($branch.id) -or
                -not $branchIds.Add($branch.id) -or -not $branch.Contains('status') -or $branch.status -cne 'passed') {
                return [ordered]@{ case=$Case; status='failed'; reason='required branch is missing or unconfirmed'; exitCode=$ExitCode }
            }
        }
        $expected = [Collections.Generic.HashSet[string]]::new(
            [string[]]$matrix.Document.cases[$Case], [StringComparer]::Ordinal)
        if (-not $expected.SetEquals($branchIds)) {
            return [ordered]@{ case=$Case; status='failed'; reason='required branches do not match the reviewed matrix'; exitCode=$ExitCode }
        }
        return [ordered]@{ case=$Case; status='blocked'; exitCode=$ExitCode
            reason='legacy result requires a new format2 execution and independent review'
            resultSha256=(Get-Sha256 $Path) }
    }
    # Leave Rust's body-free evidence in its original directory; emit only fixed safe fields.
    return [ordered]@{ case=$Case; status=$result.status; exitCode=$ExitCode; resultSha256=(Get-Sha256 $Path) }
}

function New-CapacityL6ExecutionContext {
    param([string]$SourceManifest, [string]$RepositoryRoot, [string]$TestExecutableSha256, $Validated)
    $source = Get-AbsolutePath $SourceManifest 'build source manifest' File
    $repository = Get-AbsolutePath $RepositoryRoot 'repository' Directory
    if (-not (Test-Within $source $repository)) { throw 'build source manifest must belong to repository' }
    $document = Read-StrictJson $source 'build source manifest'
    if ($document.testExecutableSha256 -cne $TestExecutableSha256 -or
        $document.files -isnot [array] -or $document.files.Count -eq 0) { throw 'build source manifest does not bind executable' }
    $paths = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($file in $document.files) {
        if ($file.path -isnot [string] -or [string]::IsNullOrWhiteSpace($file.path) -or
            -not $paths.Add($file.path) -or $file.sha256 -isnot [string] -or
            $file.sha256 -cnotmatch '^[0-9a-f]{64}$') { throw 'invalid build source file list' }
    }
    return @{
        RunId = [Guid]::NewGuid().ToString('N')
        SourceManifest = $source
        ConfigPath = $Validated.ConfigPath
        Fingerprint = [ordered]@{
            exeSha256 = $TestExecutableSha256
            sourceManifestSha256 = (Get-Sha256 $source)
            configSha256 = (Get-Sha256 $Validated.ConfigPath)
            branchMatrixSha256 = (Get-CapacityL6Matrix).Sha256
        }
    }
}

function Assert-CapacityL6ExecutionContext($Context) {
    if ((Get-Sha256 $Context.SourceManifest) -cne $Context.Fingerprint.sourceManifestSha256 -or
        (Get-Sha256 $Context.ConfigPath) -cne $Context.Fingerprint.configSha256 -or
        (Get-CapacityL6Matrix).Sha256 -cne $Context.Fingerprint.branchMatrixSha256) {
        throw 'execution fingerprint changed'
    }
}

function Write-CapacityL6ExecutionManifest($Context, [string]$RunRoot, [object[]]$Cases) {
    Assert-CapacityL6ExecutionContext $Context
    $root = Get-AbsolutePath $RunRoot 'execution run root' Directory
    if ($null -eq $Cases -or $Cases.Count -eq 0) { throw 'empty execution manifest' }
    $document = [ordered]@{ format=1; kind='capacity_l6_execution_manifest'
        runId=$Context.RunId; fingerprint=$Context.Fingerprint; cases=$Cases }
    $path = Join-Path $root 'execution-manifest.json'
    $bytes = [Text.Encoding]::UTF8.GetBytes(($document | ConvertTo-Json -Depth 16) + "`n")
    $stream = [IO.File]::Open($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $stream.Write($bytes); $stream.Flush($true) } finally { $stream.Dispose() }
    return Get-Sha256 $path
}

Export-ModuleMember -Function Get-CapacityL6Cases, Test-CapacityL6Configuration,
    Assert-CapacityL6Inputs, Invoke-CapacityL6Process, Read-CapacityL6Result, Get-CapacityL6Matrix,
    Test-CapacityL6Completion, New-CapacityL6ExecutionContext,
    Assert-CapacityL6ExecutionContext, Write-CapacityL6ExecutionManifest
