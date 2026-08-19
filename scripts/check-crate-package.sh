#!/usr/bin/env bash
# Assert cargo package ships generated/bmx and stays under the crates.io size cap.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

if [[ ! -d src/mxfuse-sys/generated/bmx ]]; then
  echo "src/mxfuse-sys/generated/bmx is missing; run cargo build -p mxfuse-sys first" >&2
  exit 1
fi

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

cargo package -p mxfuse-sys --allow-dirty --no-verify --list >"$tmp"
if ! grep -F -q 'generated/bmx/CMakeLists.txt' "$tmp"; then
  echo "cargo package -p mxfuse-sys does not include generated/bmx" >&2
  head -n 40 "$tmp" >&2
  exit 1
fi
if ! grep -F -x -q 'LICENSE' "$tmp"; then
  echo "cargo package -p mxfuse-sys does not include LICENSE" >&2
  exit 1
fi
if ! grep -F -x -q 'THIRD_PARTY_NOTICES.md' "$tmp"; then
  echo "cargo package -p mxfuse-sys does not include THIRD_PARTY_NOTICES.md" >&2
  exit 1
fi

cargo package -p mxfuse-sys --allow-dirty --no-verify --quiet
crate="$(ls -1 target/package/mxfuse-sys-*.crate | tail -n 1)"
size="$(wc -c < "$crate" | tr -d ' ')"
max=$((10 * 1024 * 1024))
echo "$crate is ${size} bytes"
if [[ "$size" -ge "$max" ]]; then
  echo "crate exceeds crates.io 10 MiB limit" >&2
  exit 1
fi

cargo package -p mxfuse --allow-dirty --no-verify --list >"$tmp"
if grep -F -q 'sample_op1a.mxf' "$tmp"; then
  echo "cargo package -p mxfuse unexpectedly includes the MXF fixture" >&2
  exit 1
fi
if ! grep -F -x -q 'LICENSE' "$tmp"; then
  echo "cargo package -p mxfuse does not include LICENSE" >&2
  exit 1
fi

echo "crate package checks passed"
