$ErrorActionPreference = 'Stop'
$run = 'D:/Project/acp-remote/.tmp/cross-sid-r20-20261030'
$evidence = "$run/abc-evidence"
$expectedSid = 'S-1-5-21-1850245178-3063344048-767428509-1006'
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
if ($sid -ne $expectedSid) { throw "Wrong execution SID: $sid" }
if (Test-Path "$evidence/result.json") { throw 'Refusing repeated run' }
$hash = (Get-FileHash "$run/shared/acp-remote.exe").Hash
if ($hash -ne '561A6024E983DD5239C09D1388CA5D256625F690EE9345E64B9654BC43AAFBE3') { throw 'Binary mismatch' }
Add-Type -Path "$run/shared/ConsoleRunner.cs"
$rows = @()
foreach ($command in @('device pair', 'daemon stop')) {
    $rows += [ConsoleRunner]::Run("$run/shared/acp-remote.exe", "--config `"$run/shared/node.toml`" $command", 15000)
}
$result = [ordered]@{sid=$sid; user=[Security.Principal.WindowsIdentity]::GetCurrent().Name; binaryHash=$hash; utc=[DateTime]::UtcNow.ToString('o'); commands=$rows}
$result | ConvertTo-Json -Depth 6 | Set-Content -Encoding UTF8 "$evidence/result.json"
if (@($rows | Where-Object { $_.TimedOut -or $_.ExitCode -eq 0 -or -not $_.ConsoleInput -or -not $_.ConsoleOutput -or $_.Output -notmatch 'os error 5|Access is denied' -or $_.Output -match 'requires an interactive terminal|pairingCode=' }).Count) { exit 1 }
exit 0
