param([ValidateSet('loopback','tailscale')][string]$Mode='loopback')
$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/r15'
node ./testing/probes/establish-node-access-r6-e2e-20261030/close-browser.mjs A
Start-Sleep -Seconds 1
$remaining=@(Get-CimInstance Win32_Process | Where-Object {$_.Name -eq 'msedge.exe' -and $_.CommandLine -like '*r6-e2e-acceptance-20261030/r15/profiles/A*'})
if($remaining.Count -ne 0){throw 'Owned profile did not fully exit'}
$args=@("--user-data-dir=$run/profiles/A",'--no-first-run','--no-default-browser-check','--remote-debugging-port=9338','--no-proxy-server')
if($Mode -eq 'loopback'){$args+='"--host-resolver-rules=MAP faraday.tail74842e.ts.net 127.0.0.1"'}
$args+='https://faraday.tail74842e.ts.net:30453/na032-inspection'
$edge=Start-Process 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe' -ArgumentList $args -WindowStyle Hidden -PassThru
$edge.Id | Set-Content "$run/edge-A.pid"
Start-Sleep -Seconds 1
"profile_before_reopen=0 mode=$Mode no_proxy_server=True"
