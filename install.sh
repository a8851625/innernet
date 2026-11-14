#!/bin/sh
# innernet installer.
#
#   curl -fsSL https://raw.githubusercontent.com/a8851625/innernet/main/install.sh | sh
#
# Downloads a prebuilt release and installs the `innernet` and
# `innernet-server` binaries. Pass arguments after `sh -s --`:
#
#   curl -fsSL .../install.sh | sh -s -- --version v2.1.0
#   curl -fsSL .../install.sh | sh -s -- --bin-dir ~/.local/bin
#   curl -fsSL .../install.sh | sh -s -- --uninstall
#
# POSIX sh on purpose: this runs under sh, dash and busybox ash.

set -eu

REPO="a8851625/innernet"
BIN_NAME="innernet"
SERVER_BIN_NAME="innernet-server"

VERSION=""
BIN_DIR=""
UNINSTALL=0
NO_VERIFY=0

# --- output -----------------------------------------------------------------

# Colour only when writing to a terminal, so piped output stays clean.
if [ -t 1 ] && [ "${NO_COLOR:-}" = "" ]; then
    C_RESET=$(printf '\033[0m')
    C_DIM=$(printf '\033[2m')
    C_RED=$(printf '\033[31m')
    C_GREEN=$(printf '\033[32m')
    C_YELLOW=$(printf '\033[33m')
    C_BOLD=$(printf '\033[1m')
else
    C_RESET=""; C_DIM=""; C_RED=""; C_GREEN=""; C_YELLOW=""; C_BOLD=""
fi

info()  { printf '%s\n' "$*"; }
dim()   { printf '%s%s%s\n' "$C_DIM" "$*" "$C_RESET"; }
ok()    { printf '%s%s%s\n' "$C_GREEN" "$*" "$C_RESET"; }
warn()  { printf '%s%s%s\n' "$C_YELLOW" "$*" "$C_RESET" >&2; }

die() {
    printf '%s\n' "${C_RED}error:${C_RESET} $*" >&2
    exit 1
}

usage() {
    cat <<EOF
${C_BOLD}innernet installer${C_RESET}

Usage:
  install.sh [options]

Options:
  --version <tag>     Install a specific release tag (default: latest)
  --bin-dir <dir>     Where to install (default: /usr/local/bin, or
                      ~/.local/bin when /usr/local/bin is not writable)
  --no-verify         Skip the SHA-256 checksum check
  --uninstall         Remove the installed binaries
  -h, --help          Show this help

Examples:
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | sh
  curl -fsSL .../install.sh | sh -s -- --version v2.1.0
  curl -fsSL .../install.sh | sh -s -- --bin-dir ~/.local/bin
EOF
}

# --- argument parsing -------------------------------------------------------

while [ $# -gt 0 ]; do
    case "$1" in
        --version)
            [ $# -ge 2 ] || die "--version needs a value"
            VERSION="$2"; shift 2 ;;
        --version=*)
            VERSION="${1#*=}"; shift ;;
        --bin-dir)
            [ $# -ge 2 ] || die "--bin-dir needs a value"
            BIN_DIR="$2"; shift 2 ;;
        --bin-dir=*)
            BIN_DIR="${1#*=}"; shift ;;
        --no-verify)
            NO_VERIFY=1; shift ;;
        --uninstall)
            UNINSTALL=1; shift ;;
        -h|--help)
            usage; exit 0 ;;
        *)
            die "unknown option: $1 (try --help)" ;;
    esac
done

# --- platform detection -----------------------------------------------------

