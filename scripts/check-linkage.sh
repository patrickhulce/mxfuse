#!/usr/bin/env bash
# Fail if a shipped binary dynamically links a library that should be static.
set -euo pipefail

binary="${1:?usage: check-linkage.sh <binary>}"
os="${2:-$(uname -s)}"

forbidden='libbmx|libMXF|libMXF\+\+|libexpat|liburiparser|libuuid'

case "$os" in
  Linux|linux)
    deps="$(ldd "$binary" || true)"
    echo "$deps"
    if echo "$deps" | grep -Eiq "$forbidden"; then
      echo "forbidden shared library found in $binary" >&2
      exit 1
    fi
    if ! echo "$deps" | grep -Eq 'libstdc\+\+\.so\.6'; then
      echo "expected dynamic libstdc++.so.6 on Linux" >&2
      exit 1
    fi
    ;;
  Darwin|darwin|macOS)
    deps="$(otool -L "$binary")"
    echo "$deps"
    if echo "$deps" | grep -Eiq "$forbidden"; then
      echo "forbidden shared library found in $binary" >&2
      exit 1
    fi
    if command -v vtool >/dev/null 2>&1; then
      vtool -show "$binary" || true
    fi
    ;;
  MINGW*|MSYS*|Windows|windows)
    if command -v dumpbin >/dev/null 2>&1; then
      deps="$(dumpbin /dependents "$binary")"
    elif command -v llvm-readobj >/dev/null 2>&1; then
      deps="$(llvm-readobj --coff-imports "$binary")"
    else
      echo "no PE dependency tool available" >&2
      exit 1
    fi
    echo "$deps"
    if echo "$deps" | grep -Eiq 'bmx\.dll|MXF\.dll|expat\.dll|uriparser\.dll'; then
      echo "forbidden DLL found in $binary" >&2
      exit 1
    fi
    ;;
  *)
    echo "unsupported OS: $os" >&2
    exit 1
    ;;
esac
