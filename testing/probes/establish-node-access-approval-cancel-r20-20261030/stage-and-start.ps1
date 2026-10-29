$ErrorActionPreference='Stop'
$root='D:/Project/acp-remote/.tmp/approval-cancel-boundary-r20-20261030'
$source='D:/Project/acp-remote/.tmp/frozen-implementation-r20-20261030/bin/acp-remote.exe'
$expected='561A6024E983DD5239C09D1388CA5D256625F690EE9345E64B9654BC43AAFBE3'
$manifest='D:/Project/acp-remote/testing/runs/establish-node-access/2026-10-30-implementation-r20.sha256'
if((Get-FileHash -LiteralPath $manifest).Hash -ne '73727BCF617F3F96BCC24C4CBB7B38BA044A56856C6CB4D628869832C8B368A4'){throw 'R20 manifest mismatch'}
if((Get-Content -LiteralPath $manifest).Count -ne 83){throw 'R20 manifest count mismatch'}
if((Get-FileHash -LiteralPath $source).Hash -ne $expected){throw 'R20 binary mismatch'}
if(Test-Path -LiteralPath "$root/bin/acp-remote.exe"){throw 'refuse existing staged product'}
Copy-Item -LiteralPath $source -Destination "$root/bin/acp-remote.exe"
if((Get-FileHash -LiteralPath "$root/bin/acp-remote.exe").Hash -ne $expected){throw 'copy mismatch'}
foreach($port in @(30455,30456)){if(Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue){throw 'owned port occupied'};$config=@"
data_dir = "$root/data-$port"
listen = "100.119.123.57:$port"
canonical_origin = "https://faraday.tail74842e.ts.net:$port"
tls_cert = "D:/Project/acp-remote/.tmp/establish-node-access-tailscale-20261029/faraday.tail74842e.ts.net.crt"
tls_key = "D:/Project/acp-remote/.tmp/establish-node-access-tailscale-20261029/faraday.tail74842e.ts.net.key"
"@;[IO.File]::WriteAllText("$root/node-$port.toml",$config,[Text.UTF8Encoding]::new($false))}
$p=Start-Process "$root/bin/acp-remote.exe" -ArgumentList @('--config',"$root/node-30455.toml",'daemon','start') -WindowStyle Hidden -PassThru -RedirectStandardOutput "$root/node.out" -RedirectStandardError "$root/node.err"
[IO.File]::WriteAllText("$root/owned-node.pid",[string]$p.Id);Start-Sleep -Milliseconds 800
& "$root/bin/acp-remote.exe" --config "$root/node-30455.toml" daemon status
if($LASTEXITCODE -ne 0){throw 'R20 node not ready'}
