# frozen_string_literal: true

# DRAFT — not published to a tap yet.
# Install locally: brew install --build-from-source ./packaging/homebrew/cuni.rb
# Or preferred today: cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.7

class Cuni < Formula
  desc "CuNi — exact multi-target language (py/go/js or refuse)"
  homepage "https://github.com/ceedot-rock/cuni"
  url "https://github.com/ceedot-rock/cuni/archive/refs/tags/v0.1.7.tar.gz"
  # Update after: curl -sL <url> | shasum -a 256
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
