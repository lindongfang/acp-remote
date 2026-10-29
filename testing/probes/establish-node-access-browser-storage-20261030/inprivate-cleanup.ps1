$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030'
$bin="$run/bin/acp-remote.exe"
$ownerSid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$canonical='\\?\D:\Project\acp-remote\.tmp\browser-storage-acceptance-20261030\data-30453'
$bytes=[Text.Encoding]::UTF8.GetBytes("acp-remote/local-ipc/v1`0"+$ownerSid+"`0")+[Text.Encoding]::Unicode.GetBytes($canonical)
$digest=([BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($bytes))).Replace('-','').ToLowerInvariant()
$pipe='\\.\pipe\acp-remote.'+$digest
$pipeBefore=Test-Path -LiteralPath $pipe
$stop=& $bin --config "$run/node-30453.toml" daemon stop
$stopExit=$LASTEXITCODE
Start-Sleep -Seconds 1
$privateNodePid=[int](Get-Content "$run/private-node.pid")
$edgeRemaining=@(Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'msedge.exe' -and $_.CommandLine -like '*browser-storage-acceptance-20261030/profiles/edge-inprivate*' })
$listeners=@(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object LocalPort -In 9338,30453,30454)
$lock=[IO.File]::Open("$run/data-30453/.node.lock",[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
$lock.Dispose()
$result=@{stopExit=$stopExit;stopOutput=$stop;nodePresent=[bool](Get-Process -Id $privateNodePid -ErrorAction SilentlyContinue);pipeBefore=$pipeBefore;pipeAfter=(Test-Path -LiteralPath $pipe);privateProfileProcesses=$edgeRemaining.Count;listenerCount=$listeners.Count;lockReopened=$true;binaryHash=(Get-FileHash $bin).Hash}
$result | ConvertTo-Json | Set-Content "$run/evidence/inprivate-cleanup.json"
$result | ConvertTo-Json
