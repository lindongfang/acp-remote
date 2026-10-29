param([ValidateSet('pair','short','size','hold')][string]$Mode='pair',[int]$Bytes=0)
$ErrorActionPreference='Stop'
$root='D:/Project/acp-remote/.tmp/budget-acceptance-20261030'
$address=([IO.File]::ReadAllText("$root/pipe.json")|ConvertFrom-Json).pipe
$name=$address.Substring('\\.\pipe\'.Length)
$pipe=[IO.Pipes.NamedPipeClientStream]::new('.', $name,[IO.Pipes.PipeDirection]::InOut,[IO.Pipes.PipeOptions]::Asynchronous,[Security.Principal.TokenImpersonationLevel]::Impersonation)
try{
 $pipe.Connect(5000)
 $writer=[IO.StreamWriter]::new($pipe,[Text.UTF8Encoding]::new($false),4096,$true);$writer.AutoFlush=$true;$writer.NewLine="`n"
 $reader=[IO.StreamReader]::new($pipe,[Text.UTF8Encoding]::new($false),$false,4096,$true)
 if($Mode -eq 'short'){
  $clock=[Diagnostics.Stopwatch]::StartNew();$writer.Write('{');$task=$reader.ReadLineAsync()
  Start-Sleep -Milliseconds 29000
  $before=@{elapsedMs=$clock.ElapsedMilliseconds;ended=$task.IsCompleted}
  if(-not $task.Wait(4000)){throw 'short request did not end by 33 seconds'}
  @{before=$before;closedElapsedMs=$clock.ElapsedMilliseconds;response=$task.Result}|ConvertTo-Json -Compress
 }elseif($Mode -eq 'size'){
  $base=@{v=1;requestId=[Convert]::ToBase64String([Guid]::NewGuid().ToByteArray()).TrimEnd('=').Replace('+','-').Replace('/','_');command='status'}|ConvertTo-Json -Compress
  $body=$base+(' '*($Bytes-[Text.Encoding]::UTF8.GetByteCount($base)))
  $clock=[Diagnostics.Stopwatch]::StartNew();$writer.WriteLine($body);$task=$reader.ReadLineAsync();if(-not $task.Wait(5000)){throw 'size response timeout'}
  @{bodyBytes=$Bytes;wireBytes=($Bytes+1);response=$task.Result;elapsedMs=$clock.ElapsedMilliseconds}|ConvertTo-Json -Compress
 }elseif($Mode -eq 'hold'){
  $writer.Write('{');[Console]::Out.WriteLine('{"connected":true}');[Console]::Out.Flush();[Console]::In.ReadLine()|Out-Null
 }else{
  $requestId=[Convert]::ToBase64String([Guid]::NewGuid().ToByteArray()).TrimEnd('=').Replace('+','-').Replace('/','_')
  $writer.WriteLine((@{v=1;requestId=$requestId;command='device.pair'}|ConvertTo-Json -Compress))
  $line=$reader.ReadLine();if(-not $line){throw 'no pair response'};[Console]::Out.WriteLine($line);[Console]::Out.Flush()
  # The harness decides when to claim and approve; stdout is private to parent.
  $instruction=[Console]::In.ReadLine()
  if($instruction -eq 'next'){
   [Console]::Out.WriteLine($reader.ReadLine());[Console]::Out.Flush()
   $instruction=[Console]::In.ReadLine()
  }
  if($instruction -eq 'approve' -or $instruction -eq 'reject'){
   $writer.WriteLine((@{v=1;requestId=$requestId;decision=$instruction}|ConvertTo-Json -Compress))
   [Console]::Out.WriteLine($reader.ReadLine());[Console]::Out.Flush()
  }
 }
}finally{$pipe.Dispose()}