# Whether the C library is musl rather than glibc.
#
# A glibc binary simply does not run on a musl system: there is no ld-linux
# loader, and the failure is "not found", which looks like a missing file. So
# the choice of release archive has to follow the C library, not just the OS.
#
# Probed by running `ldd` on the shell itself, because /lib/ld-musl-* paths
# differ between OpenWrt, Alpine and vendor images. `ldd --version` is not
# portable (busybox ldd has no such flag), and reading /etc/os-release misses
# musl-on-glibc-distro setups.
is_musl() {
    if command -v ldd >/dev/null 2>&1; then
        # musl's ldd prints "musl libc" or the loader path; glibc prints
        # "GNU libc" or "GLIBC". busybox ldd prints neither, so fall through.
        if ldd --version 2>&1 | grep -qi musl; then
            echo yes
            return
        fi
        if ldd --version 2>&1 | grep -qiE 'gnu libc|glibc'; then
            echo no
            return
        fi
    fi
    # No usable ldd, or busybox's. Look for the loader itself.
    if [ -e /lib/ld-musl-"$(uname -m)".so.1 ]; then
        echo yes
        return
    fi
    # OpenWrt is always musl, whatever else is true.
    if [ -f /etc/openwrt_release ] || command -v opkg >/dev/null 2>&1; then
        echo yes
        return
    fi
    echo no
}

detect_target() {
    os=$(uname -s)
    arch=$(uname -m)

    case "$os" in
        Linux)
            if [ "$(is_musl)" = "yes" ]; then
                os_part="unknown-linux-musl"
            else
                os_part="unknown-linux-gnu"
            fi
            ;;
        Darwin) os_part="apple-darwin" ;;
        *)
            die "unsupported OS: $os. This script handles Linux and macOS.
For Windows, download the .zip from https://github.com/${REPO}/releases."
            ;;
    esac

    case "$arch" in
        x86_64|amd64)  arch_part="x86_64" ;;
        arm64|aarch64) arch_part="aarch64" ;;
        *)
            die "unsupported architecture: $arch (need x86_64 or arm64)"
            ;;
    esac

    printf '%s-%s' "$arch_part" "$os_part"
}

# --- download ---------------------------------------------------------------

# fetch <url> <dest>
fetch() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --retry 3 --connect-timeout 10 -o "$2" "$1"
    elif command -v wget >/dev/null 2>&1; then
        wget -q -T 10 -O "$2" "$1"
    else
        die "need curl or wget to download"
    fi
}

# Resolve the latest tag without the API, so this works unauthenticated and
# does not burn a rate-limit slot.
resolve_latest_version() {
    # releases/latest redirects to releases/tag/<tag>; read the final URL.
    if command -v curl >/dev/null 2>&1; then
        url=$(curl -fsSLI -o /dev/null -w '%{url_effective}' \
              "https://github.com/${REPO}/releases/latest" 2>/dev/null || true)
        tag=${url##*/}
    else
        tag=""
    fi

    # Fall back to the API if the redirect trick did not yield a tag.
    if [ -z "$tag" ] || [ "$tag" = "latest" ]; then
        if command -v curl >/dev/null 2>&1; then
            tag=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
                  | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)
        fi
    fi

    [ -n "$tag" ] || die "could not determine the latest release.
Specify one explicitly: --version v2.1.0"
    printf '%s' "$tag"
}

# --- checksum ---------------------------------------------------------------

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    else
        printf ''
    fi
}

verify_checksum() {
    archive="$1"
    sums="$2"

    [ "$NO_VERIFY" -eq 1 ] && { dim "skipping checksum verification (--no-verify)"; return 0; }

    expected=$(sed -n "s/.*\([0-9a-f]\{64\}\)[[:space:]]*\*\{0,1\}$(basename "$archive")$/\1/p" "$sums" 2>/dev/null | head -1)

    if [ -z "$expected" ]; then
        warn "no checksum published for $(basename "$archive"); skipping verification"
        return 0
    fi

    actual=$(sha256_of "$archive")
    if [ -z "$actual" ]; then
        warn "no sha256 tool found; skipping verification"
        return 0
    fi

    if [ "$expected" != "$actual" ]; then
        die "checksum mismatch for $(basename "$archive")
  expected: $expected
  actual:   $actual
Refusing to install. Re-run with --no-verify only if you trust the download."
    fi
    ok "checksum verified"
}

# --- install ----------------------------------------------------------------

