$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/cross-sid-r20-20261030'
$bin="$run/shared/acp-remote.exe"
$config="$run/shared/node.toml"
$result=Get-Content -Raw "$run/abc-evidence/result.json" | ConvertFrom-Json
if($result.sid -ne 'S-1-5-21-1850245178-3063344048-767428509-1006' -or $result.commands.Count -ne 2){throw 'Invalid child evidence'}
foreach($row in $result.commands){if($row.ExitCode -ne 1 -or $row.TimedOut -or -not $row.ConsoleInput -or -not $row.ConsoleOutput -or $row.Output -notmatch 'canonicalizing.*os error 5'){throw 'Child assertion failed'}}
$status=& $bin --config $config daemon status
$statusExit=$LASTEXITCODE
$list=& $bin --config $config device list
$listExit=$LASTEXITCODE
node "$PSScriptRoot/snapshot.mjs" after
if($LASTEXITCODE){throw 'Snapshot failed'}
$before=Get-Content -Raw "$run/owner-evidence/before.json" | ConvertFrom-Json
$after=Get-Content -Raw "$run/owner-evidence/after.json" | ConvertFrom-Json
$same=$before.logicalSha256 -eq $after.logicalSha256
$ownerSid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$canonical='\\?\D:\Project\acp-remote\.tmp\cross-sid-r20-20261030\data'
$bytes=[Text.Encoding]::UTF8.GetBytes("acp-remote/local-ipc/v1`0"+$ownerSid+"`0")+[Text.Encoding]::Unicode.GetBytes($canonical)
$sha=[Security.Cryptography.SHA256]::Create()
$digest=([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-','').ToLowerInvariant()
$pipe='\\.\pipe\acp-remote.'+$digest
$pipeBefore=Test-Path -LiteralPath $pipe
$stop=& $bin --config $config daemon stop
$stopExit=$LASTEXITCODE
Start-Sleep -Seconds 1
$nodePid=[int](Get-Content "$run/owner-evidence/node.pid")
$nodeRemaining=[bool](Get-Process -Id $nodePid -ErrorAction SilentlyContinue)
$pipeAfter=Test-Path -LiteralPath $pipe
$listeners=@(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object LocalPort -In 30453,30454).Count
$lock=[IO.File]::Open("$run/data/.node.lock",[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
$lock.Dispose()
$watchdogPid=[int](Get-Content "$run/owner-evidence/watchdog.pid")
$watchdog=Get-CimInstance Win32_Process -Filter "ProcessId=$watchdogPid"
if($watchdog -and $watchdog.CommandLine -like '*establish-node-access-cross-sid-r20-20261030/watchdog.ps1*'){Stop-Process -Id $watchdogPid -ErrorAction Stop; Wait-Process -Id $watchdogPid -Timeout 3 -ErrorAction SilentlyContinue}
$record=[ordered]@{ownerSid=$ownerSid; status=$status;statusExit=$statusExit;list=$list;listExit=$listExit;dbUnchanged=$same;logicalSha256=$after.logicalSha256;pipe=$pipe;pipeBefore=$pipeBefore;stop=$stop;stopExit=$stopExit;nodeRemaining=$nodeRemaining;pipeAfter=$pipeAfter;listeners=$listeners;exclusiveLockReopen=$true;watchdogRemaining=[bool](Get-Process -Id $watchdogPid -ErrorAction SilentlyContinue);binaryHash=(Get-FileHash $bin).Hash;childResultHash=(Get-FileHash "$run/abc-evidence/result.json").Hash}
$record | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 "$run/owner-evidence/final.json"
$record | ConvertTo-Json -Depth 5
if($statusExit -or $listExit -or -not $same -or -not $pipeBefore -or $stopExit -or $nodeRemaining -or $pipeAfter -or $listeners){throw 'Final assertion failed'}
