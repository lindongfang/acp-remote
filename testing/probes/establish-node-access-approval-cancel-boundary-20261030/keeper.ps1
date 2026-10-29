param([ValidateSet('committed','precommit','control')][string]$Case)
$ErrorActionPreference='Stop'
$run='D:/Project/acp-remote/.tmp/approval-cancel-boundary-20261030'
$dir=Join-Path $run $Case
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class KeeperControl {
 public delegate bool Handler(uint code);
 public static Handler Guard=code=>true;
 [DllImport("kernel32.dll",SetLastError=true)]public static extern bool SetConsoleCtrlHandler(Handler h,bool add);
}
'@
if(-not [KeeperControl]::SetConsoleCtrlHandler([KeeperControl]::Guard,$true)){throw 'keeper control guard failed'}
[IO.File]::WriteAllText((Join-Path $dir 'keeper.pid'),[string]$PID)
$start=[Diagnostics.ProcessStartInfo]::new()
$start.FileName="$run/bin/acp-remote.exe"
$start.Arguments='--config "'+$dir+'/proxy.toml" device pair'
$start.UseShellExecute=$false
$start.CreateNoWindow=$false
$child=[Diagnostics.Process]::Start($start)
[IO.File]::WriteAllText((Join-Path $dir 'cli.pid'),[string]$child.Id)
$child.WaitForExit();$child.Refresh()
[IO.File]::WriteAllText((Join-Path $dir 'cli-exit.json'),(@{exited=$true;exitCode=$child.ExitCode;pid=$child.Id}|ConvertTo-Json),[Text.UTF8Encoding]::new($false))
Write-Host 'OWNED_PAIR_FINISHED'
while(-not (Test-Path -LiteralPath (Join-Path $dir 'finish-keeper.flag'))){Start-Sleep -Milliseconds 100}
