$ErrorActionPreference='Stop'
$run=(Resolve-Path -LiteralPath 'D:/Project/acp-remote/.tmp/budget-acceptance-20261030').Path
$bin=Join-Path $run 'bin/acp-remote.exe'
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$results=@()
foreach($port in @(30457,30458)){
 $pidFile=if($port -eq 30457){Join-Path $run 'owned-node.pid'}else{Join-Path $run 'owned-node-30458.pid'}
 $targetId=[int][IO.File]::ReadAllText($pidFile)
 $target=Get-Process -Id $targetId -ErrorAction SilentlyContinue
 if($target -and $target.Path -ne $bin){throw 'not owned executable'}
 $data=(Resolve-Path -LiteralPath (Join-Path $run "data-$port")).Path
 if(-not $data.StartsWith($run+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'outside runtime'}
 $acl=Get-Acl -LiteralPath $data
 $allowSids=@($acl.Access|Where-Object{$_.AccessControlType -eq 'Allow'}|ForEach-Object{$_.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value}|Select-Object -Unique)
 if($allowSids.Count -ne 1 -or $allowSids[0] -ne $sid){throw 'unexpected data directory ACL'}
 $canonical='\\?\'+$data
 $bytes=[byte[]]([Text.Encoding]::UTF8.GetBytes("acp-remote/local-ipc/v1`0$sid`0")+[Text.Encoding]::Unicode.GetBytes($canonical))
 $sha=[Security.Cryptography.SHA256]::Create();try{$digest=([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-','').ToLower()}finally{$sha.Dispose()}
 $pipe='\\.\pipe\acp-remote.'+$digest
 $before=@([IO.Directory]::GetFiles('\\.\pipe\')) -contains $pipe
 if($target -and -not $before){throw 'owned live pipe address not confirmed before cleanup'}
 if($target){
 & $bin --config (Join-Path $run "node-$port.toml") daemon stop
 if($LASTEXITCODE -ne 0 -or -not $target.WaitForExit(10000)){throw 'owned node did not stop cleanly'}
 }
 $listener=@(Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue).Count
 $after=@([IO.Directory]::GetFiles('\\.\pipe\')) -contains $pipe
 $stream=[IO.File]::Open((Join-Path $data '.node.lock'),[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None);$stream.Dispose()
 if($listener -ne 0 -or $after){throw 'owned resource still live'}
 $results+=@{port=$port;pid=$targetId;exited=$true;alreadyExitedBeforeCleanup=($null -eq $target);listenerCount=$listener;pipeConfirmedBefore=$before;pipeAbsentAfter=(-not $after);lockExclusivelyOpened=$true;ownerOnlyDataAcl=$true}
}
[IO.File]::WriteAllText((Join-Path $run 'evidence/cleanup-r9.json'),($results|ConvertTo-Json),[Text.UTF8Encoding]::new($false))
$results|ConvertTo-Json
