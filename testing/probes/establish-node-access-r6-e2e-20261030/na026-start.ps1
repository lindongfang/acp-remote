$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/r17';$base='D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/r16'
if((Get-FileHash "$run/bin/acp-remote.exe").Hash -ne '38AA9D02F334E3C81FDC900ED4F8943BB78339650981BC0046A609381D03E6C2'){throw 'Binary mismatch'}
if(Get-NetTCPConnection -State Listen -LocalPort 30453,9339 -ErrorAction SilentlyContinue){throw 'Ports occupied'}
node ./testing/probes/establish-node-access-r6-e2e-20261030/na026-sql.mjs install
if($LASTEXITCODE -ne 0){throw 'Trigger install failed'}
$node=Start-Process "$run/bin/acp-remote.exe" -ArgumentList @('--config',"$run/node.toml",'daemon','start') -WindowStyle Hidden -PassThru -RedirectStandardOutput "$run/node-na026.out" -RedirectStandardError "$run/node-na026.err";$node.Id | Set-Content "$run/node.pid"
$edge=Start-Process 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe' -ArgumentList @("--user-data-dir=$base/profiles/B",'--no-first-run','--no-default-browser-check','--no-proxy-server','--remote-debugging-port=9339','https://faraday.tail74842e.ts.net:30453/na032-inspection') -WindowStyle Hidden -PassThru;$edge.Id | Set-Content "$run/edge-B.pid"
Start-Sleep -Seconds 1
& "$run/bin/acp-remote.exe" --config "$run/node.toml" daemon status
