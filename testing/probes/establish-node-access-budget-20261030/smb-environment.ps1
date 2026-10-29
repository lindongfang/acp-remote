$ErrorActionPreference='Stop'
$root='D:/Project/acp-remote/.tmp/budget-acceptance-20261030'
$result=@{utc=[DateTime]::UtcNow.ToString('o');computerName=[Environment]::MachineName;scope='local-machine SMB loopback only; no other SID or remote machine';configurationChanges=$false;errors=@()}
try{
 $service=Get-Service -Name LanmanServer -ErrorAction Stop
 $result.lanmanServer=@{name=$service.Name;status=[string]$service.Status;startType=[string]$service.StartType}
}catch{$result.errors+=@{check='LanmanServer';type=$_.Exception.GetType().FullName;hresult=$_.Exception.HResult}}
try{
 $listeners=@(Get-NetTCPConnection -LocalPort 445 -State Listen -ErrorAction Stop)
 $result.port445Listeners=@($listeners|ForEach-Object {@{localAddress=$_.LocalAddress;localPort=$_.LocalPort;owningProcess=$_.OwningProcess}})
}catch{$result.port445Listeners=@();$result.errors+=@{check='445 listener query';type=$_.Exception.GetType().FullName;hresult=$_.Exception.HResult}}
$client=[Net.Sockets.TcpClient]::new()
$clock=[Diagnostics.Stopwatch]::StartNew()
try{
 $task=$client.ConnectAsync('127.0.0.1',445)
 if(-not $task.Wait(2000)){throw [TimeoutException]::new('Local 445 connect timeout')}
 $task.GetAwaiter().GetResult()
 $result.loopback445=@{connected=$client.Connected;elapsedMs=$clock.ElapsedMilliseconds;address='127.0.0.1';port=445}
}catch{
 $errorObject=$_.Exception
 while($errorObject.InnerException){$errorObject=$errorObject.InnerException}
 $result.loopback445=@{connected=$false;elapsedMs=$clock.ElapsedMilliseconds;address='127.0.0.1';port=445;errorType=$errorObject.GetType().FullName;hresult=$errorObject.HResult}
 if($errorObject -is [Net.Sockets.SocketException]){$result.loopback445.socketErrorCode=[string]$errorObject.SocketErrorCode;$result.loopback445.nativeErrorCode=$errorObject.NativeErrorCode}
}finally{$client.Dispose()}
if($result.loopback445.connected){
 try{
  $share=Get-SmbShare -Name 'IPC$' -ErrorAction Stop
  $result.ipcShare=@{present=$true;name=$share.Name;special=$share.Special}
 }catch{$result.ipcShare=@{present=$false;errorType=$_.Exception.GetType().FullName;hresult=$_.Exception.HResult}}
 $result.nextStep='SMB transport prerequisite present; dedicated positive-control fixture still required before any reject_remote claim'
}else{
 $result.nextStep='STOP: local SMB transport unavailable; no UNC pipe fixtures or product rejection inference'
}
[IO.File]::WriteAllText("$root/evidence/smb-environment.json",($result|ConvertTo-Json -Depth 7),[Text.UTF8Encoding]::new($false))
$result|ConvertTo-Json -Depth 7
