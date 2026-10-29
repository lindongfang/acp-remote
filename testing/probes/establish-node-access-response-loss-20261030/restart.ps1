param([ValidateSet(30455,30456)][int]$Port=30455)
$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/approval-response-loss-20261030'
$bin="$run/bin/acp-remote.exe";$config="$run/node-$Port.toml"
$pidFile=if($Port -eq 30455){"$run/owned-node.pid"}else{"$run/owned-node-30456.pid"}
$suffix=if($Port -eq 30455){''}else{'-30456'}
$oldId=[int][IO.File]::ReadAllText($pidFile)
$old=Get-Process -Id $oldId -ErrorAction Stop
if($old.Path -ne "$($run.Replace('/','\'))\bin\acp-remote.exe"){throw 'Not owned node'}
$before=@(& $bin --config $config device list)
if($LASTEXITCODE -ne 0 -or -not ($before -join '').Contains('Independent response-loss recovery')){throw 'committed client not listed'}
[IO.File]::WriteAllText("$run/evidence/owner-list-before-restart$suffix.txt",($before -join "`n"),[Text.UTF8Encoding]::new($false))
& $bin --config $config daemon stop
if($LASTEXITCODE -ne 0){throw 'stop failed'}
if(-not $old.WaitForExit(10000)){throw 'owned node did not exit'}
if(Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue){throw 'port not released'}
$p=Start-Process $bin -ArgumentList @('--config',$config,'daemon','start') -WindowStyle Hidden -PassThru -RedirectStandardOutput "$run/restart$suffix.out" -RedirectStandardError "$run/restart$suffix.err"
[IO.File]::WriteAllText($pidFile,[string]$p.Id)
Start-Sleep -Seconds 1
$status=@(& $bin --config $config daemon status)
if($LASTEXITCODE -ne 0){throw 'restart status failed'}
$after=@(& $bin --config $config device list)
if($LASTEXITCODE -ne 0 -or ($before -join "`n") -ne ($after -join "`n")){throw 'committed list changed'}
[IO.File]::WriteAllText("$run/evidence/restart$suffix.json",(@{oldPid=$oldId;newPid=$p.Id;oldExited=$true;portReleasedBeforeRestart=$true;status=$status;stableOwnerList=$true}|ConvertTo-Json),[Text.UTF8Encoding]::new($false))
Write-Output ($status -join "`n")
Write-Output 'same directory restart and committed client list verified'
