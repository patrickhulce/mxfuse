#!/usr/bin/env bash
# napi artifacts only warns when a target binary is missing. Fail the release
# if any configured platform package is empty.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
npm_dir="$root/src/node-mxfuse/npm"
missing=0

if [[ ! -d "$npm_dir" ]]; then
  echo "missing $npm_dir; run napi create-npm-dirs first" >&2
  exit 1
fi

for dir in "$npm_dir"/*; do
  [[ -d "$dir" ]] || continue
  if ! find "$dir" -name '*.node' | grep -q .; then
    echo "missing .node binary in $dir" >&2
    missing=1
  fi
done

exit "$missing"
