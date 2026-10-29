param([string]$Label='restart')
$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030'
& "$run/bin/acp-remote.exe" --config "$run/node.toml" daemon stop
"stop_exit=$LASTEXITCODE"
Start-Sleep -Seconds 1
& "$run/bin/acp-remote.exe" --config "$run/node.toml" daemon start
