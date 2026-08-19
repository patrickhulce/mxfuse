#!/usr/bin/env bash
# Fail the release if any configured napi triple is missing from the main package.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
pkg="$root/src/node-mxfuse"
missing=0

triples=(
  darwin-arm64
  darwin-x64
  linux-x64-gnu
  linux-arm64-gnu
  win32-x64-msvc
)

for triple in "${triples[@]}"; do
  if [[ ! -f "$pkg/mxfuse.${triple}.node" ]]; then
    echo "missing $pkg/mxfuse.${triple}.node" >&2
    missing=1
  fi
done

exit "$missing"
