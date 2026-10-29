$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030'
$profile="$run/profiles/edge-inprivate"
$remaining=@(Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'msedge.exe' -and $_.CommandLine -like '*browser-storage-acceptance-20261030/profiles/edge-inprivate*' })
if($remaining.Count -ne 0){throw 'Private browser still has processes'}
'owned_private_profile_processes_before_reopen=0'
$edge=Start-Process 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe' -ArgumentList @("--user-data-dir=$profile",'--inprivate','--no-first-run','--no-default-browser-check','--remote-debugging-port=9338','https://faraday.tail74842e.ts.net:30453/na031-storage-inspection') -PassThru -WindowStyle Hidden
$edge.Id | Set-Content "$run/private-edge.pid"
Start-Sleep -Seconds 2
Get-Process -Id $edge.Id | Select-Object Id,MainWindowTitle
