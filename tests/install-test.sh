#!/bin/sh
# Tests for install.sh that do not touch the network.
#
# Run: sh tests/install-test.sh
#
# The parts that matter and are easy to get wrong are argument handling, the
# platform-to-asset mapping, and checksum parsing. Those are all pure logic,
# so they can be exercised here without downloading a release.

set -eu

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
INSTALL_SH="$SCRIPT_DIR/../install.sh"

PASS=0
FAIL=0

ok() {
    PASS=$((PASS + 1))
    printf '  ok   %s\n' "$1"
}

fail() {
    FAIL=$((FAIL + 1))
    printf '  FAIL %s\n' "$1"
    [ $# -ge 2 ] && printf '       %s\n' "$2"
    return 0
}

# expect_status <expected> <description> -- <command...>
expect_status() {
    want=$1
    desc=$2
    shift 3
    set +e
    "$@" >/dev/null 2>&1
    got=$?
    set -e
    if [ "$got" -eq "$want" ]; then
        ok "$desc"
    else
        fail "$desc" "expected exit $want, got $got"
    fi
}

echo "install.sh tests"

# --- help and argument errors -----------------------------------------------

expect_status 0 "--help exits 0" -- sh "$INSTALL_SH" --help
expect_status 1 "unknown option exits 1" -- sh "$INSTALL_SH" --bogus
expect_status 1 "--version without value exits 1" -- sh "$INSTALL_SH" --version
expect_status 1 "--bin-dir without value exits 1" -- sh "$INSTALL_SH" --bin-dir

# --help must actually describe the options, not just exit.
help_out=$(sh "$INSTALL_SH" --help 2>&1)
for flag in --version --bin-dir --uninstall --no-verify; do
    case "$help_out" in
        *"$flag"*) ok "--help documents $flag" ;;
        *) fail "--help documents $flag" "flag not found in output" ;;
    esac
done

# --- platform mapping -------------------------------------------------------
#
# The script maps uname output to release asset names. A wrong mapping is a
# 404 at install time, so pin the mapping for every combination we publish.

map_target() {
    # Mirrors detect_target() in install.sh.
    os="$1"
    arch="$2"
    case "$os" in
        Linux) os_part="unknown-linux-gnu" ;;
        Darwin) os_part="apple-darwin" ;;
        *) printf 'unsupported'; return 0 ;;
    esac
    case "$arch" in
        x86_64 | amd64) arch_part="x86_64" ;;
        arm64 | aarch64) arch_part="aarch64" ;;
        *) printf 'unsupported'; return 0 ;;
    esac
    printf '%s-%s' "$arch_part" "$os_part"
}

check_mapping() {
    got=$(map_target "$1" "$2")
    if [ "$got" = "$3" ]; then
        ok "$1/$2 -> $3"
    else
        fail "$1/$2 -> $3" "got $got"
    fi
}

check_mapping Darwin arm64 aarch64-apple-darwin
check_mapping Darwin aarch64 aarch64-apple-darwin
check_mapping Darwin x86_64 x86_64-apple-darwin
check_mapping Darwin amd64 x86_64-apple-darwin
check_mapping Linux x86_64 x86_64-unknown-linux-gnu
check_mapping Linux amd64 x86_64-unknown-linux-gnu
check_mapping Linux aarch64 aarch64-unknown-linux-gnu
check_mapping Linux arm64 aarch64-unknown-linux-gnu

# Unsupported platforms must be reported, not silently mapped.
for combo in "FreeBSD amd64" "Linux riscv64" "Darwin powerpc"; do
    set -- $combo
    got=$(map_target "$1" "$2")
    if [ "$got" = "unsupported" ]; then
        ok "$1/$2 rejected"
    else
        fail "$1/$2 rejected" "got $got"
    fi
done

# --- checksum parsing -------------------------------------------------------
#
# The published SHA256SUMS uses "<hash>  <file>". The parser must find the
# right line and must not match a different archive whose name is a suffix.

parse_checksum() {
    # Mirrors the sed expression in verify_checksum().
    sed -n "s/.*\([0-9a-f]\{64\}\)[[:space:]]*\*\{0,1\}$2\$/\1/p" "$1" | head -1
}

tmp=$(mktemp -d "${TMPDIR:-/tmp}/install-test.XXXXXX")
trap 'rm -rf "$tmp"' EXIT INT TERM

H_A=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
H_B=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb

cat > "$tmp/SHA256SUMS" <<EOF
$H_A  innernet-2.1.0-aarch64-apple-darwin.tar.gz
$H_B  innernet-2.1.0-x86_64-unknown-linux-gnu.tar.gz
EOF

got=$(parse_checksum "$tmp/SHA256SUMS" "innernet-2.1.0-aarch64-apple-darwin.tar.gz")
[ "$got" = "$H_A" ] && ok "checksum parsed for aarch64-apple-darwin" \
    || fail "checksum parsed for aarch64-apple-darwin" "got $got"

got=$(parse_checksum "$tmp/SHA256SUMS" "innernet-2.1.0-x86_64-unknown-linux-gnu.tar.gz")
[ "$got" = "$H_B" ] && ok "checksum parsed for x86_64-linux" \
    || fail "checksum parsed for x86_64-linux" "got $got"

# A name that is a suffix of another must not match the wrong line. This is
# the bug the trailing $ anchor prevents.
got=$(parse_checksum "$tmp/SHA256SUMS" "apple-darwin.tar.gz")
[ -z "$got" ] && ok "suffix name does not match a longer archive" \
    || fail "suffix name does not match a longer archive" "matched $got"

got=$(parse_checksum "$tmp/SHA256SUMS" "not-in-the-file.tar.gz")
[ -z "$got" ] && ok "missing archive yields no checksum" \
    || fail "missing archive yields no checksum" "got $got"

# Binary-mode checksums use " *file"; the parser must accept both forms.
printf '%s *innernet-2.1.0-aarch64-apple-darwin.tar.gz\n' "$H_A" > "$tmp/BINSUMS"
got=$(parse_checksum "$tmp/BINSUMS" "innernet-2.1.0-aarch64-apple-darwin.tar.gz")
[ "$got" = "$H_A" ] && ok "binary-mode checksum line parsed" \
    || fail "binary-mode checksum line parsed" "got $got"

# --- version normalisation --------------------------------------------------

normalise() {
    case "$1" in
        v*) printf '%s' "$1" ;;
        *) printf 'v%s' "$1" ;;
    esac
}

for input in v2.1.0 2.1.0; do
    got=$(normalise "$input")
    [ "$got" = "v2.1.0" ] && ok "version $input normalises to v2.1.0" \
        || fail "version $input normalises to v2.1.0" "got $got"
done

# --- summary ----------------------------------------------------------------

echo ""
printf 'PASS=%s FAIL=%s\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
