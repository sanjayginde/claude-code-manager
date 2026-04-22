# Homebrew formula for ccm (Claude Code Manager).
#
# To publish via a personal tap:
#   1. Create a repo named `homebrew-tap` under your GitHub account
#      (e.g. github.com/sanjayginde/homebrew-tap).
#   2. Cut a release tag (e.g. `v0.1.0`) on this repo so a source tarball
#      is available at the `url` below.
#   3. Compute the tarball sha256:
#        curl -L https://github.com/sanjayginde/claude-code-manager/archive/refs/tags/v0.1.0.tar.gz | shasum -a 256
#   4. Copy this file to `Formula/ccm.rb` in the tap repo and replace the
#      sha256 placeholder with the real value.
#   5. Users install with:
#        brew install sanjayginde/tap/ccm
#
# Updating for a new release: bump `url` to the new tag, update `sha256`,
# and push to the tap. `brew bump-formula-pr` can automate this.
class Ccm < Formula
  desc "Terminal UI for browsing, managing, and resuming Claude Code sessions"
  homepage "https://github.com/sanjayginde/claude-code-manager"
  url "https://github.com/sanjayginde/claude-code-manager/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "REPLACE_WITH_RELEASE_TARBALL_SHA256"
  license "MIT"
  head "https://github.com/sanjayginde/claude-code-manager.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  def caveats
    <<~EOS
      ccm resumes sessions by exec'ing `claude --resume <uuid>`, so it
      requires the Claude Code CLI to be installed and on your PATH:
        https://docs.claude.com/en/docs/claude-code

      To enable AI-generated session titles (optional), set:
        export ANTHROPIC_API_KEY=...
      Without the key, ccm falls back to showing a truncated first message.
    EOS
  end

  test do
    # `ccm` is a TUI with no --help/--version flags today; just assert the
    # binary is executable and present.
    assert_predicate bin/"ccm", :exist?
    assert_predicate bin/"ccm", :executable?
  end
end
