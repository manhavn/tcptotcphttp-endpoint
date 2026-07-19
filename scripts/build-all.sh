#!/bin/bash
cd "$(dirname "$0")"
set -euo pipefail
./build-linux-amd64.sh
echo "Note: cross targets (arm64/macos/windows) need rustup targets; use cargo build --release --target <triple>"
ls -lh ../tcptotcphttp-endpoint-* 2>/dev/null || true
