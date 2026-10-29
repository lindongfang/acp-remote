$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030'
if(Test-Path $run){throw 'Refusing to reuse existing run directory'}
$manifest='D:/Project/acp-remote/testing/runs/establish-node-access/2026-10-30-implementation-r5a.sha256'
if((Get-FileHash $manifest).Hash -ne '96F7B25DA1B7ABF3C5F65D069723CA87B095800F08967720AADBD4FC621975EC'){throw 'Manifest mismatch'}
$lines=Get-Content $manifest
if($lines.Count -ne 79){throw 'Wrong entry count'}
foreach($line in $lines){$h,$p=$line -split '  ',2;if((Get-FileHash -LiteralPath $p).Hash -ne $h){throw "Source mismatch: $p"}}
$source='D:/Project/acp-remote/.tmp/acceptance-20261030-r5-target/debug/acp-remote.exe'
$expected='2DC0AFA04B55376797587136048F5EFBE7D665FF7EE69525DA248FA4D73305F2'
if((Get-FileHash $source).Hash -ne $expected){throw 'Binary mismatch'}
New-Item -ItemType Directory -Path $run,"$run/bin","$run/evidence","$run/profiles","$run/target" | Out-Null
Copy-Item -LiteralPath $source -Destination "$run/bin/acp-remote.exe"
if((Get-FileHash "$run/bin/acp-remote.exe").Hash -ne $expected){throw 'Copy mismatch'}
$template=Get-Content -Raw 'D:/Project/acp-remote/.tmp/establish-node-access-tailscale-20261029/acp-remote.toml'
$owned=@()
foreach($port in @(30453,30454)){
  $config=$template.Replace('D:/Project/acp-remote/.tmp/establish-node-access-tailscale-20261029/data',"$run/data-$port").Replace('28444',"$port")
  [IO.File]::WriteAllText("$run/node-$port.toml",$config)
  $p=Start-Process "$run/bin/acp-remote.exe" -ArgumentList @('--config',"$run/node-$port.toml",'daemon','start') -WindowStyle Hidden -PassThru -RedirectStandardOutput "$run/node-$port.out" -RedirectStandardError "$run/node-$port.err"
  $owned+=@{port=$port;pid=$p.Id}
}
$owned | ConvertTo-Json | Set-Content "$run/owned.json"
Start-Sleep -Seconds 2
foreach($port in @(30453,30454)){
  & "$run/bin/acp-remote.exe" --config "$run/node-$port.toml" daemon status
  if($LASTEXITCODE -ne 0){throw "Node $port failed"}
}
'Manifest 79/79 and private binary copy verified'
