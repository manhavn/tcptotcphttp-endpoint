#!/bin/bash
# shellcheck disable=SC2164
cd "$(dirname "$0")/.."
set -euo pipefail

APP="tcptotcphttp-endpoint"
OUT="${APP}-linux-arm64"
TARGET="aarch64-unknown-linux-musl"

echo "Building ${OUT} (static musl) ..."
rustup target add "${TARGET}" >/dev/null 2>&1 || true

cargo build --release --target "${TARGET}"
BIN="target/${TARGET}/release/${APP}"
cp -f "${BIN}" "${OUT}"
echo "OK: ${OUT}"
file "${OUT}" 2>/dev/null || true
