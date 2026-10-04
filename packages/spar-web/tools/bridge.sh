#!/usr/bin/env bash
set -euo pipefail
bridge_dir=$(cd "$(dirname "$0")" && pwd)
cargo build --offline --quiet --manifest-path "$bridge_dir/Cargo.toml"
exec "$bridge_dir/target/debug/spar-web-bridge" "$@"