# copy_in <src> <dst> <mode>
#
# `install -m` is the obvious way to put a file in place, but busybox does not
# ship it: on OpenWrt and Alpine the command is simply absent, so the install
# died at the final step -- after the download, the checksum and the extraction
# had all succeeded, which is the worst place to fail because it looks like a
# permissions problem.
#
# cp + chmod is POSIX and present everywhere. The `install` path is kept where
# it exists because it also handles the case of replacing a running binary.
#
# Uses the caller's $need_sudo: a shell function is not visible to `sudo`, so
# the privilege wrapper has to go on each command rather than on the call.
copy_in() {
    _src=$1
    _dst=$2
    _mode=$3
    if command -v install >/dev/null 2>&1; then
        $need_sudo install -m "$_mode" "$_src" "$_dst"
    else
        $need_sudo cp "$_src" "$_dst" && $need_sudo chmod "$_mode" "$_dst"
    fi
}

choose_bin_dir() {
    if [ -n "$BIN_DIR" ]; then
        printf '%s' "$BIN_DIR"
        return
    fi

    # OpenWrt first: its PATH is /usr/sbin:/usr/bin:/sbin:/bin, and
    # /usr/local/bin does not exist there at all. Without this branch the
    # fallback below picks $HOME/.local/bin, which is not on PATH either, so the
    # install "succeeds" and the command is then not found.
    #
    # /usr/bin here lives on the overlay filesystem, so it survives a reboot --
    # which matters on a router that reboots unattended.
    if { [ -f /etc/openwrt_release ] || command -v opkg >/dev/null 2>&1; } \
        && [ "$(id -u)" = "0" ]; then
        printf '/usr/bin'
        return
    fi

    # Prefer /usr/local/bin, fall back to a user-owned directory rather than
    # demanding sudo.
    if [ -w /usr/local/bin ] 2>/dev/null; then
        printf '/usr/local/bin'
    elif [ -d /usr/local/bin ] && [ "$(id -u)" = "0" ]; then
        printf '/usr/local/bin'
    else
        printf '%s/.local/bin' "$HOME"
    fi
}

do_uninstall() {
    dir=$(choose_bin_dir)
    removed=0
    for name in "$BIN_NAME" "$SERVER_BIN_NAME"; do
        path="$dir/$name"
        if [ -e "$path" ]; then
            rm -f "$path" && { info "removed $path"; removed=$((removed + 1)); }
        fi
    done
    if [ "$removed" -eq 0 ]; then
        warn "no innernet binaries found in $dir"
    else
        ok "uninstalled"
    fi
    # Deliberately leaving /etc/innernet and /var/lib/innernet alone: they hold
    # network membership, and re-joining needs a fresh invitation. Point at
    # them rather than deleting someone's access.
    if [ -d /etc/innernet ] || [ -d /var/lib/innernet ]; then
        info ""
        dim "Configuration left in place (/etc/innernet, /var/lib/innernet)."
        dim "Remove manually only if you intend to leave your networks."
    fi
}

main() {
    if [ "$UNINSTALL" -eq 1 ]; then
        do_uninstall
        exit 0
    fi

    target=$(detect_target)
    info "${C_BOLD}innernet${C_RESET} installer"
    dim "  platform: $target"

    if [ -z "$VERSION" ]; then
        dim "  resolving latest release..."
        VERSION=$(resolve_latest_version)
    fi
    # Accept both "v2.1.0" and "2.1.0".
    case "$VERSION" in
        v*) tag="$VERSION"; version="${VERSION#v}" ;;
        *)  tag="v$VERSION"; version="$VERSION" ;;
    esac
    dim "  version:  $tag"

    archive_name="${BIN_NAME}-${version}-${target}.tar.gz"
    base_url="https://github.com/${REPO}/releases/download/${tag}"

    tmpdir=$(mktemp -d "${TMPDIR:-/tmp}/innernet-install.XXXXXX")
    # shellcheck disable=SC2064
    trap "rm -rf '$tmpdir'" EXIT INT TERM

    info ""
    info "downloading ${archive_name}"
    fetch "${base_url}/${archive_name}" "$tmpdir/$archive_name" \
        || die "download failed: ${base_url}/${archive_name}
