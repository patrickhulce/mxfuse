#!/usr/bin/env bash
# Copy LICENSE, notices, and the npm README into each published package directory.
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
# npm only packs files inside the package directory.
cp "$root/README.md" "$root/src/node-mxfuse/README.md"
