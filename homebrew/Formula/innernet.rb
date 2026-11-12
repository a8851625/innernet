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
  version "2.1.0"
  license "MIT"

  # Each architecture is a separate release archive. Both are declared inside
  # the on_* blocks so Homebrew picks the matching pair of url+sha256; a
  # top-level url plus an on_intel override would leave the checksum
  # ambiguous.
  on_arm do
    url "https://github.com/a8851625/innernet/releases/download/v2.1.0/innernet-2.1.0-aarch64-apple-darwin.tar.gz"
    sha256 "5b75242b0917effabf4724ee241606a7d2834160b23816114bc1f126e5a1abdb"
  end

  on_intel do
    url "https://github.com/a8851625/innernet/releases/download/v2.1.0/innernet-2.1.0-x86_64-apple-darwin.tar.gz"
    sha256 "PLACEHOLDER_X86_64_MACOS"
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
    # The release archive contains a single top-level directory named after
    # the target triple; Homebrew extracts into the build directory, so find
    # the directory that actually holds the binaries.
    root = Dir["*/"].first || "."
    bin.install "#{root}innernet"
    bin.install "#{root}innernet-server"

    # Man pages ship as .8 and .8.gz depending on platform; install whichever
    # is present.
    if File.directory?("#{root}doc")
      # Archives ship both "x.8" and "x.8.gz" for the same page. Decompress the
      # .gz variants, then install the plain ones only, so each page is
      # installed exactly once. Homebrew's man.install expects uncompressed
      # files.
      Dir["#{root}doc/*.8.gz"].each { |gz| system "gunzip", "-f", gz }
      Dir["#{root}doc/*.8"].each { |page| man8.install page }

      # Shell completions, if this platform's archive carried them.
      completions = Dir["#{root}doc/*.completions.*"]
      completions.each do |file|
        case file
        when /\.bash$/      then bash_completion.install file
        when /\.fish$/      then fish_complete_path.install file
        when /\.elvish$/    then elvish_complete_path.install file
        when /\.ps1$/       then pwsh_completion.install file
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
