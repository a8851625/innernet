# Homebrew formula for this innernet fork.
#
# This lives in the tap repository `a8851625/homebrew-innernet`, so that
#
#   brew tap a8851625/innernet
#   brew install innernet
#
# installs prebuilt binaries from GitHub Releases instead of compiling from
# source. Compiling would drag in the whole Rust toolchain and take minutes;
# the release archives already contain exactly these two binaries.

class Innernet < Formula
  desc "Private networks over WireGuard, with subnet routing and a web admin UI"
  homepage "https://github.com/a8851625/innernet"
  version "2.3.2"
  license "MIT"

  # Each architecture is a separate release archive. Both are declared inside
  # the on_* blocks so Homebrew picks the matching pair of url+sha256; a
  # top-level url plus an on_intel override would leave the checksum
  # ambiguous.
  on_arm do
    url "https://github.com/a8851625/innernet/releases/download/v2.3.2/innernet-2.3.2-aarch64-apple-darwin.tar.gz"
    sha256 "a2bc1394d4fd2851db4b22d597ba4d265eac9058b0a7466ba83a8a0449a841f9"
  end

  on_intel do
    url "https://github.com/a8851625/innernet/releases/download/v2.3.2/innernet-2.3.2-x86_64-apple-darwin.tar.gz"
    sha256 "5127f534f3dbd0525d85741da00ffa68430d0fb9647490608bf37788351fe4c9"
  end

  livecheck do
    url :url
    strategy :github_latest
  end

  # The client shells out to the `innernet` CLI, and the userspace WireGuard
  # backend needs a userspace implementation on macOS (there is no kernel
  # WireGuard module).
  depends_on "wireguard-go"

  def install
    # The archive holds one directory named after the target triple. Locate it
    # by finding the binary rather than taking the first directory: Dir["*/"]
    # also matches sibling dirs, and inside the extracted tree "doc/" is a
    # sibling too, which previously resolved the root to "doc" and failed with
    # ENOENT on "doc/innernet".
    binary = Pathname.glob("**/innernet")
                     .reject(&:directory?)
                     .min_by { |p| p.to_s.count("/") }
    odie "innernet binary not found in the release archive" unless binary

    root = binary.dirname
    bin.install root/"innernet"
    bin.install root/"innernet-server"

    if (root/"doc").directory?
      # Archives ship both "x.8" and "x.8.gz" for the same page. Decompress
      # the .gz variants first, then install only the plain files, so each page
      # lands exactly once. man8.install expects uncompressed input.
      Pathname.glob(root/"doc/*.8.gz").each { |gz| system "gunzip", "-f", gz.to_s }
      Pathname.glob(root/"doc/*.8").each { |page| man8.install page }

      # Shell completions, when the archive carried them.
      # Homebrew exposes bash/zsh/fish/pwsh completion dirs; it has no elvish
      # location, so elvish completions are skipped rather than failing.
      Pathname.glob(root/"doc/*.completions.*").each do |file|
        case file.to_s
        when /\.bash$/     then bash_completion.install file
        when /\.zsh$/      then zsh_completion.install file
        when /\.fish$/     then fish_completion.install file
        when /powershell$/ then pwsh_completion.install file
        end
      end
    end
  end

  def caveats
    <<~EOS
      innernet needs elevated privileges to create WireGuard interfaces.

      Either run it with sudo:

        sudo innernet install <invitation>.toml

      or grant the capability once and use it unprivileged:

        sudo setcap cap_net_admin+eip #{opt_bin}/innernet

      Config lives in /etc/innernet and state in /var/lib/innernet.
    EOS
  end

  test do
    assert_match "innernet", shell_output("#{bin}/innernet --version")
    # `show --json` is the machine-readable interface the GUI depends on;
    # exercising it catches a stripped-down build.
    (testpath/"cfg").mkpath
    (testpath/"dat").mkpath
    output = shell_output(
      "#{bin}/innernet --config-dir #{testpath}/cfg --data-dir #{testpath}/dat show --json"
    )
    assert_match '"interfaces"', output
  end
end
