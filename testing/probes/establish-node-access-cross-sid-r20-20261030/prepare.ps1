$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/cross-sid-r20-20261030'
$probe='D:/Project/acp-remote/testing/probes/establish-node-access-cross-sid-r20-20261030'
$sid='*S-1-5-21-1850245178-3063344048-767428509-1006'
if(Test-Path $run){throw 'Refusing existing runtime'}
if(Get-NetTCPConnection -State Listen -LocalPort 30453,30454 -ErrorAction SilentlyContinue){throw 'Owned ports unavailable'}
$source='D:/Project/acp-remote/.tmp/frozen-implementation-r20-20261030/bin/acp-remote.exe'
if((Get-FileHash $source).Hash -ne '561A6024E983DD5239C09D1388CA5D256625F690EE9345E64B9654BC43AAFBE3'){throw 'Binary mismatch'}
New-Item -ItemType Directory $run,"$run/shared","$run/abc-evidence","$run/owner-evidence" | Out-Null
$owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
& icacls $run /inheritance:r /grant:r "*$($owner):(OI)(CI)(F)" "$($sid):(X)" | Out-Null
if($LASTEXITCODE){throw 'Root ACL failed'}
& icacls "$run/shared" /grant:r "$($sid):(OI)(CI)(RX)" | Out-Null
if($LASTEXITCODE){throw 'Shared ACL failed'}
& icacls "$run/abc-evidence" /grant:r "$($sid):(OI)(CI)(M)" | Out-Null
if($LASTEXITCODE){throw 'Evidence ACL failed'}
Copy-Item $source "$run/shared/acp-remote.exe"
Copy-Item "$probe/child.ps1","$probe/ConsoleRunner.cs" "$run/shared/"
$config=Get-Content -Raw 'D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/r16/node.toml'
$config=$config.Replace('D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/r16/data',"$run/data").Replace('100.119.123.57:30453','127.0.0.1:30453')
[IO.File]::WriteAllText("$run/shared/node.toml",$config)
Get-FileHash "$run/shared/acp-remote.exe","$run/shared/child.ps1","$run/shared/ConsoleRunner.cs","$run/shared/node.toml" | ConvertTo-Json | Set-Content "$run/owner-evidence/fixture-hashes.json"
$p=Start-Process "$run/shared/acp-remote.exe" -ArgumentList @('--config',"$run/shared/node.toml",'daemon','start') -WindowStyle Hidden -PassThru -RedirectStandardOutput "$run/owner-evidence/node.out" -RedirectStandardError "$run/owner-evidence/node.err"
$p.Id | Set-Content "$run/owner-evidence/node.pid"
Start-Sleep -Seconds 2
& "$run/shared/acp-remote.exe" --config "$run/shared/node.toml" daemon status
if($LASTEXITCODE){throw 'Owner status failed'}
& "$run/shared/acp-remote.exe" --config "$run/shared/node.toml" device list
if($LASTEXITCODE){throw 'Owner list failed'}
& icacls "$run/data" | Set-Content "$run/owner-evidence/data-acl.txt"
node "$probe/snapshot.mjs" before
