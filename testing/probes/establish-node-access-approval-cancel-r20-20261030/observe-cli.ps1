param([ValidateSet('committed','precommit','control')][string]$Case,[string]$Stage)
$ErrorActionPreference='Stop';$root='D:/Project/acp-remote/.tmp/approval-cancel-boundary-r20-20261030';$dir=Join-Path $root $Case
if($Stage -notmatch '^[a-z0-9-]+$'){throw 'invalid evidence stage'}
$cliId=[int][IO.File]::ReadAllText((Join-Path $dir 'cli.pid'))
$proc=Get-CimInstance Win32_Process -Filter "ProcessId=$cliId"
if($proc -and ($proc.ExecutablePath -ne "$($root.Replace('/','\'))\bin\acp-remote.exe" -or $proc.CommandLine -notlike "*$Case/proxy.toml*device pair*")){throw 'wrong owned CLI'}
$state=Get-Content -Raw -Encoding utf8 (Join-Path $dir 'proxy-state.json')|ConvertFrom-Json
$record=@{case=$Case;stage=$Stage;at=[DateTimeOffset]::UtcNow.ToString('O');cliAlive=($null -ne $proc);cliPid=$cliId;proxyStage=$state.stage;decisionForwarded=$state.decisionForwarded;approvedHeld=$state.approvedHeld;exitEvidencePresent=(Test-Path -LiteralPath (Join-Path $dir 'cli-exit.json'))}
[IO.File]::WriteAllText((Join-Path $root "evidence/cli-$Case-$Stage.json"),($record|ConvertTo-Json),[Text.UTF8Encoding]::new($false));$record|ConvertTo-Json -Compress
