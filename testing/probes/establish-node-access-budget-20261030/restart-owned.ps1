$ErrorActionPreference='Stop'
$root='D:/Project/acp-remote/.tmp/budget-acceptance-20261030'
$bin="$root/bin/acp-remote.exe"
$ownedId=[int][IO.File]::ReadAllText("$root/owned-node.pid")
$owned=Get-Process -Id $ownedId -ErrorAction Stop
if($owned.Path -ne [IO.Path]::GetFullPath($bin)){throw 'owned PID mismatch'}
& $bin --config "$root/node-30457.toml" daemon stop | Out-Null
if($LASTEXITCODE -ne 0 -or -not $owned.WaitForExit(10000)){throw 'normal restart stop failed'}
$proc=Start-Process $bin -ArgumentList @('--config',"$root/node-30457.toml",'daemon','start') -WindowStyle Hidden -PassThru -RedirectStandardOutput "$root/restart-$ownedId.out" -RedirectStandardError "$root/restart-$ownedId.err"
[IO.File]::WriteAllText("$root/owned-node.pid",[string]$proc.Id)
Start-Sleep -Milliseconds 500
& $bin --config "$root/node-30457.toml" daemon status | Out-Null
if($LASTEXITCODE -ne 0){throw 'restart startup failed'}
