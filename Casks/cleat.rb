# Template for the cask that ships in dreamerhyde/homebrew-tap. `version` and `sha256` come from
# scripts/build-release.sh, which prints both after notarizing.
cask "cleat" do
  version "0.1.0"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"

  url "https://github.com/dreamerhyde/cleat/releases/download/v#{version}/Cleat-#{version}.zip"
  name "Cleat"
  desc "Keeps macOS audio devices where you declared them"
  homepage "https://github.com/dreamerhyde/cleat"

  depends_on macos: ">= :sonoma"

  app "Cleat.app"
  # The app bundle is the CLI too: `cleat status` and `cleat log` are the same binary with a
  # subcommand, so there is nothing else to install.
  binary "#{appdir}/Cleat.app/Contents/MacOS/Cleat", target: "cleat"

  uninstall quit: "ai.jetto.cleat"

  zap trash: [
    "~/Library/Application Support/Cleat",
    "~/Library/Logs/Cleat",
  ]
end
