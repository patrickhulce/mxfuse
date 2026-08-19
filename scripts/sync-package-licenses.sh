#!/usr/bin/env bash
# Copy LICENSE and third-party notices into each published package directory.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"

dests=(
  "$root/src/mxfuse-sys"
  "$root/src/rust-mxfuse"
  "$root/src/python-mxfuse"
  "$root/src/node-mxfuse"
)
for dest in "${dests[@]}"; do
  cp "$root/LICENSE" "$root/THIRD_PARTY_NOTICES.md" "$dest/"
done
