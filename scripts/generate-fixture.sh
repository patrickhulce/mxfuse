#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/tests/fixtures/sample_op1a.mxf"
mkdir -p "$(dirname "$out")"

if [[ -f "$out" ]]; then
  echo "fixture already exists: $out"
  exit 0
fi

if command -v ffmpeg >/dev/null 2>&1; then
  tmp="$out.tmp"
  ffmpeg -y -f lavfi -i "color=c=black:s=720x576:r=25:d=0.16" \
    -pix_fmt yuv420p -c:v dvvideo -f mxf "$tmp"
  mv "$tmp" "$out"
  echo "wrote $out"
  exit 0
fi

echo "ffmpeg is required to generate tests/fixtures/sample_op1a.mxf" >&2
echo "Install ffmpeg, or build vendor/bmx with apps enabled and run raw2bmx." >&2
exit 1
