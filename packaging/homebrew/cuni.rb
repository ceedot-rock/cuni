# frozen_string_literal: true

# CuNi — prebuilt binary formula (no Rust toolchain needed).
#
# Install:
#   brew tap ceedot-rock/cuni && brew install cuni
#
# Per-release checklist (maintainer):
#   1. Tag vX.Y.Z on ceedot-rock/cuni → release.yml publishes
#      cuni-X.Y.Z-{aarch64,x86_64}-{apple-darwin,unknown-linux-gnu}.tar.gz + CHECKSUMS.txt
#   2. Bump `version` below and paste the four sha256 lines from CHECKSUMS.txt
#   3. Commit + push here.

class Cuni < Formula
  desc "CuNi — write once, print many languages; Python/Go/JS must match"
  homepage "https://cuni-studio.fly.dev/"
  version "0.10.0"
  license "AGPL-3.0-or-later"

  on_macos do
    on_arm do
      url "https://github.com/ceedot-rock/cuni/releases/download/v0.10.0/cuni-0.10.0-aarch64-apple-darwin.tar.gz"
      sha256 "9422406446e92c007dc1081a43ba1dfc45f8d8906b15faff3732b470eab2eea2"
    end
    on_intel do
      url "https://github.com/ceedot-rock/cuni/releases/download/v0.10.0/cuni-0.10.0-x86_64-apple-darwin.tar.gz"
      sha256 "ddc77bea045ff17e81ffd425eb81fb8b6e0754d8d9413ade0a8e6081300ec156"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/ceedot-rock/cuni/releases/download/v0.10.0/cuni-0.10.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "97d2168947cc6abe60804a2acdbf373c5884a0c702ccb219e6b53059b88c98ca"
    end
    on_intel do
      url "https://github.com/ceedot-rock/cuni/releases/download/v0.10.0/cuni-0.10.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "2d0c569b72ed6ed6e7d45abaf5fe3f19534be3bcda7da243f0f82a1e5f7f74bd"
    end
  end

  def install
    bin.install "cuni"
  end

  test do
    assert_match "cuni 0.10.0", shell_output("#{bin}/cuni version")
  end
end
