$ErrorActionPreference='Stop'
$run=if($env:ACP_E2E_ROOT){$env:ACP_E2E_ROOT}else{'D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030'}
$results=@()
foreach($name in @('key-corrupt','key-unreadable','meta-mismatch')){
 $key="$run/damage/$name/identity.key";$hashBefore=(Get-FileHash $key).Hash;$held=$null
 try{
  if($name -eq 'key-unreadable'){$held=[IO.File]::Open($key,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)}
  $info=New-Object System.Diagnostics.ProcessStartInfo
  $info.FileName="$run/bin/acp-remote.exe"
  $info.Arguments="--config $run/damage/$name.toml daemon start"
  $info.UseShellExecute=$false;$info.CreateNoWindow=$true;$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
  $p=New-Object System.Diagnostics.Process;$p.StartInfo=$info
  if(!$p.Start()){throw 'Child did not start'}
  $stdout=$p.StandardOutput.ReadToEndAsync();$stderr=$p.StandardError.ReadToEndAsync()
  if(!$p.WaitForExit(10000)){Stop-Process -Id $p.Id;throw "Unexpected running damaged node $name"}
  $exit=$p.ExitCode
  [IO.File]::WriteAllText("$run/evidence/$name-v2.out",$stdout.Result)
  [IO.File]::WriteAllText("$run/evidence/$name-v2.err",$stderr.Result)
 }finally{if($null -ne $held){$held.Dispose()}}
 $lock=[IO.File]::Open("$run/damage/$name/.node.lock",[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None);$lock.Dispose()
 $results+=@{case=$name;exit=$exit;error=[string](Get-Content -Encoding utf8 -Raw "$run/evidence/$name-v2.err");readyReported=[bool](Select-String -Path "$run/evidence/$name-v2.out" -Pattern 'node ready' -Quiet);keyUnchanged=((Get-FileHash $key).Hash -eq $hashBefore);lockReopened=$true}
}
$results | ConvertTo-Json | Set-Content "$run/evidence/damage-results-v2.json"
$results | ConvertTo-Json
node ./testing/probes/establish-node-access-r6-e2e-20261030/damage-copies.mjs after
