param([ValidateSet('committed','precommit','control')][string]$Case)
$ErrorActionPreference='Stop';$run='D:/Project/acp-remote/.tmp/approval-cancel-boundary-r20-20261030';$dir=Join-Path $run $Case
if(Test-Path -LiteralPath $dir){throw 'case exists'}
New-Item -ItemType Directory -Path $dir,(Join-Path $dir 'proxydata')|Out-Null
$config=[IO.File]::ReadAllText("$run/node-30455.toml").Replace("$run/data-30455","$run/$Case/proxydata")
[IO.File]::WriteAllText((Join-Path $dir 'proxy.toml'),$config,[Text.UTF8Encoding]::new($false))
$nodeId=[int][IO.File]::ReadAllText("$run/owned-node.pid");$owner=(Invoke-CimMethod -InputObject (Get-CimInstance Win32_Process -Filter "ProcessId=$nodeId") -MethodName GetOwnerSid).Sid
if($owner -ne [Security.Principal.WindowsIdentity]::GetCurrent().User.Value){throw 'backend owner SID mismatch'}
$proxy=Start-Process "$run/target/debug/independent-owner-ipc-delay-r20.exe" -ArgumentList @($Case) -WindowStyle Hidden -PassThru -RedirectStandardOutput "$dir/proxy.out" -RedirectStandardError "$dir/proxy.err"
[IO.File]::WriteAllText((Join-Path $dir 'proxy.pid'),[string]$proxy.Id)
for($i=0;$i -lt 40;$i++){if(Test-Path -LiteralPath (Join-Path $dir 'proxy-state.json')){break};Start-Sleep -Milliseconds 100}
$keeper=Start-Process powershell -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File','D:/Project/acp-remote/testing/probes/establish-node-access-approval-cancel-r20-20261030/keeper.ps1','-Case',$Case) -WindowStyle Hidden -PassThru
[IO.File]::WriteAllText((Join-Path $dir 'launcher.pid'),[string]$keeper.Id)
Start-Sleep -Milliseconds 650
Write-Output "owned case started: $Case"
