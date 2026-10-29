$ErrorActionPreference='Stop'
$base='D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030'
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$resources=@()
foreach($sub in @('data','r15/data','r16/data','r9/data','r9/damage/key-corrupt','r9/damage/key-unreadable','r9/damage/meta-mismatch')){
 $dir="$base/$sub";$canonical='\\?\'+[IO.Path]::GetFullPath($dir)
 $bytes=[Text.Encoding]::UTF8.GetBytes("acp-remote/local-ipc/v1`0"+$sid+"`0")+[Text.Encoding]::Unicode.GetBytes($canonical)
 $digest=([BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($bytes))).Replace('-','').ToLowerInvariant()
 $lock=[IO.File]::Open("$dir/.node.lock",[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None);$lock.Dispose()
 $resources+=@{directory=$sub;pipePresent=(Test-Path -LiteralPath ('\\.\pipe\acp-remote.'+$digest));exclusiveLockReopened=$true}
}
$edges=@(Get-CimInstance Win32_Process | Where-Object {$_.Name -eq 'msedge.exe' -and $_.CommandLine -like '*r6-e2e-acceptance-20261030*profiles*'})
$listeners=@(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object LocalPort -In 30453,30454,9338,9339)
$result=@{edgeProcesses=$edges.Count;listeners=$listeners.Count;resources=$resources;r16Binary=(Get-FileHash "$base/r16/bin/acp-remote.exe").Hash;r17Binary=(Get-FileHash "$base/r17/bin/acp-remote.exe").Hash;r18Binary=(Get-FileHash "$base/r18/bin/acp-remote.exe").Hash}
$result | ConvertTo-Json -Depth 5 | Set-Content "$base/r17/evidence/latest-cleanup.json"
$result | ConvertTo-Json -Depth 5
