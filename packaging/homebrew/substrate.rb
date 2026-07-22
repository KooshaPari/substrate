class Substrate < Formula
  desc "Policy-driven task dispatch gateway and CLI"
  homepage "https://github.com/KooshaPari/substrate"
  version "0.3.0"
  license "MIT"

  on_arm do
    url "https://github.com/KooshaPari/substrate/releases/download/v#{version}/substrate-#{version}-arm64-darwin.tar.gz"
    sha256 "ba6ea0a5738c74bd480163b19eb3f1f02739b42d4f3bcc5e6be9e484d8dd7554"
  end

  def install
    bin.install "substrate"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/substrate --version")
  end
end
