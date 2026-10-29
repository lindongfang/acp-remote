$ErrorActionPreference='Stop'
$run=$env:ACP_E2E_ROOT
if(!$run -or $run -notlike '*r6-e2e-acceptance-20261030/r16'){throw 'Explicit r16 root required'}
$keyBefore=(Get-FileHash "$run/data/identity.key").Hash
$before=@(Get-CimInstance Win32_Process | Where-Object {$_.Name -eq 'msedge.exe' -and $_.CommandLine -like "*$run/profiles/A*"})
& "$run/bin/acp-remote.exe" --config "$run/node.toml" daemon stop
if($LASTEXITCODE -ne 0){throw 'Stop failed'}
node ./testing/probes/establish-node-access-r6-e2e-20261030/close-browser.mjs A
Start-Sleep -Seconds 1
$after=@(Get-CimInstance Win32_Process | Where-Object {$_.Name -eq 'msedge.exe' -and $_.CommandLine -like "*$run/profiles/A*"})
if($after.Count -ne 0){throw 'A profile did not fully exit'}
$config=Get-Content -Raw "$run/node.toml"
[IO.File]::WriteAllText("$run/node.toml",$config.Replace('127.0.0.1:30453','100.119.123.57:30453'))
$node=Start-Process "$run/bin/acp-remote.exe" -ArgumentList @('--config',"$run/node.toml",'daemon','start') -WindowStyle Hidden -PassThru -RedirectStandardOutput "$run/node-ts.out" -RedirectStandardError "$run/node-ts.err"
$node.Id | Set-Content "$run/node.pid"
foreach($entry in @(@('A',9338),@('B',9339))){$name=$entry[0];$port=$entry[1];$edge=Start-Process 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe' -ArgumentList @("--user-data-dir=$run/profiles/$name",'--no-first-run','--no-default-browser-check',"--remote-debugging-port=$port",'--no-proxy-server','https://faraday.tail74842e.ts.net:30453/na032-inspection') -WindowStyle Hidden -PassThru;$edge.Id | Set-Content "$run/edge-$name.pid"}
Start-Sleep -Seconds 1
$state=@{beforeAProcesses=$before.Count;afterAProcesses=$after.Count;sameDataKeyFile=((Get-FileHash "$run/data/identity.key").Hash -eq $keyBefore);canonicalOrigin='https://faraday.tail74842e.ts.net:30453';oldListen='127.0.0.1:30453';newListen='100.119.123.57:30453';hostResolverRulesAfter=$false;noProxyServer=$true}
$state | ConvertTo-Json | Set-Content "$run/evidence/path-switch.json"
$state | ConvertTo-Json
& "$run/bin/acp-remote.exe" --config "$run/node.toml" daemon status
