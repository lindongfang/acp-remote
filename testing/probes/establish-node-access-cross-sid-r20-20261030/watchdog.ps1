$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/cross-sid-r20-20261030'
$owned=[int](Get-Content "$run/owner-evidence/node.pid")
Start-Sleep -Seconds 900
$p=Get-Process -Id $owned -ErrorAction SilentlyContinue
if($p -and [IO.Path]::GetFullPath($p.Path) -eq [IO.Path]::GetFullPath("$run/shared/acp-remote.exe")) {
    & "$run/shared/acp-remote.exe" --config "$run/shared/node.toml" daemon stop *> "$run/owner-evidence/watchdog-stop.txt"
}
