$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/platform-acceptance-20261030'
$bin='D:/Project/acp-remote/.tmp/acceptance-20261030-target/debug/acp-remote.exe'
$ownerSid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$canonical='\\?\D:\Project\acp-remote\.tmp\platform-acceptance-20261030\data'
$bytes=[Text.Encoding]::UTF8.GetBytes("acp-remote/local-ipc/v1`0"+$ownerSid+"`0")+[Text.Encoding]::Unicode.GetBytes($canonical)
$sha=[Security.Cryptography.SHA256]::Create()
$digest=([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-','').ToLowerInvariant()
$pipe="acp-remote.$digest"
"owned_pipe_before=$(Test-Path -LiteralPath ('\\.\pipe\'+$pipe))"
& $bin --config "$run/node.toml" daemon stop
"stop_exit=$LASTEXITCODE"
Start-Sleep -Seconds 1
$ownedNodePid=[int](Get-Content "$run/owned.pid")
"owned_process_present=$([bool](Get-Process -Id $ownedNodePid -ErrorAction SilentlyContinue))"
"owned_pipe_after=$(Test-Path -LiteralPath ('\\.\pipe\'+$pipe))"
$listeners=@(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object LocalPort -In 30453,30454)
"owned_port_listeners=$($listeners.Count)"
$lock=[IO.File]::Open("$run/data/.node.lock",[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
$lock.Dispose()
'exclusive_lock_reopen=PASS'
Get-Content "$run/node.out"
Get-Content "$run/node.err"
