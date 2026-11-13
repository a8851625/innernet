#!/bin/sh
# Put the CLI inside the built app so installing the app is enough.
#
# Tauri's own externalBin mechanism is the obvious fit, but it validates the
# file during the build script, which makes every plain `cargo build` and
# `cargo test` in gui/src-tauri fail on a fresh clone unless the binary was
# built first. Injecting after the bundle is built keeps the developer loop
# clean and produces the same result.
#
# Usage: inject-cli.sh <app-path> <cli-binary>
set -eu

app=$1
cli=$2

[ -d "$app" ] || { echo "no such app bundle: $app" >&2; exit 1; }
[ -f "$cli" ] || { echo "no such CLI: $cli" >&2; exit 1; }

case "$app" in
  *.app)
    dest="$app/Contents/MacOS/innernet"
    ;;
  *)
    # Windows/Linux layouts put the executable beside its siblings.
    dest="$(dirname "$app")/innernet"
    ;;
esac

cp "$cli" "$dest"
chmod +x "$dest"
echo "injected $(basename "$cli") -> $dest"
"$dest" --version || true

# Adding a file invalidates the bundle signature, and macOS refuses to launch
# an app whose contents changed after signing. Re-sign ad hoc: this is not a
# Developer ID signature, so the app is still unsigned in Gatekeeper's eyes,
# but it is self-consistent and launches.
if [ "$(uname -s)" = Darwin ]; then
  echo "re-signing the bundle"
  codesign --force --deep --sign - "$app"
  codesign --verify --deep --strict "$app" && echo "signature valid"
fi
