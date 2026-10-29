$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030'
$bin="$run/bin/acp-remote.exe"
$owned=Get-Content -Raw "$run/owned.json" | ConvertFrom-Json
$results=@()
foreach($item in $owned){
 $port=[int]$item.port
 $ownerSid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
 $canonical="\\?\D:\Project\acp-remote\.tmp\browser-storage-acceptance-20261030\data-$port"
 $bytes=[Text.Encoding]::UTF8.GetBytes("acp-remote/local-ipc/v1`0"+$ownerSid+"`0")+[Text.Encoding]::Unicode.GetBytes($canonical)
 $sha=[Security.Cryptography.SHA256]::Create()
 $digest=([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-','').ToLowerInvariant()
 $pipe='\\.\pipe\acp-remote.'+$digest
 $before=Test-Path -LiteralPath $pipe
 $stop=& $bin --config "$run/node-$port.toml" daemon stop
 $exit=$LASTEXITCODE
 Start-Sleep -Seconds 1
 $lock=[IO.File]::Open("$run/data-$port/.node.lock",[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
 $lock.Dispose()
 $results+=@{port=$port;pid=$item.pid;stopExit=$exit;stopOutput=$stop;pipeBefore=$before;pipeAfter=(Test-Path -LiteralPath $pipe);processPresent=[bool](Get-Process -Id $item.pid -ErrorAction SilentlyContinue);lockReopened=$true}
}
$listeners=@(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object LocalPort -In 30453,30454)
$summary=@{nodes=$results;listenerCount=$listeners.Count;binaryHash=(Get-FileHash $bin).Hash}
$summary | ConvertTo-Json -Depth 5 | Set-Content "$run/evidence/node-cleanup.json"
$summary | ConvertTo-Json -Depth 5
