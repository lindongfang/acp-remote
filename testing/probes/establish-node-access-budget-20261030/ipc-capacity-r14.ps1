param([Parameter(Mandatory=$true)][ValidatePattern('^[A-Fa-f0-9]{64}$')][string]$ExpectedBinarySha256)
$ErrorActionPreference='Stop'
$root='D:/Project/acp-remote/.tmp/budget-acceptance-20261030'
$bin=[IO.Path]::GetFullPath("$root/bin/acp-remote.exe")
$hash=(Get-FileHash -LiteralPath $bin -Algorithm SHA256).Hash
if($hash -ne $ExpectedBinarySha256){throw 'Formal copied binary hash mismatch'}
if($hash -eq 'AB97418E98D67A71E25BBF62B1E780F2F767530CA4290FB9C78E220A222A6024'){throw 'Known failing R9 must not be retested with this regression'}
$ownedId=[int][IO.File]::ReadAllText("$root/owned-node.pid")
$owned=Get-Process -Id $ownedId -ErrorAction Stop
if($owned.Path -ne $bin){throw 'Owned process executable mismatch'}
$address=([IO.File]::ReadAllText("$root/pipe.json")|ConvertFrom-Json).pipe
$name=$address.Substring('\\.\pipe\'.Length)
$all=[Collections.Generic.List[IO.Pipes.NamedPipeClientStream]]::new()
$held=[Collections.Generic.List[object]]::new()
$clock=[Diagnostics.Stopwatch]::StartNew()
$evidence=@{binarySha256=$hash;nodePid=$ownedId;checks=@();passed=$false}
function Check([string]$id,$actual,[bool]$pass){
 $evidence.checks+=@{id=$id;elapsedMs=$clock.ElapsedMilliseconds;actual=$actual;pass=$pass}
 if(-not $pass){throw "Regression assertion failed: $id"}
}
function New-OwnerPipe{
 $s=[IO.Pipes.NamedPipeClientStream]::new('.',$name,[IO.Pipes.PipeDirection]::InOut,[IO.Pipes.PipeOptions]::Asynchronous,[Security.Principal.TokenImpersonationLevel]::Impersonation)
 $all.Add($s);$s.Connect(3000);return ,$s
}
function Write-Bytes($stream,[string]$text){$bytes=[Text.Encoding]::UTF8.GetBytes($text);$stream.Write($bytes,0,$bytes.Length);$stream.Flush()}
function Complete-Status($entry){
 $id=[Convert]::ToBase64String([Guid]::NewGuid().ToByteArray()).TrimEnd('=').Replace('+','-').Replace('/','_')
 $body=@{v=1;requestId=$id;command='status'}|ConvertTo-Json -Compress
 Write-Bytes $entry.stream ($body.Substring(1)+"`n")
 if(-not $entry.read.Wait(3000)){return $false}
 if($entry.read.IsFaulted -or -not $entry.read.Result){return $false}
 $response=$entry.read.Result|ConvertFrom-Json
 return ($response.ok -and $response.requestId -eq $id -and $response.status.state -eq 'ready')
}
try{
 foreach($number in 1..32){
  $s=New-OwnerPipe;Write-Bytes $s '{'
  $reader=[IO.StreamReader]::new($s,[Text.UTF8Encoding]::new($false),$false,4096,$true)
  $held.Add(@{stream=$s;reader=$reader;read=$reader.ReadLineAsync()})
  if($number -eq 31 -or $number -eq 32){
   Start-Sleep -Milliseconds 100
   $ended=@($held|Where-Object {$_.read.IsCompleted}).Count
   Check "active-$number" @{connected=$held.Count;ended=$ended} ($ended -eq 0)
  }
 }
 # OS-level connect must succeed. A Connect failure cannot count as application admission rejection.
 $over=New-OwnerPipe
 $overReader=[IO.StreamReader]::new($over,[Text.UTF8Encoding]::new($false),$false,4096,$true)
 $overRead=$overReader.ReadLineAsync()
 $overClosed=$false
 try{if($overRead.Wait(2000)){$overClosed=($null -eq $overRead.Result)}}catch [AggregateException]{
  $overClosed=($overRead.IsFaulted -and $overRead.Exception.InnerException -is [IO.IOException])
 }
 $ended=@($held|Where-Object {$_.read.IsCompleted}).Count
 Check 'overbudget-33' @{osConnected=$true;closedWithoutResponse=$overClosed;existingEnded=$ended} ($overClosed -and $ended -eq 0)
 $over.Dispose()
 # Complete one admitted request to release its permit, then prove a new request can be admitted.
 Check 'release-one-existing-status' @{index=0} (Complete-Status $held[0])
 $held[0].stream.Dispose()
 Start-Sleep -Milliseconds 100
 $replacement=New-OwnerPipe
 $replacementReader=[IO.StreamReader]::new($replacement,[Text.UTF8Encoding]::new($false),$false,4096,$true)
 Write-Bytes $replacement '{'
 $replacementEntry=@{stream=$replacement;reader=$replacementReader;read=$replacementReader.ReadLineAsync()}
 Check 'admission-recovers-after-release' @{otherActive=31} (Complete-Status $replacementEntry)
 $replacement.Dispose()
 $successful=0
 foreach($entry in $held.GetRange(1,31)){if(Complete-Status $entry){$successful++};$entry.stream.Dispose()}
 Check 'all-original-clients-survive' @{successfulStatusResponses=($successful+1);expected=32} ($successful -eq 31)
 $owned.Refresh()
 $listeners=@(Get-NetTCPConnection -LocalPort 30457 -State Listen -ErrorAction SilentlyContinue)
 Check 'node-still-ready' @{alive=(-not $owned.HasExited);listenerCount=$listeners.Count;owningProcess=@($listeners|ForEach-Object OwningProcess)} ((-not $owned.HasExited) -and $listeners.Count -eq 1 -and $listeners[0].OwningProcess -eq $ownedId)
 $evidence.passed=$true
}catch{
 $evidence.failureType=$_.Exception.GetType().FullName
 throw
}finally{
 foreach($s in $all){$s.Dispose()}
 $evidence.totalElapsedMs=$clock.ElapsedMilliseconds
 [IO.File]::WriteAllText("$root/evidence/ipc-capacity-r14.json",($evidence|ConvertTo-Json -Depth 8))
}
