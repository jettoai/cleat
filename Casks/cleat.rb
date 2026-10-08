# Template for the cask that ships in jettoai/homebrew-tap. `version` and `sha256` come from
# rs/scripts/build-release.sh, which prints both after notarizing.
cask "cleat" do
  version "0.4.0"
  sha256 "REPLACE_WITH_SHA256_PRINTED_BY_BUILD_RELEASE"

  url "https://github.com/jettoai/cleat/releases/download/v#{version}/Cleat-#{version}.zip"
  name "Cleat"
  desc "Keeps audio devices where you declared them"
  homepage "https://github.com/jettoai/cleat"

  depends_on macos: :sonoma

  app "Cleat.app"
  # The app bundle is the CLI too: `cleat status` and `cleat log` are the same binary with a
  # subcommand, so there is nothing else to install.
  binary "#{appdir}/Cleat.app/Contents/MacOS/Cleat", target: "cleat"

  # The daemon is kept alive by a launchd agent inside the bundle, so removing the app means
  # unloading the job as well - otherwise launchd is left holding a label whose program is gone.
  uninstall launchctl: "ai.jetto.cleat",
            quit:      "ai.jetto.cleat"

  # An upgrade quits the old daemon, and a clean quit is exactly what KeepAlive leaves alone, so
  # the new one has to be asked for. The kickstart is the normal path and works once the agent has
  # been registered; on a first install, or a first upgrade from a version that had no agent,
  # there is no job to kick, so the app itself is started instead. That copy registers the agent
  # and then hands over to the one launchd starts, so either way exactly one daemon is left.
  postflight do
    result = system_command "/bin/launchctl",
                            args:         ["kickstart", "-k", "gui/#{Process.uid}/ai.jetto.cleat"],
                            must_succeed: false
    unless result.success?
      system_command "/usr/bin/open",
                     args:         ["-g", "#{appdir}/Cleat.app"],
                     must_succeed: false
    end
  end

  zap trash: [
    "~/Library/Application Support/Cleat",
    # Left by the Sentry SDK of the Swift builds (0.3.x); the Rust build writes nothing here.
    "~/Library/Caches/ai.jetto.cleat",
    "~/Library/Logs/Cleat",
  ]
end
