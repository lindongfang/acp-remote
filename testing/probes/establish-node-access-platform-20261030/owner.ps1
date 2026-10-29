$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/platform-acceptance-20261030'
$bin='D:/Project/acp-remote/.tmp/acceptance-20261030-target/debug/acp-remote.exe'
if(Test-Path $run){throw 'Refusing existing run directory'}
New-Item -ItemType Directory -Path $run,"$run/profile","$run/target" | Out-Null
$config=Get-Content -Raw 'D:/Project/acp-remote/.tmp/establish-node-access-tailscale-20261029/acp-remote.toml'
$config=$config.Replace('D:/Project/acp-remote/.tmp/establish-node-access-tailscale-20261029/data',"$run/data").Replace('100.119.123.57:28444','127.0.0.1:30453').Replace(':28444',':30453')
[IO.File]::WriteAllText("$run/node.toml",$config)
[IO.File]::WriteAllText("$run/node2.toml",$config.Replace('30453','30454'))
whoami /user
icacls $run /grant 'faraday\codexsandboxoffline:(OI)(CI)(RX)'
$p=Start-Process $bin -ArgumentList @('--config',"$run/node.toml",'daemon','start') -PassThru -WindowStyle Hidden -RedirectStandardOutput "$run/node.out" -RedirectStandardError "$run/node.err"
$p.Id | Set-Content "$run/owned.pid"
Start-Sleep -Seconds 2
if($p.HasExited){Get-Content "$run/node.err";throw 'Node failed'}
& $bin --config "$run/node.toml" daemon status
"status_exit=$LASTEXITCODE"
& $bin --config "$run/node.toml" device list
"list_exit=$LASTEXITCODE"
& $bin --config "$run/node.toml" device revoke --device-id AAAAAAAAAAAAAAAAAAAAAA
"revoke_exit=$LASTEXITCODE"
& $bin --config "$run/node2.toml" daemon start
"second_instance_exit=$LASTEXITCODE"
icacls "$run/data"
