$ErrorActionPreference='Stop'
$root='D:/Project/acp-remote/.tmp/budget-acceptance-20261030'
$address=([IO.File]::ReadAllText("$root/pipe.json")|ConvertFrom-Json).pipe
$name=$address.Substring('\\.\pipe\'.Length)
$pipe=[IO.Pipes.NamedPipeClientStream]::new('.',$name,[IO.Pipes.PipeDirection]::InOut,[IO.Pipes.PipeOptions]::Asynchronous,[Security.Principal.TokenImpersonationLevel]::Impersonation)
try{
 $pipe.Connect(5000)
 $writer=[IO.StreamWriter]::new($pipe,[Text.UTF8Encoding]::new($false),4096,$true);$writer.AutoFlush=$true;$writer.NewLine="`n"
 $reader=[IO.StreamReader]::new($pipe,[Text.UTF8Encoding]::new($false),$false,4096,$true)
 $id=[Convert]::ToBase64String([Guid]::NewGuid().ToByteArray()).TrimEnd('=').Replace('+','-').Replace('/','_')
 $clock=[Diagnostics.Stopwatch]::StartNew()
 $writer.WriteLine((@{v=1;requestId=$id;command='device.pair'}|ConvertTo-Json -Compress))
 $started=$reader.ReadLine();if(-not $started){throw 'No pairing start'}
 [Console]::Out.WriteLine((@{kind='started';elapsedMs=$clock.ElapsedMilliseconds;wallUnixMs=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds();response=($started|ConvertFrom-Json)}|ConvertTo-Json -Depth 8 -Compress));[Console]::Out.Flush()
 $next=$reader.ReadLineAsync()
 while(-not $next.IsCompleted){if($clock.ElapsedMilliseconds -gt 310000){throw 'No terminal event within 310 seconds'};Start-Sleep -Milliseconds 100}
 $event=$next.GetAwaiter().GetResult()
 [Console]::Out.WriteLine((@{kind='event';elapsedMs=$clock.ElapsedMilliseconds;wallUnixMs=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds();response=($event|ConvertFrom-Json)}|ConvertTo-Json -Depth 8 -Compress));[Console]::Out.Flush()
 $end=$reader.ReadLineAsync();if(-not $end.Wait(5000)){throw 'IPC did not close after event'}
 [Console]::Out.WriteLine((@{kind='closed';elapsedMs=$clock.ElapsedMilliseconds;eof=($null -eq $end.Result)}|ConvertTo-Json -Compress));[Console]::Out.Flush()
}finally{$pipe.Dispose()}
