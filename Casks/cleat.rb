# Template for the cask that ships in jettoai/homebrew-tap. `version` and `sha256` come from
# scripts/build-release.sh, which prints both after notarizing.
cask "cleat" do
  version "0.3.0"
  sha256 "a37c45d14acb4df9390a098012d25f4a99bd5e85d4323084130bd5eab2ce4ea6"

  url "https://github.com/jettoai/cleat/releases/download/v#{version}/Cleat-#{version}.zip"
  name "Cleat"
  desc "Keeps audio devices where you declared them"
  homepage "https://github.com/jettoai/cleat"

  depends_on macos: :sonoma

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
