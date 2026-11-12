# Homebrew tap for this fork

The formula lives here so that CI can rewrite it, but it is published to a
separate repository:

```sh
brew tap a8851625/innernet
brew install innernet
```

That tap repository is `a8851625/homebrew-innernet`. It has to be separate
because Homebrew finds formulae in a repository named `homebrew-<something>`,
and this repository is named `innernet`.

## How the formula gets updated

You do not edit `Formula/innernet.rb` by hand on every release.

The release workflow rewrites it after the archives have been published:

1. `cli-server` builds every target and uploads the archives to the Release.
2. `checksums` merges the per-platform fragments into `SHA256SUMS`.
3. `homebrew` downloads that file, runs `scripts/update-homebrew-formula.py`
   to write the new version and both macOS checksums into the formula, and
   pushes the result to the tap.

The version and the checksums therefore cannot disagree with the archives,
because both are read from the same file the release published.

## Why it is automated

It used to be four manual steps: bump the version, update two URLs, compute two
checksums, push to the tap. Skipping them is invisible. The tap sat three
releases behind at 2.2.2 while the release workflow kept going green on 2.3.0,
2.3.1 and 2.3.2, so `brew install` silently handed out a binary that was missing
the subnet-routing and web-admin work entirely.

Two guards now fail loudly instead:

- The `homebrew` job refuses to run unless both macOS archives actually exist on
  the release, so a half-published tag cannot reach the tap.
- `rust.yml` compares the formula's version against `client/Cargo.toml` on every
  push, so a formula that was left behind fails CI rather than sitting stale.

## One-time setup: TAP_TOKEN

The workflow's own `GITHUB_TOKEN` cannot write to another repository, so the
push needs a separate token.

1. Create a fine-grained personal access token at
   <https://github.com/settings/personal-access-tokens/new>:
   - **Repository access**: only `a8851625/homebrew-innernet`
   - **Permissions**: `Contents: Read and write`
   - Nothing else is needed.
2. Store it on this repository as a secret named `TAP_TOKEN`:

   ```sh
   gh secret set TAP_TOKEN --repo a8851625/innernet
   ```

Without it the `homebrew` job fails with an explicit message rather than
skipping silently, which is the point: a release that does not reach the tap
should look broken, not finished.

## Doing it by hand

If the token is unavailable, the same script runs locally:

```sh
# from a checkout, with the release's checksums on disk
gh release download v2.3.2 --pattern SHA256SUMS --clobber
python3 scripts/update-homebrew-formula.py \
  --version 2.3.2 --sums SHA256SUMS --formula homebrew/Formula/innernet.rb

# then copy it into the tap and push
```

`--check` reports whether the formula is already current without writing
anything, which is what CI uses.

## Notes and traps

- **Validate with `ruby -c`, not `brew`.** On some machines `brew style` and
  `brew ruby` corrupt Homebrew's own vendored gems (native extensions built for
  a different Ruby end up in the portable-ruby bundle, after which `brew tap`
  fails with an unrelated `json` error). `ruby -c` catches syntax problems
  without touching Homebrew's internals.
- **Each architecture declares its own `url` + `sha256` inside `on_arm` /
  `on_intel`.** A top-level `url` with an `on_intel` override leaves the
  checksum ambiguous.
- **Do not glob for the archive root.** `Dir["*/"]` matches `doc/` too, once
  inside the extracted tree; locate the binary instead.
- **Uncompressing matters.** Archives ship both `innernet.8` and
  `innernet.8.gz`, so decompress first and install only the plain `*.8`, or
  every man page installs twice.
- **One tap at a time.** Homebrew refuses two same-named formulae from different
  taps, so `brew untap tonarino/innernet` before installing this one.
