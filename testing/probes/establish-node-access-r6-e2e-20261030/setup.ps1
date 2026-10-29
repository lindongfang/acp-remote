$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030'
if(Test-Path $run){throw 'Refusing existing run'}
if(Get-NetTCPConnection -State Listen -LocalPort 30453,30454,9338,9339 -ErrorAction SilentlyContinue){throw 'Ports occupied'}
$expected='135BD9BDDABFDD530A5251AECB1DFF0DC640E81F80241EC53C8703C2336D1509'
$source='D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030/bin-r6/acp-remote.exe'
if((Get-FileHash $source).Hash -ne $expected){throw 'r6 mismatch'}
New-Item -ItemType Directory $run,"$run/bin","$run/evidence","$run/profiles","$run/target" | Out-Null
Copy-Item $source "$run/bin/acp-remote.exe"
if((Get-FileHash "$run/bin/acp-remote.exe").Hash -ne $expected){throw 'Copy mismatch'}
$config=Get-Content -Raw 'D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030/node-30453.toml'
[IO.File]::WriteAllText("$run/node.toml",$config.Replace('D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030/data-30453',"$run/data"))
$node=Start-Process "$run/bin/acp-remote.exe" -ArgumentList @('--config',"$run/node.toml",'daemon','start') -WindowStyle Hidden -PassThru -RedirectStandardOutput "$run/node-1.out" -RedirectStandardError "$run/node-1.err"
$node.Id | Set-Content "$run/node.pid"
foreach($entry in @(@('A',9338),@('B',9339))){
 $name=$entry[0];$port=$entry[1]
 $edge=Start-Process 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe' -ArgumentList @("--user-data-dir=$run/profiles/$name",'--no-first-run','--no-default-browser-check',"--remote-debugging-port=$port",'https://faraday.tail74842e.ts.net:30453/na032-inspection') -WindowStyle Hidden -PassThru
 $edge.Id | Set-Content "$run/edge-$name.pid"
}
Start-Sleep -Seconds 2
& "$run/bin/acp-remote.exe" --config "$run/node.toml" daemon status
tailscale ip -4
Resolve-DnsName faraday.tail74842e.ts.net -Type A | Select-Object Name,IPAddress
