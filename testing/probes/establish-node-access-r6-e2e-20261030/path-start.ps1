$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/r12-path'
if(Test-Path $run){throw 'Refusing existing path run'}
if(Get-NetTCPConnection -State Listen -LocalPort 30453,9338 -ErrorAction SilentlyContinue){throw 'Ports occupied'}
$source='D:/Project/acp-remote/.tmp/acceptance-20261030-r5-target/debug/acp-remote.exe'
$expected='92D9D759D4D6EFE5806338FB1E3DF0FA996CCA10F4766D9E6CC4E14484A2DD44'
if((Get-FileHash $source).Hash -ne $expected){throw 'R12 binary mismatch'}
New-Item -ItemType Directory $run,"$run/bin","$run/evidence","$run/profiles","$run/target" | Out-Null
Copy-Item $source "$run/bin/acp-remote.exe"
if((Get-FileHash "$run/bin/acp-remote.exe").Hash -ne $expected){throw 'Copy mismatch'}
$manifest='testing/runs/establish-node-access/2026-10-30-implementation-r12.sha256'
$rows=Get-Content $manifest;$bad=@();foreach($row in $rows){$h,$p=$row -split '  ',2;$a=(Get-FileHash $p).Hash;if($a -ne $h){$bad+=@{path=$p;expected=$h;actual=$a}}}
@{manifestHash=(Get-FileHash $manifest).Hash;count=$rows.Count;mismatches=$bad;binaryHash=$expected} | ConvertTo-Json -Depth 5 | Set-Content "$run/evidence/version.json"
$template=Get-Content -Raw 'D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/r9/node.toml'
$config=$template.Replace('D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/r9/data',"$run/data").Replace('100.119.123.57:30453','127.0.0.1:30453')
[IO.File]::WriteAllText("$run/node.toml",$config)
$node=Start-Process "$run/bin/acp-remote.exe" -ArgumentList @('--config',"$run/node.toml",'daemon','start') -WindowStyle Hidden -PassThru -RedirectStandardOutput "$run/node-loop.out" -RedirectStandardError "$run/node-loop.err"
$node.Id | Set-Content "$run/node.pid"
$edge=Start-Process 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe' -ArgumentList @("--user-data-dir=$run/profiles/A",'--no-first-run','--no-default-browser-check','--remote-debugging-port=9338','"--host-resolver-rules=MAP faraday.tail74842e.ts.net 127.0.0.1"','https://faraday.tail74842e.ts.net:30453/na032-inspection') -WindowStyle Hidden -PassThru
$edge.Id | Set-Content "$run/edge-A.pid"
Start-Sleep -Seconds 2
& "$run/bin/acp-remote.exe" --config "$run/node.toml" daemon status
"R12 manifest=$($rows.Count) mismatches=$($bad.Count) copy=$expected"
