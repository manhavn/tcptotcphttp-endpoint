#!/bin/bash
cd "$(dirname "$0")/.."
set -euo pipefail
OUT="tcptotcphttp-endpoint-linux-amd64"
echo "Building $OUT (static-friendly release) ..."
export CARGO_TARGET_DIR=target
cargo build --release
cp -f "target/release/tcptotcphttp-endpoint" "$OUT"
strip "$OUT" 2>/dev/null || true
file "$OUT" 2>/dev/null || true
echo "OK: $OUT"
