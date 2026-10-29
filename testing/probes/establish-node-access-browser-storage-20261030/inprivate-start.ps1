$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030'
$private="$run/profiles/edge-inprivate"
if(Test-Path $private){throw 'Refusing existing private profile'}
if(Get-NetTCPConnection -State Listen -LocalPort 9338,30453 -ErrorAction SilentlyContinue){throw 'Required port already occupied'}
if((Get-FileHash "$run/bin/acp-remote.exe").Hash -ne '2DC0AFA04B55376797587136048F5EFBE7D665FF7EE69525DA248FA4D73305F2'){throw 'r5a binary mismatch'}
$node=Start-Process "$run/bin/acp-remote.exe" -ArgumentList @('--config',"$run/node-30453.toml",'daemon','start') -PassThru -WindowStyle Hidden -RedirectStandardOutput "$run/private-node.out" -RedirectStandardError "$run/private-node.err"
$node.Id | Set-Content "$run/private-node.pid"
$edge='C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe'
$args=@("--user-data-dir=$private",'--inprivate','--no-first-run','--no-default-browser-check','--remote-debugging-port=9338','https://faraday.tail74842e.ts.net:30453')
$browser=Start-Process $edge -ArgumentList $args -PassThru -WindowStyle Hidden
$browser.Id | Set-Content "$run/private-edge.pid"
Start-Sleep -Seconds 3
& "$run/bin/acp-remote.exe" --config "$run/node-30453.toml" daemon status
"private_edge_root_pid=$($browser.Id)"
try{(Invoke-RestMethod 'http://127.0.0.1:9338/json/version').Browser}catch{'CDP endpoint unavailable: '+$_.Exception.Message}
