#!/bin/bash
# shellcheck disable=SC2164
cd "$(dirname "$0")"
set -euo pipefail

echo "=== build all targets: tcptotcphttp-endpoint ==="
./build-linux-amd64.sh
if ./build-linux-arm64.sh 2>/dev/null; then
  echo "arm64 OK"
else
  echo "WARN: linux-arm64 skipped/failed (install cross musl toolchain if needed)"
fi
echo "=== done ==="
ls -lh ../tcptotcphttp-endpoint-linux-* 2>/dev/null || true
