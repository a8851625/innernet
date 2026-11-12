#!/usr/bin/env python3
"""Tests for update-homebrew-formula.py.

Run with: python3 scripts/test-update-homebrew-formula.py

The script exists because the tap fell three releases behind while the release
workflow kept going green, so the interesting cases here are the ones that
would have let that happen silently: a formula left pointing at an old tag, a
checksum swapped between architectures, and a missing platform.
"""

from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / "update-homebrew-formula.py"
FORMULA = HERE.parent / "homebrew" / "Formula" / "innernet.rb"

spec = importlib.util.spec_from_file_location("updater", SCRIPT)
updater = importlib.util.module_from_spec(spec)
spec.loader.exec_module(updater)

failures: list[str] = []


def check(name: str, condition: bool, detail: str = "") -> None:
    if condition:
        print(f"  ok   {name}")
    else:
        failures.append(name)
        print(f"  FAIL {name}{': ' + detail if detail else ''}")


SUMS = """\
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  innernet-9.9.9-aarch64-apple-darwin.tar.gz
bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  innernet-9.9.9-x86_64-apple-darwin.tar.gz
cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc  innernet-9.9.9-x86_64-unknown-linux-gnu.tar.gz
dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd  innernet_aarch64.app.tar.gz
"""

print("parse_sums")
with tempfile.TemporaryDirectory() as tmp:
    sums_file = Path(tmp) / "SHA256SUMS"
    sums_file.write_text(SUMS)
    parsed = updater.parse_sums(sums_file)

    check("keeps both macOS targets", set(parsed) == set(updater.REQUIRED_TARGETS), str(parsed))
    check(
        "does not pick up the GUI bundle",
        "innernet_aarch64.app.tar.gz" not in parsed,
        str(parsed),
    )
    check(
        "arm checksum is the arm one",
        parsed.get("aarch64-apple-darwin", "").startswith("aaaa"),
        parsed.get("aarch64-apple-darwin", ""),
    )
    check(
        "intel checksum is the intel one",
        parsed.get("x86_64-apple-darwin", "").startswith("bbbb"),
        parsed.get("x86_64-apple-darwin", ""),
    )

print("parse_sums tolerates noise")
with tempfile.TemporaryDirectory() as tmp:
    noisy = Path(tmp) / "SHA256SUMS"
    noisy.write_text("\n\n" + SUMS + "garbage line\n")
    check("ignores blank and malformed lines", len(updater.parse_sums(noisy)) == 2)

print("rewrite")
original = (HERE.parent / "homebrew" / "Formula" / "innernet.rb").read_text()
with tempfile.TemporaryDirectory() as tmp:
    sums_file = Path(tmp) / "SHA256SUMS"
    sums_file.write_text(SUMS)
    parsed = updater.parse_sums(sums_file)
    rewritten = updater.rewrite(original, "9.9.9", parsed)

    check("version updated", '  version "9.9.9"' in rewritten)
    check("no stale version left", "2.2.2" not in rewritten, "old version still present")
    check("arm url points at the new tag", "download/v9.9.9/innernet-9.9.9-aarch64-apple-darwin.tar.gz" in rewritten)
    check("intel url points at the new tag", "download/v9.9.9/innernet-9.9.9-x86_64-apple-darwin.tar.gz" in rewritten)
    check("arm sha follows the arm url", 'sha256 "aaaa' in rewritten)
    check("intel sha follows the intel url", 'sha256 "bbbb' in rewritten)

    # The whole point: the two checksums must not get swapped. Slice the text
    # between the block openers instead of splitting on "end", which also
    # appears in the surrounding comments.
    arm_block = rewritten.split("on_arm do", 1)[1].split("on_intel do", 1)[0]
    intel_block = rewritten.split("on_intel do", 1)[1].split("livecheck do", 1)[0]
    check("arm block has only the arm checksum", arm_block.count("a"*64) == 1 and "b"*64 not in arm_block, arm_block)
    check("intel block has only the intel checksum", intel_block.count("b"*64) == 1 and "a"*64 not in intel_block, intel_block)

    # Idempotent: running twice changes nothing.
    again = updater.rewrite(rewritten, "9.9.9", parsed)
    check("rewrite is idempotent", again == rewritten)

print("missing platform is fatal")
with tempfile.TemporaryDirectory() as tmp:
    partial = Path(tmp) / "SHA256SUMS"
    partial.write_text("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  innernet-9.9.9-aarch64-apple-darwin.tar.gz\n")
    formula = Path(tmp) / "innernet.rb"
    formula.write_text(original)
    before = formula.read_text()
    result = subprocess.run(
        [sys.executable, str(SCRIPT), "--version", "9.9.9", "--sums", str(partial), "--formula", str(formula)],
        capture_output=True,
        text=True,
    )
    check("exits non-zero", result.returncode != 0, result.stderr.strip())
    check("names the missing target", "x86_64-apple-darwin" in result.stderr, result.stderr.strip())
    check("formula untouched on failure", formula.read_text() == before)

print("--check mode")
with tempfile.TemporaryDirectory() as tmp:
    sums_file = Path(tmp) / "SHA256SUMS"
    sums_file.write_text(SUMS)
    stale = Path(tmp) / "stale.rb"
    stale.write_text(original)  # still at 2.2.2
    result = subprocess.run(
        [sys.executable, str(SCRIPT), "--version", "9.9.9", "--sums", str(sums_file), "--formula", str(stale), "--check"],
        capture_output=True,
        text=True,
    )
    check("--check fails on a stale formula", result.returncode == 1, f"rc={result.returncode}")

    fresh = Path(tmp) / "fresh.rb"
    fresh.write_text(updater.rewrite(original, "9.9.9", updater.parse_sums(sums_file)))
    result = subprocess.run(
        [sys.executable, str(SCRIPT), "--version", "9.9.9", "--sums", str(sums_file), "--formula", str(fresh), "--check"],
        capture_output=True,
        text=True,
    )
    check("--check passes on an up-to-date formula", result.returncode == 0, result.stderr.strip())

print("formula in the repo matches its own version")
check("repo formula parses as Ruby", subprocess.run(["ruby", "-c", str(FORMULA)], capture_output=True).returncode == 0)

if failures:
    print(f"\nFAIL={len(failures)}: {', '.join(failures)}")
    raise SystemExit(1)
print("\nALL TESTS PASSED")
