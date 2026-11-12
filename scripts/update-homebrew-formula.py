#!/usr/bin/env python3
"""Rewrite the Homebrew formula for a given release.

The formula carries a version and one url+sha256 pair per macOS architecture,
so every release has to be reflected in it. Doing that by hand is what caused
the tap to sit three releases behind: the release workflow would go green and
the formula would quietly keep pointing at an older tag.

This script takes the SHA256SUMS file the release already produces and rewrites
the formula in place, so the version and both checksums can never disagree with
the archives they describe.

Usage:
    update-homebrew-formula.py --version 2.3.2 --sums SHA256SUMS \
        --formula homebrew/Formula/innernet.rb

Exits non-zero, changing nothing, if a required architecture is missing from
the checksums or the formula does not look like the file this script expects.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

# The formula only ships prebuilt macOS binaries, so these are the only two
# checksums it needs. Linux users go through install.sh instead.
REQUIRED_TARGETS = ("aarch64-apple-darwin", "x86_64-apple-darwin")


def parse_sums(path: Path) -> dict[str, str]:
    """Map target triple -> sha256 from a SHA256SUMS file.

    Lines look like "<sha>  innernet-2.3.2-aarch64-apple-darwin.tar.gz". The
    release also carries GUI bundles, which are not part of the formula and are
    skipped by only keeping names that match the CLI archive pattern.
    """
    sums: dict[str, str] = {}
    pattern = re.compile(r"^innernet-[\d.]+-(?P<target>[a-z0-9_]+-[a-z0-9_]+-[a-z0-9_]+)\.tar\.gz$")
    for line in path.read_text().splitlines():
        line = line.strip()
        if not line:
            continue
        parts = line.split()
        if len(parts) != 2:
            continue
        digest, name = parts
        match = pattern.match(name)
        if match:
            sums[match.group("target")] = digest
    return sums


def rewrite(text: str, version: str, sums: dict[str, str]) -> str:
    # 1. The version attribute.
    text, version_hits = re.subn(
        r'^(\s*version\s+)"[^"]*"$',
        rf'\g<1>"{version}"',
        text,
        count=1,
        flags=re.M,
    )
    if version_hits != 1:
        raise SystemExit("could not find a single `version \"...\"` line in the formula")

    # 2. Every release url, and the sha256 that follows it. Doing these in one
    #    pass keeps each checksum attached to the architecture it belongs to,
    #    which matters because the two differ.
    def fix_block(match: re.Match[str]) -> str:
        target = match.group("target")
        if target not in sums:
            raise SystemExit(f"no checksum for {target} in SHA256SUMS")
        return (
            f'url "https://github.com/a8851625/innernet/releases/download/'
            f'v{version}/innernet-{version}-{target}.tar.gz"\n'
            f'    sha256 "{sums[target]}"'
        )

    text, url_hits = re.subn(
        r'url "https://github\.com/a8851625/innernet/releases/download/v[\d.]+/'
        r'innernet-[\d.]+-(?P<target>[a-z0-9_]+-[a-z0-9_]+-[a-z0-9_]+)\.tar\.gz"\s*\n'
        r'\s*sha256 "[0-9a-f]*"',
        fix_block,
        text,
    )
    if url_hits != len(REQUIRED_TARGETS):
        raise SystemExit(
            f"expected {len(REQUIRED_TARGETS)} release url blocks, rewrote {url_hits}"
        )
    return text


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--sums", required=True, type=Path)
    parser.add_argument("--formula", required=True, type=Path)
    parser.add_argument(
        "--check",
        action="store_true",
        help="exit 1 if the formula is out of date, without writing anything",
    )
    args = parser.parse_args()

    version = args.version.lstrip("v")
    sums = parse_sums(args.sums)
    missing = [t for t in REQUIRED_TARGETS if t not in sums]
    if missing:
        raise SystemExit(f"SHA256SUMS is missing: {', '.join(missing)}")

    original = args.formula.read_text()
    updated = rewrite(original, version, sums)

    if args.check:
        if original != updated:
            print(
                f"{args.formula} is not at {version}; run this script without --check",
                file=sys.stderr,
            )
            return 1
        print(f"{args.formula} is up to date at {version}")
        return 0

    args.formula.write_text(updated)
    print(f"rewrote {args.formula} to {version}")
    for target in REQUIRED_TARGETS:
        print(f"  {target}: {sums[target]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
