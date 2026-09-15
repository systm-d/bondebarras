class Bondebarras < Formula
  desc "TUI to audit and clean up GitHub organization resources"
  homepage "https://github.com/systm-d/bondebarras"
  url "https://github.com/systm-d/bondebarras/archive/refs/tags/v0.5.0.tar.gz"
  sha256 "736dba32a547683dafd77f2adb54492b332b2d90e5d16fdabf766c3826a3629c"
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
