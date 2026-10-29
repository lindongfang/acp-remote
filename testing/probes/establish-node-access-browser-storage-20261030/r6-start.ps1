$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030'
$manifest='testing/runs/establish-node-access/2026-10-30-implementation-r6.sha256'
if((Get-FileHash $manifest).Hash -ne '1EB583A929AAB7769691FC05EF9A3A86D876E7502D94AC9CBC9F24D5EBCB10F7'){throw 'r6 manifest mismatch'}
$rows=Get-Content $manifest
if($rows.Count -ne 83){throw 'r6 count mismatch'}
foreach($row in $rows){$hash,$path=$row -split '  ',2;if((Get-FileHash -LiteralPath $path).Hash -ne $hash){throw "Mismatch: $path"}}
$expected='135BD9BDDABFDD530A5251AECB1DFF0DC640E81F80241EC53C8703C2336D1509'
$source='D:/Project/acp-remote/.tmp/acceptance-20261030-r5-target/debug/acp-remote.exe'
if((Get-FileHash $source).Hash -ne $expected){throw 'r6 binary mismatch'}
if((Test-Path "$run/bin-r6") -or (Test-Path "$run/profiles/edge-inprivate-r6")){throw 'Refusing existing r6 resources'}
if(Get-NetTCPConnection -State Listen -LocalPort 9338,30453 -ErrorAction SilentlyContinue){throw 'Required port occupied'}
New-Item -ItemType Directory "$run/bin-r6" | Out-Null
Copy-Item -LiteralPath $source -Destination "$run/bin-r6/acp-remote.exe"
if((Get-FileHash "$run/bin-r6/acp-remote.exe").Hash -ne $expected){throw 'r6 copy mismatch'}
$template=Get-Content -Raw "$run/node-30453.toml"
[IO.File]::WriteAllText("$run/node-r6.toml",$template.Replace('/data-30453','/data-r6-30453'))
$node=Start-Process "$run/bin-r6/acp-remote.exe" -ArgumentList @('--config',"$run/node-r6.toml",'daemon','start') -PassThru -WindowStyle Hidden -RedirectStandardOutput "$run/node-r6.out" -RedirectStandardError "$run/node-r6.err"
$node.Id | Set-Content "$run/node-r6.pid"
$edge=Start-Process 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe' -ArgumentList @("--user-data-dir=$run/profiles/edge-inprivate-r6",'--inprivate','--no-first-run','--no-default-browser-check','--remote-debugging-port=9338','https://faraday.tail74842e.ts.net:30453/na031-storage-inspection') -PassThru -WindowStyle Hidden
$edge.Id | Set-Content "$run/edge-r6.pid"
Start-Sleep -Seconds 2
& "$run/bin-r6/acp-remote.exe" --config "$run/node-r6.toml" daemon status
Get-Process -Id $edge.Id | Select-Object Id,MainWindowTitle
"manifest=83/83 binary=$expected"
