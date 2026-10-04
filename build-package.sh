#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release
host=$(rustc -vV | sed -n 's/^host: //p')
case "$host" in
  x86_64-unknown-linux-gnu) key=linux_x86_64_gnu; ext=so ;;
  x86_64-unknown-linux-musl) key=linux_x86_64_musl; ext=so ;;
  aarch64-unknown-linux-gnu) key=linux_aarch64_gnu; ext=so ;;
  x86_64-apple-darwin) key=macos_x86_64; ext=dylib ;;
  aarch64-apple-darwin) key=macos_aarch64; ext=dylib ;;
  *) echo "unsupported native package target: $host" >&2; exit 1 ;;
esac
mkdir -p "native/$key"
cp "target/release/librust_tcp.$ext" "native/$key/librust_tcp.$ext"
echo "built native/$key/librust_tcp.$ext"
