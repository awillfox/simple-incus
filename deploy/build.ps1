# Builds the production artifacts from Windows:
#   target/site/                                          (hydrate wasm + js + css)
#   target/x86_64-unknown-linux-gnu/release/simple-incus  (ELF x86-64 server)
# Requires: rustup targets wasm32-unknown-unknown + x86_64-unknown-linux-gnu,
#           cargo-leptos, cargo-zigbuild, zig on PATH.
# (cargo-leptos cannot drive zigbuild itself: it appends "build" to the command,
#  which `cargo zigbuild` rejects. So the frontend and server are built separately.)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)

if (-not (Get-Command zig -ErrorAction SilentlyContinue)) {
    $z = Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\zig.zig*" -Recurse -Filter zig.exe -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($z) { $env:PATH += ";" + $z.DirectoryName } else { throw 'zig not found on PATH' }
}

cargo leptos build --release --frontend-only
if ($LASTEXITCODE -ne 0) { throw 'frontend build failed' }
cargo zigbuild --release --target x86_64-unknown-linux-gnu --no-default-features --features ssr --bin simple-incus
if ($LASTEXITCODE -ne 0) { throw 'server build failed' }

$bin = 'target/x86_64-unknown-linux-gnu/release/simple-incus'
$magic = [System.IO.File]::ReadAllBytes($bin)[0..3] -join ','
if ($magic -ne '127,69,76,70') { throw "not an ELF binary: $magic" }
Write-Host "OK: $bin (ELF), site in target/site"
