param([switch]$HostOnly)
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
$name = 'spar-tcp-0.1.0'
$required = @(
    'linux_x86_64_gnu/librust_tcp.so',
    'linux_x86_64_musl/librust_tcp.so',
    'linux_aarch64_gnu/librust_tcp.so',
    'macos_x86_64/librust_tcp.dylib',
    'macos_aarch64/librust_tcp.dylib',
    'windows_x86_64_msvc/rust_tcp.dll'
)
if ($HostOnly) {
    $hostTriple = (rustc -vV | Select-String '^host: ').ToString().Substring(6)
    if ($hostTriple -ne 'x86_64-pc-windows-msvc') { throw "Unsupported host: $hostTriple" }
    $required = @('windows_x86_64_msvc/rust_tcp.dll')
    $name = "$name-$hostTriple"
}
foreach ($artifact in $required) {
    $path = Join-Path 'native' $artifact
    if (-not (Test-Path $path) -or (Get-Item $path).Length -eq 0) {
        throw "Missing $path; build and test on that host before assembling distribution"
    }
}
$stage = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())
try {
    $root = Join-Path $stage $name
    New-Item -ItemType Directory -Force (Join-Path $root 'src'), (Join-Path $root 'native'), 'dist' | Out-Null
    Copy-Item spar.package.spar, README.md $root
    Copy-Item src/lib.spar (Join-Path $root 'src')
    Copy-Item native/interface.json (Join-Path $root 'native')
    foreach ($artifact in $required) {
        $target = Join-Path (Join-Path $root 'native') $artifact
        New-Item -ItemType Directory -Force (Split-Path $target) | Out-Null
        Copy-Item (Join-Path 'native' $artifact) $target
    }
    $archive = Join-Path 'dist' "$name.zip"
    Compress-Archive -Path $root -DestinationPath $archive -Force
    $hash = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $name.zip" | Set-Content "$archive.sha256"
    Write-Output $archive
} finally {
    Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
}
