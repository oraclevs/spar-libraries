$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$hostTriple = (rustc -vV | Select-String '^host: ').ToString().Substring(6)
if ($hostTriple -ne 'x86_64-pc-windows-msvc') { throw "Unsupported native package target: $hostTriple" }
New-Item -ItemType Directory -Force native/windows_x86_64_msvc | Out-Null
Copy-Item target/release/rust_tcp.dll native/windows_x86_64_msvc/rust_tcp.dll
Write-Output 'built native/windows_x86_64_msvc/rust_tcp.dll'