The release may not include this platform."

    # Checksums are optional: fetch quietly and only verify if present.
    if fetch "${base_url}/SHA256SUMS" "$tmpdir/SHA256SUMS" 2>/dev/null; then
        verify_checksum "$tmpdir/$archive_name" "$tmpdir/SHA256SUMS"
    else
        dim "no SHA256SUMS published for this release; skipping verification"
    fi

    info "extracting"
    tar -xzf "$tmpdir/$archive_name" -C "$tmpdir" \
        || die "could not extract $archive_name"

    # The tarball contains a single top-level directory.
    src_dir="$tmpdir/${BIN_NAME}-${version}-${target}"
    [ -d "$src_dir" ] || die "unexpected archive layout (no $src_dir)"

    dest=$(choose_bin_dir)
    mkdir -p "$dest" 2>/dev/null || true

    # macOS quarantines downloaded executables; without clearing it the
    # binaries are killed on first run with an unhelpful message.
    if [ "$(uname -s)" = "Darwin" ] && command -v xattr >/dev/null 2>&1; then
        xattr -d com.apple.quarantine "$src_dir/$BIN_NAME" 2>/dev/null || true
        xattr -d com.apple.quarantine "$src_dir/$SERVER_BIN_NAME" 2>/dev/null || true
    fi

    info "installing to $dest"
    need_sudo=""
    if [ ! -w "$dest" ]; then
        if [ "$(id -u)" != "0" ] && command -v sudo >/dev/null 2>&1; then
            need_sudo="sudo"
            dim "  (needs elevated permissions)"
        else
            die "cannot write to $dest. Re-run with --bin-dir ~/.local/bin"
        fi
    fi

    for name in "$BIN_NAME" "$SERVER_BIN_NAME"; do
        copy_in "$src_dir/$name" "$dest/$name" 0755 \
            || die "could not install $name to $dest"
    done

    # Man pages are a nice-to-have; never fail the install over them.
    if [ -d "$src_dir/doc" ]; then
        man_dir="/usr/local/share/man/man8"
        if $need_sudo mkdir -p "$man_dir" 2>/dev/null; then
            for page in innernet.8 innernet-server.8; do
                if [ -f "$src_dir/doc/$page" ]; then
                    copy_in "$src_dir/doc/$page" "$man_dir/$page" 0644 2>/dev/null || true
                fi
            done
        fi
    fi

    info ""
    ok "installed innernet $tag"

    installed=$("$dest/$BIN_NAME" --version 2>/dev/null || true)
    [ -n "$installed" ] && dim "  $installed"

    # Warn when the destination is not on PATH, which is the most common
    # "installed but command not found" confusion.
    case ":$PATH:" in
        *":$dest:"*) ;;
        *)
            info ""
            warn "$dest is not on your PATH"
            dim "  add this to your shell profile:"
            dim "    export PATH=\"$dest:\$PATH\""
            ;;
    esac

    info ""
    dim "next steps:"
    dim "  innernet --help"
    dim "  sudo innernet install <invitation>.toml"
    if [ -d /etc/innernet ] && [ "$(uname -s)" = "Darwin" ]; then
        dim ""
        dim "tip: innernet needs root or the 'innernet' group to manage interfaces."
    fi

    # OpenWrt needs the kernel side installed separately: the module is not
    # built into the stock image, and without it `up` fails at the first netlink
    # call with an error that names neither WireGuard nor the missing package.
    if [ -f /etc/openwrt_release ] || command -v opkg >/dev/null 2>&1; then
        info ""
        if [ -e /sys/module/wireguard ]; then
            dim "WireGuard kernel module: present"
        else
            warn "WireGuard kernel module is not loaded"
            dim "  install it first, or 'innernet up' cannot create the interface:"
            dim "    opkg update && opkg install kmod-wireguard"
        fi
        dim ""
        dim "on this system the boot service is a procd init script:"
        dim "  sudo innernet install-service <interface>"
        dim "  /etc/init.d/innernet-<interface> status"
    fi
}

main
