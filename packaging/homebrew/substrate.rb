# packaging/homebrew/substrate.rb — Homebrew formula for `substrate`
# Submit: brew tap KooshaPari/phenotype https://github.com/KooshaPari/homebrew-phenotype
#         brew install substrate
class Substrate < Formula
  desc "Hexagonal dispatch spine for the Phenotype fleet (CLI / HTTP / MCP)"
  homepage "https://github.com/KooshaPari/substrate"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/KooshaPari/substrate/releases/download/v#{version}/substrate-macos-arm64.tar.gz"
      sha256 "PLACEHOLDER_ARM64_SHA"
    end
    on_intel do
      url "https://github.com/KooshaPari/substrate/releases/download/v#{version}/substrate-macos-x86_64.tar.gz"
      sha256 "PLACEHOLDER_X86_64_SHA"
    end
  end

  on_linux do
    url "https://github.com/KooshaPari/substrate/releases/download/v#{version}/substrate-linux-x86_64.tar.gz"
    sha256 "PLACEHOLDER_LINUX_SHA"
  end

  def install
    bin.install "substrate"
  end

  test do
    assert_match "substrate", shell_output("#{bin}/substrate --version")
  end
end