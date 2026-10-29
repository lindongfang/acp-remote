$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030'
node ./testing/probes/establish-node-access-r6-e2e-20261030/damage-copies.mjs prepare
if($LASTEXITCODE -ne 0){throw 'Prepare failed'}
$results=@()
foreach($name in @('key-corrupt','key-unreadable','meta-mismatch')){
 $key="$run/damage/$name/identity.key"
 $hashBefore=(Get-FileHash $key).Hash
 try{
  if($name -eq 'key-unreadable'){icacls $key /deny 'faraday\jack:(RD)' | Out-Null}
  $p=Start-Process "$run/bin/acp-remote.exe" -ArgumentList @('--config',"$run/damage/$name.toml",'daemon','start') -WindowStyle Hidden -PassThru -RedirectStandardOutput "$run/evidence/$name.out" -RedirectStandardError "$run/evidence/$name.err"
  if(!$p.WaitForExit(10000)){Stop-Process -Id $p.Id;throw "Unexpected running damaged node $name"}
  $exit=$p.ExitCode
 }finally{if($name -eq 'key-unreadable'){icacls $key /remove:d 'faraday\jack' | Out-Null}}
 $results+=@{case=$name;exit=$exit;error=(Get-Content -Raw "$run/evidence/$name.err");readyReported=[bool](Select-String -Path "$run/evidence/$name.out" -Pattern 'node ready' -Quiet);keyUnchanged=((Get-FileHash $key).Hash -eq $hashBefore)}
}
$results | ConvertTo-Json | Set-Content "$run/evidence/damage-results.json"
$results | ConvertTo-Json
node ./testing/probes/establish-node-access-r6-e2e-20261030/damage-copies.mjs after
