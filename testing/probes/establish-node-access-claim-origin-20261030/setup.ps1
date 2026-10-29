$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/claim-origin-acceptance-20261030'
if(Test-Path -LiteralPath $run){throw 'Owned runtime already exists'}
$source='D:/Project/acp-remote/.tmp/acceptance-20261030-r5-target/debug/acp-remote.exe'
$expected='AB97418E98D67A71E25BBF62B1E780F2F767530CA4290FB9C78E220A222A6024'
if((Get-FileHash -LiteralPath $source).Hash -ne $expected){throw 'Immutable binary mismatch'}
New-Item -ItemType Directory -Path $run,"$run/bin","$run/evidence" | Out-Null
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
& icacls.exe $run /inheritance:r /grant:r "*$($sid):(OI)(CI)(F)" | Out-Null
if($LASTEXITCODE -ne 0){throw 'owner ACL failed'}
Copy-Item -LiteralPath $source -Destination "$run/bin/acp-remote.exe"
if((Get-FileHash -LiteralPath "$run/bin/acp-remote.exe").Hash -ne $expected){throw 'Copied binary mismatch'}
foreach($port in @(30455,30456)){
 if(Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue){throw 'Reserved port occupied'}
 $config=@"
data_dir = "$run/data-$port"
listen = "100.119.123.57:$port"
canonical_origin = "https://faraday.tail74842e.ts.net:$port"
tls_cert = "D:/Project/acp-remote/.tmp/establish-node-access-tailscale-20261029/faraday.tail74842e.ts.net.crt"
tls_key = "D:/Project/acp-remote/.tmp/establish-node-access-tailscale-20261029/faraday.tail74842e.ts.net.key"
"@
 [IO.File]::WriteAllText("$run/node-$port.toml",$config,[Text.UTF8Encoding]::new($false))
}
$p=Start-Process "$run/bin/acp-remote.exe" -ArgumentList @('--config',"$run/node-30455.toml",'daemon','start') -WindowStyle Hidden -PassThru -RedirectStandardOutput "$run/node.out" -RedirectStandardError "$run/node.err"
[IO.File]::WriteAllText("$run/owned-node.pid",[string]$p.Id)
Start-Sleep -Seconds 1
& "$run/bin/acp-remote.exe" --config "$run/node-30455.toml" daemon status
if($LASTEXITCODE -ne 0){throw 'Node startup failed'}
Write-Output "owned node pid=$($p.Id); binary verified; port=30455"
