$ErrorActionPreference='Stop'
$root='D:/Project/acp-remote/.tmp/budget-acceptance-20261030'
$address=([IO.File]::ReadAllText("$root/pipe.json")|ConvertFrom-Json).pipe
$name=$address.Substring('\\.\pipe\'.Length)
$streams=[Collections.Generic.List[IO.Pipes.NamedPipeClientStream]]::new()
$tasks=[Collections.Generic.List[Threading.Tasks.Task[string]]]::new()
$clock=[Diagnostics.Stopwatch]::StartNew()
$observations=@()
try{
 foreach($number in 1..32){
  $stream=[IO.Pipes.NamedPipeClientStream]::new('.',$name,[IO.Pipes.PipeDirection]::InOut,[IO.Pipes.PipeOptions]::Asynchronous,[Security.Principal.TokenImpersonationLevel]::Impersonation)
  try{
   $stream.Connect(3000);$streams.Add($stream)
   $stream.Write([byte[]]@(123),0,1);$stream.Flush()
   $reader=[IO.StreamReader]::new($stream,[Text.UTF8Encoding]::new($false),$false,4096,$true)
   $tasks.Add($reader.ReadLineAsync())
  }catch{
   $observations+=@{connections=$number;error=$_.Exception.GetType().FullName;elapsedMs=$clock.ElapsedMilliseconds}
   break
  }
  if($number -ge 30){Start-Sleep -Milliseconds 100;$observations+=@{connections=$number;ended=@($tasks|Where-Object IsCompleted).Count;elapsedMs=$clock.ElapsedMilliseconds}}
 }
 Start-Sleep -Milliseconds 2500
 $ownedId=[int][IO.File]::ReadAllText("$root/owned-node.pid")
 $node=Get-Process -Id $ownedId -ErrorAction SilentlyContinue
 if($node -and $node.Path -ne [IO.Path]::GetFullPath("$root/bin/acp-remote.exe")){throw 'PID path mismatch'}
 $result=@{observations=$observations;endedAfterSettling=@($tasks|Where-Object IsCompleted).Count;elapsedMs=$clock.ElapsedMilliseconds;nodePid=$ownedId;nodeAlive=($null -ne $node);listenerCount=@(Get-NetTCPConnection -LocalPort 30457 -State Listen -ErrorAction SilentlyContinue).Count}
 [IO.File]::WriteAllText("$root/evidence/ipc-capacity.json",($result|ConvertTo-Json -Depth 5))
 $result|ConvertTo-Json -Depth 5
}finally{foreach($stream in $streams){$stream.Dispose()}}
