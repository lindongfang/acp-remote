$ErrorActionPreference='Stop'
$root='D:/Project/acp-remote/.tmp/budget-acceptance-20261030'
$frozen='D:/Project/acp-remote/.tmp/frozen-implementation-r17-20261030'
$manifest='D:/Project/acp-remote/testing/runs/establish-node-access/2026-10-30-implementation-r17.sha256'
$expectedManifest='5074890C209A702335F13042A26DBA64278EFD7A7B0A70A4E42E2F698BA3E26F'
$expectedBinary='38AA9D02F334E3C81FDC900ED4F8943BB78339650981BC0046A609381D03E6C2'
if((Get-FileHash $manifest).Hash -ne $expectedManifest){throw 'R17 manifest mismatch'}
$checked=0
foreach($line in [IO.File]::ReadAllLines($manifest)){
 if($line -notmatch '^([A-Fa-f0-9]{64})  (.+)$'){throw 'Unexpected manifest line'}
 $wanted=$Matches[1];$relative=$Matches[2]
 $path=Join-Path $frozen $relative
 if((Get-FileHash -LiteralPath $path).Hash -ne $wanted){throw "Frozen manifest entry mismatch: $relative"}
 $checked++
}
if($checked -ne 83){throw 'Unexpected manifest count'}
foreach($port in @(30457,30458)){if(Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue){throw 'Owned test port occupied'}}
$binary='D:/Project/acp-remote/.tmp/acceptance-20261030-r5-target/debug/acp-remote.exe'
if((Get-FileHash $binary).Hash -ne $expectedBinary){throw 'R17 source binary mismatch'}
Copy-Item -LiteralPath $binary -Destination "$root/bin/acp-remote-r17.exe"
if((Get-FileHash "$root/bin/acp-remote-r17.exe").Hash -ne $expectedBinary){throw 'R17 copied binary mismatch'}
$fixture="$root/r17-slow-writer-source"
if(Test-Path -LiteralPath $fixture){throw 'Dedicated source fixture already exists'}
New-Item -ItemType Directory -Path $fixture,"$fixture/clients","$fixture/clients/web" | Out-Null
Copy-Item -LiteralPath "$frozen/app" -Destination "$fixture/app" -Recurse
Copy-Item -LiteralPath "$frozen/clients/web/dist" -Destination "$fixture/clients/web/dist" -Recurse
Copy-Item -LiteralPath "$frozen/rust-toolchain.toml" -Destination "$fixture/rust-toolchain.toml"
$path="$fixture/app/src/node.rs"
$source=[IO.File]::ReadAllText($path)
$begin=$source.IndexOf('    async fn paused_real_writers_close_at_the_byte_or_frame_queue_budget()')
$end=$source.IndexOf('    async fn revoke_commit_discards_an_already_queued_management_result()', $begin)
if($begin -lt 0 -or $end -lt $begin){throw 'Expected frozen test boundary missing'}
$test=$source.Substring($begin,$end-$begin)
$test=$test.Replace('area.config(free_address())','area.config("127.0.0.1:30457".parse().expect("owned test address"))')
$test=$test.Replace('        for byte_first in [true, false] {',"        for byte_first in [true, false] {`n            let observation_started = std::time::Instant::now();")
$test=$test.Replace('            assert_eq!(code, 4429);',@'
            eprintln!("BUDGET_OBSERVATION byte_first={} bytes={} frames={} rejected_bytes={} queue_close_code={} elapsed_ms={}", byte_first, bytes, frames, rejected, code, observation_started.elapsed().as_millis());
            assert_eq!(code, 4429);
'@)
$test=$test.Replace('            assert_eq!(u16::from(close.code), 4429);',@'
            eprintln!("BUDGET_SOCKET_CLOSE byte_first={} code={} elapsed_ms={}", byte_first, u16::from(close.code), observation_started.elapsed().as_millis());
            assert_eq!(u16::from(close.code), 4429);
'@)
if(-not $test.Contains('BUDGET_OBSERVATION') -or $test.Contains('area.config(free_address())')){throw 'Adapter instrumentation failed'}
[IO.File]::WriteAllText($path,$source.Substring(0,$begin)+$test+$source.Substring($end),[Text.UTF8Encoding]::new($false))
@{manifestSha256=$expectedManifest;entriesChecked=$checked;binarySha256=$expectedBinary;testOnlyAdaptations=@('sole filtered test listen address 127.0.0.1:30457','stderr queue and socket-close observations');frozenNodeSha256=(Get-FileHash "$frozen/app/src/node.rs").Hash;adaptedNodeSha256=(Get-FileHash $path).Hash;managementSha256=(Get-FileHash "$fixture/app/src/management_ws.rs").Hash}|ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 "$root/evidence/r17-slow-writer-fixture.json"
$env:CARGO_TARGET_DIR="$root/target"
$env:TEMP="$root/temp";$env:TMP=$env:TEMP
# Native stderr is logged directly by cmd so PowerShell does not turn Cargo progress into an error.
$command='cargo test -j 2 --locked --manifest-path "'+$fixture+'/app/Cargo.toml" --lib node::tests::paused_real_writers_close_at_the_byte_or_frame_queue_budget -- --exact --nocapture > "'+$root+'/evidence/r17-slow-writer.log" 2>&1'
& cmd.exe /d /s /c $command
$result=$LASTEXITCODE
Get-Content -LiteralPath "$root/evidence/r17-slow-writer.log" -Tail 20
exit $result
