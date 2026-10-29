$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/r18';$base='D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/r16'
if(Test-Path $run){throw 'Refusing existing r18 run'}
if(Get-NetTCPConnection -State Listen -LocalPort 30453,9339 -ErrorAction SilentlyContinue){throw 'Ports occupied'}
$expected='328D7CEBCA708A300D48AA673204E4E52F1131E6E05951F85DA4601DA7AA2800'
$manifest='testing/runs/establish-node-access/2026-10-30-implementation-r18.sha256'
if((Get-FileHash $manifest).Hash -ne '614413C0243680E6E7E0DF3C98141F0D568DDCAA042209EB9C996A99881E5B8A'){throw 'Manifest mismatch'}
$rows=Get-Content $manifest;if($rows.Count -ne 83){throw 'Wrong count'}
foreach($row in $rows){$h,$p=$row -split '  ',2;if((Get-FileHash (Join-Path '.tmp/frozen-implementation-r18-20261030' $p)).Hash -ne $h){throw "Frozen mismatch $p"}}
$source='D:/Project/acp-remote/.tmp/frozen-implementation-r18-20261030/bin/acp-remote.exe'
if((Get-FileHash $source).Hash -ne $expected){throw 'Binary mismatch'}
New-Item -ItemType Directory $run,"$run/bin","$run/evidence" | Out-Null
Copy-Item $source "$run/bin/acp-remote.exe"
if((Get-FileHash "$run/bin/acp-remote.exe").Hash -ne $expected){throw 'Copy mismatch'}
Copy-Item 'D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/r17/node.toml' "$run/node.toml"
$p=Start-Process "$run/bin/acp-remote.exe" -ArgumentList @('--config',"$run/node.toml",'daemon','start') -WindowStyle Hidden -PassThru -RedirectStandardOutput "$run/node.out" -RedirectStandardError "$run/node.err";$p.Id | Set-Content "$run/node.pid"
$edge=Start-Process 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe' -ArgumentList @("--user-data-dir=$base/profiles/B",'--no-first-run','--no-default-browser-check','--no-proxy-server','--remote-debugging-port=9339','https://faraday.tail74842e.ts.net:30453/na032-inspection') -WindowStyle Hidden -PassThru;$edge.Id | Set-Content "$run/edge-B.pid"
@{manifestHash=(Get-FileHash $manifest).Hash;frozenEntries=83;frozenMismatches=0;binaryHash=$expected;data="$base/data";profile="$base/profiles/B"} | ConvertTo-Json | Set-Content "$run/evidence/version.json"
Start-Sleep -Seconds 1
& "$run/bin/acp-remote.exe" --config "$run/node.toml" daemon status
