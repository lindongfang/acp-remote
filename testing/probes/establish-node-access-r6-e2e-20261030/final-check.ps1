$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/r9'
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$checks=@()
foreach($sub in @('data','damage/key-corrupt','damage/key-unreadable','damage/meta-mismatch')){
 $dir="$run/$sub"
 $canonical='\\?\'+[IO.Path]::GetFullPath($dir)
 $bytes=[Text.Encoding]::UTF8.GetBytes("acp-remote/local-ipc/v1`0"+$sid+"`0")+[Text.Encoding]::Unicode.GetBytes($canonical)
 $digest=([BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($bytes))).Replace('-','').ToLowerInvariant()
 $lock=[IO.File]::Open("$dir/.node.lock",[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None);$lock.Dispose()
 $checks+=@{directory=$sub;pipePresent=(Test-Path -LiteralPath ('\\.\pipe\acp-remote.'+$digest));exclusiveLockReopened=$true}
}
$edges=@(Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'msedge.exe' -and $_.CommandLine -like '*r6-e2e-acceptance-20261030*profiles*' })
$listeners=@(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object LocalPort -In 30453,30454,9338,9339)
$result=@{edgeProcesses=$edges.Count;listeners=$listeners.Count;resources=$checks;binaryHash=(Get-FileHash "$run/bin/acp-remote.exe").Hash}
$result | ConvertTo-Json -Depth 5 | Set-Content "$run/evidence/final-cleanup.json"
$result | ConvertTo-Json -Depth 5
