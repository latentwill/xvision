# typed: false
# frozen_string_literal: true

# Homebrew formula for xvn — the xvision CLI + dashboard.
#
# Install: brew install latentwill/xvision/xvn
#
# The release workflow auto-commits formula updates on tag push, regenerating
# the `url` and `sha256` lines for each platform bottle below.

class Xvn < Formula
  desc "Self-hosted trading strategy engine with dashboard and optimizer"
  homepage "https://github.com/latentwill/xvision"
  version "0.38.0"
  license "Apache-2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/latentwill/xvision/releases/download/v0.38.0/xvn-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_ACTUAL_SHA256_ARM64"
    else
      url "https://github.com/latentwill/xvision/releases/download/v0.38.0/xvn-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_ACTUAL_SHA256_X86_64"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/latentwill/xvision/releases/download/v0.38.0/xvn-x86_64-linux-musl.tar.gz"
      sha256 "REPLACE_WITH_ACTUAL_SHA256_LINUX"
    end
  end

  def install
    bin.install "xvn"
  end

  test do
    system "#{bin}/xvn", "--version"
  end
end
