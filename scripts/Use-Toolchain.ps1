$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$privateCargo = Join-Path $taskRoot 'research/build-environment/artifacts/cargo'
$privateRustup = Join-Path $taskRoot 'research/build-environment/artifacts/rustup'
if (Test-Path -LiteralPath "$privateCargo/bin/cargo.exe") {
    $env:CARGO_HOME = $privateCargo
    $env:RUSTUP_HOME = $privateRustup
    $env:PATH = "$privateCargo/bin;$env:PATH"
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw 'Rust MSVC toolchain is required. Install from https://rustup.rs/ and retry.'
}
$localClang = Join-Path $taskRoot 'research/build-environment/artifacts/libclang-18.1.1'
if (-not $env:LIBCLANG_PATH -and (Test-Path -LiteralPath "$localClang/libclang.dll")) {
    $env:LIBCLANG_PATH = $localClang
}
