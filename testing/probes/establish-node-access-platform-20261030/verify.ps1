$ErrorActionPreference='Stop'
$manifest='testing/runs/establish-node-access/2026-10-30-implementation-r4.sha256'
if((Get-FileHash $manifest).Hash -ne 'A1A7F70D36CD1EA7BDA79A25E1F6FFA864487EE7CE046818AB963144761AEA53'){throw 'Manifest mismatch'}
$lines=Get-Content $manifest
if($lines.Count -ne 79){throw 'Manifest count mismatch'}
foreach($line in $lines){
    $expected,$path=$line -split '  ',2
    if((Get-FileHash -LiteralPath $path).Hash -ne $expected){throw "Source mismatch: $path"}
}
if((Get-FileHash '.tmp/acceptance-20261030-target/debug/acp-remote.exe').Hash -ne '6F0C92B02DEAC8A6857C3834B2309565A00109ACA49CEDF58AF4986E08F1D302'){throw 'Binary mismatch'}
'Frozen manifest 79/79 and binary PASS'
$env:CARGO_TARGET_DIR='D:/Project/acp-remote/.tmp/platform-acceptance-20261030/target'
foreach($filter in @('platform::windows::tests::named_pipe_enforces_owner_dacl_sid_and_first_instance','identity::tests::','platform::windows::tests::dpapi_round_trip_uses_current_user_scope')){
    cargo test --manifest-path app/Cargo.toml --locked --lib $filter
    if($LASTEXITCODE -ne 0){throw "Test failed: $filter"}
}
