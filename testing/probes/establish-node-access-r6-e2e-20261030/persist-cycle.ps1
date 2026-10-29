$ErrorActionPreference='Stop'
$run=if($env:ACP_E2E_ROOT){$env:ACP_E2E_ROOT}else{'D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030'}
$profile="*$run/profiles/A*"
$before=@(Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'msedge.exe' -and $_.CommandLine -like $profile })
if($before.Count -eq 0){throw 'No owned A processes'}
node ./testing/probes/establish-node-access-r6-e2e-20261030/close-browser.mjs A
Start-Sleep -Seconds 1
$after=@(Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'msedge.exe' -and $_.CommandLine -like $profile })
@{beforeCount=$before.Count;beforePids=@($before.ProcessId);afterCount=$after.Count} | ConvertTo-Json | Set-Content "$run/evidence/A-full-exit.json"
if($after.Count -ne 0){throw 'A processes remain'}
$edge=Start-Process 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe' -ArgumentList @("--user-data-dir=$run/profiles/A",'--no-first-run','--no-default-browser-check','--remote-debugging-port=9338','https://faraday.tail74842e.ts.net:30453/na032-inspection') -WindowStyle Hidden -PassThru
$edge.Id | Set-Content "$run/edge-A.pid"
Start-Sleep -Seconds 1
Get-Process -Id $edge.Id | Select-Object Id,MainWindowTitle
"full_exit_before=$($before.Count) after=$($after.Count)"
