$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030'
$bin="$run/bin-r6/acp-remote.exe"
$ownerSid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$canonical='\\?\D:\Project\acp-remote\.tmp\browser-storage-acceptance-20261030\data-r6-30453'
$bytes=[Text.Encoding]::UTF8.GetBytes("acp-remote/local-ipc/v1`0"+$ownerSid+"`0")+[Text.Encoding]::Unicode.GetBytes($canonical)
$digest=([BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($bytes))).Replace('-','').ToLowerInvariant()
$pipe='\\.\pipe\acp-remote.'+$digest
$pipeBefore=Test-Path -LiteralPath $pipe
$stop=& $bin --config "$run/node-r6.toml" daemon stop
$stopExit=$LASTEXITCODE
Start-Sleep -Seconds 1
$nodePid=[int](Get-Content "$run/node-r6.pid")
$privateRemaining=@(Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'msedge.exe' -and $_.CommandLine -like '*browser-storage-acceptance-20261030/profiles/edge-inprivate-r6*' })
$listeners=@(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object LocalPort -In 9338,30453,30454)
$lock=[IO.File]::Open("$run/data-r6-30453/.node.lock",[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None);$lock.Dispose()
$result=@{stopExit=$stopExit;stopOutput=$stop;nodePresent=[bool](Get-Process -Id $nodePid -ErrorAction SilentlyContinue);pipeBefore=$pipeBefore;pipeAfter=(Test-Path -LiteralPath $pipe);privateProfileProcesses=$privateRemaining.Count;listenerCount=$listeners.Count;lockReopened=$true;binaryHash=(Get-FileHash $bin).Hash}
$result | ConvertTo-Json | Set-Content "$run/evidence/r6-cleanup.json"
$result | ConvertTo-Json
