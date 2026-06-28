class Spulse < Formula
  desc "Command-line tool for querying and analyzing Soroban Pulse events"
  homepage "https://github.com/soroban-pulse/soroban-pulse/tree/main/cli"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/soroban-pulse/soroban-pulse/releases/download/spulse-v#{version}/spulse-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_AARCH64_DARWIN"
    end
    on_intel do
      url "https://github.com/soroban-pulse/soroban-pulse/releases/download/spulse-v#{version}/spulse-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_X86_64_DARWIN"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/soroban-pulse/soroban-pulse/releases/download/spulse-v#{version}/spulse-aarch64-unknown-linux-musl.tar.gz"
      sha256 "PLACEHOLDER_SHA256_AARCH64_LINUX"
    end
    on_intel do
      url "https://github.com/soroban-pulse/soroban-pulse/releases/download/spulse-v#{version}/spulse-x86_64-unknown-linux-musl.tar.gz"
      sha256 "PLACEHOLDER_SHA256_X86_64_LINUX"
    end
  end

  def install
    bin.install "spulse"
  end

  # Generate shell completions at install time
  def post_install
    (bash_completion/"spulse").write `#{bin}/spulse --generate bash 2>/dev/null || true`
    (zsh_completion/"_spulse").write `#{bin}/spulse --generate zsh 2>/dev/null || true`
    (fish_completion/"spulse.fish").write `#{bin}/spulse --generate fish 2>/dev/null || true`
  end

  test do
    assert_match "spulse", shell_output("#{bin}/spulse --version")
    system bin/"spulse", "config", "path"
  end
end
