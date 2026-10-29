$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/r17'
$base='D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/r16'
if(Test-Path $run){throw 'Refusing existing r17 run'}
if(Get-NetTCPConnection -State Listen -LocalPort 30453,30454,9338,9339 -ErrorAction SilentlyContinue){throw 'Ports occupied'}
$manifest='testing/runs/establish-node-access/2026-10-30-implementation-r17.sha256'
$expectedManifest='5074890C209A702335F13042A26DBA64278EFD7A7B0A70A4E42E2F698BA3E26F'
$expectedBinary='38AA9D02F334E3C81FDC900ED4F8943BB78339650981BC0046A609381D03E6C2'
if((Get-FileHash $manifest).Hash -ne $expectedManifest){throw 'Manifest mismatch'}
$rows=Get-Content $manifest;if($rows.Count -ne 83){throw 'Count mismatch'}
foreach($row in $rows){$h,$p=$row -split '  ',2;$f=Join-Path 'D:/Project/acp-remote/.tmp/frozen-implementation-r17-20261030' $p;if((Get-FileHash $f).Hash -ne $h){throw "Frozen mismatch $p"}}
$source='D:/Project/acp-remote/.tmp/frozen-implementation-r17-20261030/bin/acp-remote.exe'
if((Get-FileHash $source).Hash -ne $expectedBinary){throw 'Binary mismatch'}
New-Item -ItemType Directory $run,"$run/bin","$run/evidence" | Out-Null
Copy-Item $source "$run/bin/acp-remote.exe"
if((Get-FileHash "$run/bin/acp-remote.exe").Hash -ne $expectedBinary){throw 'Copy mismatch'}
Copy-Item "$base/node.toml" "$run/node.toml"
$node=Start-Process "$run/bin/acp-remote.exe" -ArgumentList @('--config',"$run/node.toml",'daemon','start') -WindowStyle Hidden -PassThru -RedirectStandardOutput "$run/node.out" -RedirectStandardError "$run/node.err"
$node.Id | Set-Content "$run/node.pid"
foreach($entry in @(@('A',9338),@('B',9339))){$name=$entry[0];$port=$entry[1];$edge=Start-Process 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe' -ArgumentList @("--user-data-dir=$base/profiles/$name",'--no-first-run','--no-default-browser-check',"--remote-debugging-port=$port",'--no-proxy-server','https://faraday.tail74842e.ts.net:30453/na032-inspection') -WindowStyle Hidden -PassThru;$edge.Id | Set-Content "$run/edge-$name.pid"}
@{manifestHash=$expectedManifest;frozenEntries=83;frozenMismatches=0;binaryHash=$expectedBinary;data="$base/data";profiles="$base/profiles"} | ConvertTo-Json | Set-Content "$run/evidence/version.json"
Start-Sleep -Seconds 2
& "$run/bin/acp-remote.exe" --config "$run/node.toml" daemon status
'R17 frozen 83/83 and copied binary verified; only owned R16 data/profile reused'
