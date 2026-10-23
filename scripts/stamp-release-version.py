#!/usr/bin/env python3
"""Stamp published package versions to match a release tag.

Checked-in source is always ``0.0.0-dev``: that string is valid Cargo semver,
npm semver, and PEP 440 (normalized to ``0.0.0.dev0``). ``0.0.0.dev0`` itself
is not a legal Cargo or npm version. Release jobs pass the tag (``v1.2.3``)
to replace the placeholder before they build or publish.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEV_VERSION = "0.0.0-dev"
RELEASE_VERSION_RE = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")
WORKSPACE_PACKAGES = (
    "mxfuse",
    "mxfuse-sys",
    "mxfuse_node_bindings",
    "mxfuse_python_bindings",
)


def replace_once(path: Path, pattern: str, replacement: str, *, flags: int = 0) -> None:
    text = path.read_text()
    new, count = re.subn(pattern, replacement, text, count=1, flags=flags)
    if count != 1:
        raise SystemExit(f"{path}: expected 1 replacement for {pattern!r}, got {count}")
    path.write_text(new)


def stamp_cargo_lock(path: Path, version: str) -> None:
    text = path.read_text()
    for name in WORKSPACE_PACKAGES:
        pattern = rf'(name = "{re.escape(name)}"\nversion = ")([^"]+)(")'
        new, count = re.subn(pattern, rf"\g<1>{version}\3", text, count=1)
        if count != 1:
            raise SystemExit(f"{path}: failed to stamp {name} (count={count})")
        text = new
    path.write_text(text)


def stamp(version: str) -> None:
    replace_once(
        ROOT / "Cargo.toml",
        r'(?m)^(\[workspace\.package\]\n(?:(?!\[).)*?^version = ")[^"]+(")',
        rf"\g<1>{version}\2",
        flags=re.DOTALL,
    )
    stamp_cargo_lock(ROOT / "Cargo.lock", version)
    replace_once(
        ROOT / "src/rust-mxfuse/Cargo.toml",
        r'(mxfuse-sys = \{ path = "\.\./mxfuse-sys", version = ")[^"]+(")',
        rf"\g<1>{version}\2",
    )
    replace_once(
        ROOT / "src/python-mxfuse/pyproject.toml",
        r'(?m)^(version = ")[^"]+(")',
        rf"\g<1>{version}\2",
    )
    replace_once(
        ROOT / "src/python-mxfuse/uv.lock",
        r'(name = "mxfuse"\nversion = ")[^"]+("\nsource = \{ editable = "\." \})',
        rf"\g<1>{version}\2",
    )
    replace_once(
        ROOT / "src/node-mxfuse/package.json",
        r'("version": ")[^"]+(")',
        rf"\g<1>{version}\2",
    )
    replace_once(
        ROOT / "package.json",
        r'("version": ")[^"]+(")',
        rf"\g<1>{version}\2",
    )
    print(f"stamped {version}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "version",
        nargs="?",
        help="release version without a leading v, e.g. 0.1.3",
    )
    parser.add_argument(
        "--dev",
        action="store_true",
        help=f"restore the source placeholder ({DEV_VERSION})",
    )
    args = parser.parse_args()
    if args.dev:
        if args.version:
            raise SystemExit("pass a version or --dev, not both")
        stamp(DEV_VERSION)
        return 0
    if not args.version:
        raise SystemExit("pass a version or --dev")
    version = args.version.removeprefix("v")
    if not RELEASE_VERSION_RE.fullmatch(version):
        raise SystemExit(f"invalid release version: {args.version}")
    stamp(version)
    return 0


if __name__ == "__main__":
    sys.exit(main())
