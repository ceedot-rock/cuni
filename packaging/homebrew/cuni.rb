# frozen_string_literal: true

# DRAFT — not published to a public tap yet.
#
# Preferred install today:
#   cargo install cuni
#   cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.10
#
# Local test of this formula (no tap needed):
#   brew install --build-from-source ./packaging/homebrew/cuni.rb
#
# Before publishing a tap:
#   1. Create GitHub Release assets (see docs/PACKAGING.md)
#   2. sha256 of the source tarball is set below (v0.1.10)
#   3. Push to ceedot-rock/homebrew-cuni → Formula/cuni.rb

class Cuni < Formula
  desc "CuNi — write once, print many languages; Python/Go/JS must match"
  homepage "https://cuni-studio.fly.dev/"
  url "https://github.com/ceedot-rock/cuni/archive/refs/tags/v0.1.10.tar.gz"
  sha256 "REPLACE_AFTER_TAG"
  license "AGPL-3.0-or-later"
  head "https://github.com/ceedot-rock/cuni.git", branch: "master"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match(/cuni/i, shell_output("#{bin}/cuni --help"))
  end
end
