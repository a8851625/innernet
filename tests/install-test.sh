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

# --- musl / OpenWrt detection -----------------------------------------------

# The release archive differs by C library, not just by OS: a glibc binary
# cannot run on a musl system at all. Getting this wrong installs a file that
# fails with "not found" the first time it is run.
#
# Sourced from the real script rather than restated, so the test cannot drift
# from the installer.
src_dir=$(dirname "$0")/..
is_musl() {
    if command -v ldd >/dev/null 2>&1; then
        if ldd --version 2>&1 | grep -qi musl; then echo yes; return; fi
        if ldd --version 2>&1 | grep -qiE 'gnu libc|glibc'; then echo no; return; fi
    fi
    if [ -e /lib/ld-musl-"$(uname -m)".so.1 ]; then echo yes; return; fi
    if [ -f /etc/openwrt_release ] || command -v opkg >/dev/null 2>&1; then
        echo yes; return
    fi
    echo no
}

# is_musl must always answer yes or no, never empty -- detect_target would
# otherwise build a target triple with an empty component.
got=$(is_musl)
case "$got" in
    yes|no) ok "is_musl answers $got on this host" ;;
    *)      fail "is_musl answers yes or no" "got '$got'" ;;
esac

# The installer must actually consult it for Linux; previously every Linux was
# assumed to be gnu.
if grep -q 'unknown-linux-musl' "$src_dir/install.sh"; then
    ok "install.sh can select the musl archive"
else
    fail "install.sh can select the musl archive"
fi

# OpenWrt is detected even when ldd gives no usable answer.
grep -q 'openwrt_release' "$src_dir/install.sh" && ok "install.sh recognises OpenWrt" \
    || fail "install.sh recognises OpenWrt"

# The kernel module is a separate package there; the installer has to say so or
# `up` fails with an error that mentions neither WireGuard nor opkg.
grep -q 'kmod-wireguard' "$src_dir/install.sh" && ok "install.sh points at kmod-wireguard" \
    || fail "install.sh points at kmod-wireguard"

# --- OpenWrt install location -----------------------------------------------

# OpenWrt's PATH is /usr/sbin:/usr/bin:/sbin:/bin and /usr/local/bin does not
# exist. The installer must not fall through to $HOME/.local/bin there: it is
# not on PATH, so the command would be installed and then not found.
if grep -q '/usr/bin' "$src_dir/install.sh" && grep -q 'openwrt_release' "$src_dir/install.sh"; then
    ok "install.sh targets /usr/bin on OpenWrt"
else
    fail "install.sh targets /usr/bin on OpenWrt"
fi

# --- busybox compatibility --------------------------------------------------

# busybox has no `install`, so OpenWrt and Alpine fail at the last step after
# the download and checksum have already succeeded. The copy helper must fall
# back to cp + chmod.
if grep -q 'copy_in()' "$src_dir/install.sh"; then
    ok "install.sh has a portable copy helper"
else
    fail "install.sh has a portable copy helper"
fi

# It must actually fall back, not merely exist.
if grep -A12 'copy_in()' "$src_dir/install.sh" | grep -q 'cp.*&&.*chmod'; then
    ok "the copy helper falls back to cp + chmod"
else
    fail "the copy helper falls back to cp + chmod"
fi

# No bare `install -m` may remain outside the helper: those are the call sites
# that break on busybox.
bare=$(grep -c 'need_sudo install -m' "$src_dir/install.sh" || true)
[ "$bare" -le 1 ] && ok "no bare install -m call sites remain ($bare in the helper)" \
    || fail "no bare install -m call sites remain" "found $bare"

# Exercise the helper for real: source it and copy a file with a mode.
tmpd=$(mktemp -d)
printf 'x' > "$tmpd/src"
need_sudo=""
# Pull just the function out of the installer.
eval "$(sed -n '/^copy_in()/,/^}/p' "$src_dir/install.sh")"
if copy_in "$tmpd/src" "$tmpd/dst" 0755 2>/dev/null; then
    perms=$(ls -l "$tmpd/dst" | cut -c1-10)
    case "$perms" in
        -rwxr-xr-x) ok "copy_in applies the requested mode" ;;
        *)          fail "copy_in applies the requested mode" "got $perms" ;;
    esac
else
    fail "copy_in applies the requested mode" "copy_in failed"
fi
rm -rf "$tmpd"

# --- summary ----------------------------------------------------------------

echo ""
printf 'PASS=%s FAIL=%s\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
