class Bondebarras < Formula
  desc "TUI to audit and clean up GitHub organization resources"
  homepage "https://github.com/systm-d/bondebarras"
  url "https://github.com/systm-d/bondebarras/archive/refs/tags/v1.0.0-rc.1.tar.gz"
  # Placeholder: release.yml replaces url + sha256 with the real values on tag.
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"
  license "MIT OR Apache-2.0"
  head "https://github.com/systm-d/bondebarras.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: "crates/bondebarras")
  end

  test do
    system bin/"bondebarras", "--version"
  end
end
