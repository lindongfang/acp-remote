$ErrorActionPreference='Stop'
$root='D:/Project/acp-remote/.tmp/budget-acceptance-20261030'
$bin=[IO.Path]::GetFullPath("$root/bin/acp-remote-r17.exe")
$expected='38AA9D02F334E3C81FDC900ED4F8943BB78339650981BC0046A609381D03E6C2'
if((Get-FileHash $bin).Hash -ne $expected){throw 'Copied R17 mismatch'}
if(Get-NetTCPConnection -LocalPort 30457 -State Listen -ErrorAction SilentlyContinue){throw 'Owned port occupied'}
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class BudgetSmbNative {
 [StructLayout(LayoutKind.Sequential)] public struct SA { public int length; public IntPtr descriptor; [MarshalAs(UnmanagedType.Bool)] public bool inherit; }
 [DllImport("advapi32.dll",CharSet=CharSet.Unicode,SetLastError=true)] public static extern bool ConvertStringSecurityDescriptorToSecurityDescriptor(string s, uint revision, out IntPtr sd, out uint size);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] public static extern SafePipeHandle CreateNamedPipe(string name,uint access,uint mode,uint instances,uint output,uint input,uint timeout,ref SA security);
 [DllImport("kernel32.dll")] public static extern IntPtr LocalFree(IntPtr value);
}
'@
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$name='acp-remote.budget-smb-positive.'+[Guid]::NewGuid().ToString('N')
$result=@{utc=[DateTime]::UtcNow.ToString('o');binarySha256=$expected;scope='same-owner local-machine loopback SMB, not another SID or machine';server='127.0.0.1';passed=$false}
$descriptor=[IntPtr]::Zero;$server=$null;$client=$null;$remote=$null;$cts=$null;$node=$null
try{
 $config="$root/node-30457.toml"
 $node=Start-Process $bin -ArgumentList @('--config',$config,'daemon','start') -WindowStyle Hidden -PassThru -RedirectStandardOutput "$root/smb-node.out" -RedirectStandardError "$root/smb-node.err"
 [IO.File]::WriteAllText("$root/smb-node.pid",[string]$node.Id)
 Start-Sleep -Milliseconds 500
 $local=(& $bin --config $config daemon status 2>&1|Out-String)
 if($LASTEXITCODE -ne 0 -or $local -notmatch 'state=ready'){throw 'Product local status not ready'}
 $result.productLocalStatusReady=$true
 [uint32]$size=0
 if(-not [BudgetSmbNative]::ConvertStringSecurityDescriptorToSecurityDescriptor("D:P(A;;GA;;;$sid)",1,[ref]$descriptor,[ref]$size)){throw 'SDDL conversion failed'}
 $attributes=[BudgetSmbNative+SA]::new();$attributes.length=[Runtime.InteropServices.Marshal]::SizeOf($attributes);$attributes.descriptor=$descriptor;$attributes.inherit=$false
 # Byte/wait mode 0 explicitly omits PIPE_REJECT_REMOTE_CLIENTS (0x8).
 $handle=[BudgetSmbNative]::CreateNamedPipe("\\.\pipe\$name",0x40000003,0,1,1024,1024,0,[ref]$attributes)
 if($handle.IsInvalid){throw "Positive pipe creation failed $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"}
 $server=[IO.Pipes.NamedPipeServerStream]::new([IO.Pipes.PipeDirection]::InOut,$true,$false,$handle)
 $acl=$server.GetAccessControl();$rules=$acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])
 $allow=@($rules|Where-Object {$_.AccessControlType -eq 'Allow'}|ForEach-Object {$_.IdentityReference.Value}|Select-Object -Unique)
 if($allow.Count -ne 1 -or $allow[0] -ne $sid){throw 'Positive pipe not owner-only'}
 $result.positiveDaclOwnerOnly=$true;$result.positiveRejectRemoteFlag=$false
 $cts=[Threading.CancellationTokenSource]::new();$cts.CancelAfter(4000)
 $accept=$server.WaitForConnectionAsync($cts.Token)
 $client=[IO.Pipes.NamedPipeClientStream]::new('127.0.0.1',$name,[IO.Pipes.PipeDirection]::InOut,[IO.Pipes.PipeOptions]::Asynchronous,[Security.Principal.TokenImpersonationLevel]::Impersonation)
 $watch=[Diagnostics.Stopwatch]::StartNew();$client.Connect(2500)
 if(-not $accept.Wait(2500)){throw 'Positive accept timeout'}
 $client.WriteByte(81);$client.Flush();$buffer=[byte[]]::new(1);$read=$server.ReadAsync($buffer,0,1);if(-not $read.Wait(2000) -or $read.Result -ne 1 -or $buffer[0] -ne 81){throw 'Positive client-to-server failed'}
 $server.WriteByte(82);$server.Flush();$back=[byte[]]::new(1);$read=$client.ReadAsync($back,0,1);if(-not $read.Wait(2000) -or $read.Result -ne 1 -or $back[0] -ne 82){throw 'Positive server-to-client failed'}
 $result.positive=@{unc="\\127.0.0.1\pipe\$name";connected=$true;clientToServerByte=81;serverToClientByte=82;elapsedMs=$watch.ElapsedMilliseconds}
 $product=([IO.File]::ReadAllText("$root/pipe.json")|ConvertFrom-Json).pipe.Substring('\\.\pipe\'.Length)
 $remote=[IO.Pipes.NamedPipeClientStream]::new('127.0.0.1',$product,[IO.Pipes.PipeDirection]::InOut,[IO.Pipes.PipeOptions]::Asynchronous,[Security.Principal.TokenImpersonationLevel]::Impersonation)
 $watch.Restart();$denied=$false
 try{$remote.Connect(2500);$result.productRemote=@{connected=$true;elapsedMs=$watch.ElapsedMilliseconds}}
 catch{
  $errorObject=$_.Exception;while($errorObject.InnerException){$errorObject=$errorObject.InnerException}
  $denied=$errorObject -is [UnauthorizedAccessException]
  $result.productRemote=@{unc="\\127.0.0.1\pipe\$product";connected=$false;elapsedMs=$watch.ElapsedMilliseconds;errorType=$errorObject.GetType().FullName;hresult=$errorObject.HResult}
 }
 $result.passed=$denied
 if(-not $denied){throw 'Product UNC open did not produce access denied after positive success'}
}catch{
 $result.failureType=$_.Exception.GetType().FullName
 throw
}finally{
 if($remote){$remote.Dispose()};if($client){$client.Dispose()};if($cts){$cts.Cancel()};if($server){$server.Dispose()};if($cts){$cts.Dispose()};if($descriptor -ne [IntPtr]::Zero){[BudgetSmbNative]::LocalFree($descriptor)|Out-Null}
 $result.fixtureDisposed=$true
 if($node){
  $node.Refresh()
  if(-not $node.HasExited){if($node.Path -ne $bin){throw 'Owned node PID mismatch'};& $bin --config "$root/node-30457.toml" daemon stop | Out-Null;if($LASTEXITCODE -ne 0 -or -not $node.WaitForExit(10000)){throw 'Owned SMB node normal stop failed'}}
  $result.productNodeExited=$node.HasExited
 }
 $result.remainingListenerCount=@(Get-NetTCPConnection -LocalPort 30457 -State Listen -ErrorAction SilentlyContinue).Count
 $result.positivePipeStillListed=@([IO.Directory]::GetFiles('\\.\pipe\')) -contains "\\.\pipe\$name"
 [IO.File]::WriteAllText("$root/evidence/smb-pipe-control.json",($result|ConvertTo-Json -Depth 7),[Text.UTF8Encoding]::new($false))
 $result|ConvertTo-Json -Depth 7
}
