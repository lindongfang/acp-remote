param(
    [string]$Binary = 'D:\Project\acp-remote\.tmp\independent-acceptance-20261030\target\debug\acp-remote.exe',
    [string]$Config = 'D:\Project\acp-remote\.tmp\protocol-acceptance-20261030\node.toml'
)
$ErrorActionPreference = 'Stop'
# Run in a real PTY as the node owner. The protocol peer claims the printed code.
& $Binary --config $Config device pair
$pairExit = $LASTEXITCODE
Write-Output "PAIR_EXIT_CODE=$pairExit"
exit $pairExit
