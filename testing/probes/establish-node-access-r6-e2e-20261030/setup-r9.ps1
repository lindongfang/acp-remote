$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/r9'
if(Test-Path $run){throw 'Refusing existing r9 run'}
if(Get-NetTCPConnection -State Listen -LocalPort 30453,30454,9338,9339 -ErrorAction SilentlyContinue){throw 'Ports occupied'}
$manifest='testing/runs/establish-node-access/2026-10-30-implementation-r9.sha256'
if((Get-FileHash $manifest).Hash -ne '7876C7803106193C476EE0C23DAC27AA25036154CDF293D6A7EB7ECD2FEA6DB7'){throw 'Manifest mismatch'}
$rows=Get-Content $manifest;if($rows.Count -ne 83){throw 'Count mismatch'}
$mismatches=@();foreach($row in $rows){$h,$p=$row -split '  ',2;$actual=(Get-FileHash $p).Hash;if($actual -ne $h){$mismatches+=@{path=$p;expected=$h;actual=$actual}}}
$expected='AB97418E98D67A71E25BBF62B1E780F2F767530CA4290FB9C78E220A222A6024'
$source='D:/Project/acp-remote/.tmp/acceptance-20261030-r5-target/debug/acp-remote.exe'
if((Get-FileHash $source).Hash -ne $expected){throw 'r9 binary mismatch'}
New-Item -ItemType Directory $run,"$run/bin","$run/evidence","$run/profiles","$run/target" | Out-Null
$mismatches | ConvertTo-Json | Set-Content "$run/evidence/source-mismatches-at-start.json"
Copy-Item $source "$run/bin/acp-remote.exe"
if((Get-FileHash "$run/bin/acp-remote.exe").Hash -ne $expected){throw 'Copy mismatch'}
$config=Get-Content -Raw 'D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/node.toml'
[IO.File]::WriteAllText("$run/node.toml",$config.Replace('D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/data',"$run/data"))
$node=Start-Process "$run/bin/acp-remote.exe" -ArgumentList @('--config',"$run/node.toml",'daemon','start') -WindowStyle Hidden -PassThru -RedirectStandardOutput "$run/node-1.out" -RedirectStandardError "$run/node-1.err"
$node.Id | Set-Content "$run/node.pid"
foreach($entry in @(@('A',9338),@('B',9339))){$name=$entry[0];$port=$entry[1];$edge=Start-Process 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe' -ArgumentList @("--user-data-dir=$run/profiles/$name",'--no-first-run','--no-default-browser-check',"--remote-debugging-port=$port",'https://faraday.tail74842e.ts.net:30453/na032-inspection') -WindowStyle Hidden -PassThru;$edge.Id | Set-Content "$run/edge-$name.pid"}
Start-Sleep -Seconds 2
& "$run/bin/acp-remote.exe" --config "$run/node.toml" daemon status
"r9 manifest entries=83 source_mismatches=$($mismatches.Count); fixed binary copy verified"
