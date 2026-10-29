$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030'
$remaining=@(Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'msedge.exe' -and $_.CommandLine -like '*browser-storage-acceptance-20261030/profiles/edge-inprivate-r6*' })
if($remaining.Count -ne 0){throw 'r6 private processes remain'}
'r6_private_processes_before_reopen=0'
Copy-Item "$run/evidence/r6-empty-store.json" "$run/evidence/r6-empty-store-initial-literal.json"
$edge=Start-Process 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe' -ArgumentList @("--user-data-dir=$run/profiles/edge-inprivate-r6",'--inprivate','--no-first-run','--no-default-browser-check','--remote-debugging-port=9338','https://faraday.tail74842e.ts.net:30453/na031-storage-inspection') -PassThru -WindowStyle Hidden
$edge.Id | Set-Content "$run/edge-r6.pid"
Start-Sleep -Seconds 2
Get-Process -Id $edge.Id | Select-Object Id,MainWindowTitle
