# Homebrew formula for the `domtree` CLI, installed from the prebuilt binaries
# that .github/workflows/release.yml attaches to each GitHub release.
# --- template notes (fill-formula.sh removes this block) ---
#
# THIS FILE IS A TEMPLATE. The sha256 values below are PLACEHOLDERS (all
# zeros). For every `v*` tag, the `homebrew` job in release.yml runs
# fill-formula.sh to put in the version and the real checksums of the release
# assets, prints the result in its job summary and uploads it as the
# `homebrew-formula` artifact. Copy that output to `Formula/domtree.rb` in the
# iamsaquib8/homebrew-tap repository; see packaging/homebrew/README.md.
#
# The `# PLACEHOLDER: <target>` markers are what fill-formula.sh looks for —
# keep one on each sha256 line.
# --- end template notes ---
class Domtree < Formula
  desc "Query HTML with CSS selectors: elements, attributes, text, Markdown or JSON"
  homepage "https://github.com/iamsaquib8/dom-tree-rs"
  version "0.3.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/iamsaquib8/dom-tree-rs/releases/download/v#{version}/domtree-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000" # PLACEHOLDER: aarch64-apple-darwin
    end
    on_intel do
      url "https://github.com/iamsaquib8/dom-tree-rs/releases/download/v#{version}/domtree-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000" # PLACEHOLDER: x86_64-apple-darwin
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/iamsaquib8/dom-tree-rs/releases/download/v#{version}/domtree-v#{version}-aarch64-unknown-linux-musl.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000" # PLACEHOLDER: aarch64-unknown-linux-musl
    end
    on_intel do
      url "https://github.com/iamsaquib8/dom-tree-rs/releases/download/v#{version}/domtree-v#{version}-x86_64-unknown-linux-musl.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000" # PLACEHOLDER: x86_64-unknown-linux-musl
    end
  end

  def install
    # Each archive holds a single `domtree-v<version>-<target>/` directory,
    # which Homebrew has already entered.
    bin.install "domtree"
    doc.install "README.md"
  end

  test do
    assert_match "domtree #{version}", shell_output("#{bin}/domtree --version")

    html = "<ul><li class='x'>a</li><li>b &amp; c</li></ul>"
    assert_equal "a\nb & c\n", pipe_output("#{bin}/domtree -t li", html)
    assert_equal "2\n", pipe_output("#{bin}/domtree -c li", html)
    assert_equal "x\n", pipe_output("#{bin}/domtree -a class li", html)
    assert_equal "- a\n- b & c\n", pipe_output("#{bin}/domtree --markdown ul", html)
  end
end
