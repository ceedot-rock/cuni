# frozen_string_literal: true

# DRAFT — not published to a public tap yet.
#
# Preferred install today:
#   cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.7
#
# Local test of this formula (no tap needed):
#   brew install --build-from-source ./packaging/homebrew/cuni.rb
#
# Before publishing a tap:
#   1. Create GitHub Release assets (see docs/PACKAGING.md)
#   2. Compute sha256 of the source tarball:
#        curl -sL https://github.com/ceedot-rock/cuni/archive/refs/tags/v0.1.7.tar.gz | shasum -a 256
#   3. Replace REPLACE_WITH_TAG_TARBALL_SHA256 below
#   4. Push to ceedot-rock/homebrew-cuni → Formula/cuni.rb

class Cuni < Formula
  desc "CuNi — exact multi-target language (py/go/js or refuse)"
  homepage "https://github.com/ceedot-rock/cuni"
  url "https://github.com/ceedot-rock/cuni/archive/refs/tags/v0.1.7.tar.gz"
  sha256 "REPLACE_WITH_TAG_TARBALL_SHA256"
  license "MIT"
  head "https://github.com/ceedot-rock/cuni.git", branch: "master"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match(/cuni/i, shell_output("#{bin}/cuni --help"))
  end
end
