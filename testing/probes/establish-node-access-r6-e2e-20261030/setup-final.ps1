param([Parameter(Mandatory=$true)][string]$ExpectedBinary,[Parameter(Mandatory=$true)][string]$ExpectedManifest,[string]$Version='r15')
$ErrorActionPreference='Stop'
$run="D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/$Version"
if(Test-Path $run){throw 'Refusing existing final run'}
if(Get-NetTCPConnection -State Listen -LocalPort 30453,30454,9338,9339 -ErrorAction SilentlyContinue){throw 'Ports occupied'}
$manifest="testing/runs/establish-node-access/2026-10-30-implementation-$Version.sha256"
if((Get-FileHash $manifest).Hash -ne $ExpectedManifest){throw 'Manifest mismatch'}
$source='D:/Project/acp-remote/.tmp/acceptance-20261030-r5-target/debug/acp-remote.exe'
if((Get-FileHash $source).Hash -ne $ExpectedBinary){throw 'Final binary mismatch'}
$rows=Get-Content $manifest;$bad=@();foreach($row in $rows){$h,$p=$row -split '  ',2;$actual=(Get-FileHash $p).Hash;if($actual -ne $h){$bad+=@{path=$p;expected=$h;actual=$actual}}}
New-Item -ItemType Directory $run,"$run/bin","$run/evidence","$run/profiles","$run/target" | Out-Null
Copy-Item $source "$run/bin/acp-remote.exe"
if((Get-FileHash "$run/bin/acp-remote.exe").Hash -ne $ExpectedBinary){throw 'Copy mismatch'}
@{manifestHash=$ExpectedManifest;count=$rows.Count;mismatches=$bad;binaryHash=$ExpectedBinary} | ConvertTo-Json -Depth 5 | Set-Content "$run/evidence/version.json"
$template=Get-Content -Raw 'D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/r9/node.toml'
$config=$template.Replace('D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/r9/data',"$run/data").Replace('100.119.123.57:30453','127.0.0.1:30453')
[IO.File]::WriteAllText("$run/node.toml",$config)
$node=Start-Process "$run/bin/acp-remote.exe" -ArgumentList @('--config',"$run/node.toml",'daemon','start') -WindowStyle Hidden -PassThru -RedirectStandardOutput "$run/node-loop.out" -RedirectStandardError "$run/node-loop.err"
$node.Id | Set-Content "$run/node.pid"
$edge=Start-Process 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe' -ArgumentList @("--user-data-dir=$run/profiles/A",'--no-first-run','--no-default-browser-check','--remote-debugging-port=9338','--no-proxy-server','"--host-resolver-rules=MAP faraday.tail74842e.ts.net 127.0.0.1"','https://faraday.tail74842e.ts.net:30453/na032-inspection') -WindowStyle Hidden -PassThru
$edge.Id | Set-Content "$run/edge-A.pid"
Start-Sleep -Seconds 2
& "$run/bin/acp-remote.exe" --config "$run/node.toml" daemon status
"Final $Version manifest=$($rows.Count) mismatches=$($bad.Count) copied_binary=$ExpectedBinary"
