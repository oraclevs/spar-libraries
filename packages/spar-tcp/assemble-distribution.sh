#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
version=0.1.0
name="spar-tcp-$version"
all_targets=(
  linux_x86_64_gnu/librust_tcp.so
  linux_x86_64_musl/librust_tcp.so
  linux_aarch64_gnu/librust_tcp.so
  macos_x86_64/librust_tcp.dylib
  macos_aarch64/librust_tcp.dylib
  windows_x86_64_msvc/rust_tcp.dll
)
if [[ "${1:-}" == "--host-only" ]]; then
  host=$(rustc -vV | sed -n 's/^host: //p')
  case "$host" in
    x86_64-unknown-linux-gnu) required=(linux_x86_64_gnu/librust_tcp.so) ;;
    x86_64-unknown-linux-musl) required=(linux_x86_64_musl/librust_tcp.so) ;;
    aarch64-unknown-linux-gnu) required=(linux_aarch64_gnu/librust_tcp.so) ;;
    x86_64-apple-darwin) required=(macos_x86_64/librust_tcp.dylib) ;;
    aarch64-apple-darwin) required=(macos_aarch64/librust_tcp.dylib) ;;
    *) echo "unsupported host: $host" >&2; exit 2 ;;
  esac
  name="$name-$host"
elif [[ $# -eq 0 ]]; then
  required=("${all_targets[@]}")
else
  echo "usage: $0 [--host-only]" >&2
  exit 2
fi
for artifact in "${required[@]}"; do
  if [[ ! -s "native/$artifact" ]]; then
    echo "missing native/$artifact; build and test on that host before assembling distribution" >&2
    exit 2
  fi
done
mkdir -p dist
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/$name/src" "$stage/$name/native"
cp spar.package.spar README.md "$stage/$name/"
cp src/lib.spar "$stage/$name/src/"
cp native/interface.json "$stage/$name/native/"
for artifact in "${required[@]}"; do
  mkdir -p "$stage/$name/native/$(dirname "$artifact")"
  cp "native/$artifact" "$stage/$name/native/$artifact"
done
tar -C "$stage" -czf "dist/$name.tar.gz" "$name"
if command -v sha256sum >/dev/null 2>&1; then
  (cd dist && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
else
  (cd dist && shasum -a 256 "$name.tar.gz" > "$name.tar.gz.sha256")
fi
echo "dist/$name.tar.gz"
