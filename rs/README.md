# Cleat, in Rust

A port of the Swift daemon in `Cleat/`, with the same config file (`~/.config/cleat/config.json`),
status file (`~/Library/Application Support/Cleat/status.json`) and log
(`~/Library/Logs/Cleat/cleat.log`). A development build writes those same files, so run it beside an
installed Cleat with `run --observe` only.

```bash
cargo test
bash scripts/bundle.sh                 # target/bundle.noindex/Cleat.app, ad-hoc signed, bundle id ai.jetto.cleat.rs
target/bundle.noindex/Cleat.app/Contents/MacOS/Cleat status
```

The CLI (`status`, `log`, `restart`, `version`, `help`) prints what the Swift `cleat` prints.

- No arguments (or flag-shaped ones only): the daemon, writing to CoreAudio. `--trace` adds the
  `pass:`, `event:` and `listeners:` diagnostics to the log.
- `run --observe`: decides and logs every action with `[observe: not applied]`, writes nothing,
  never touches the launch agent. Always traces. For running beside the Swift daemon.

`ai.jetto.cleat.rs` is a development identity: it never registers a login agent. To test launchd
supervision, bootstrap a plist of your own (Label `ai.jetto.cleat.rs`, Program the bundle's
binary, `KeepAlive` `SuccessfulExit` false) and `launchctl bootout` it afterwards.

Opening the app from Finder, Spotlight or Raycast while that launchd job exists opens the
settings window instead of a second daemon (the Dock shows the icon while it is open). With no
job, it starts the daemon. Launch that way with a clean environment
(`env -i HOME="$HOME" PATH=/usr/bin:/bin open -g …`): `open` passes the caller's environment to
the app.

The daemon shows a menu bar item (icon: `bundle/MenuBarIcon.png` and `@2x`, copied into the bundle
by `scripts/bundle.sh`; replace them to change it). Its menu shows the current devices, opens the
settings window (one at a time), shows About, and quits. Quit stays down until the next login or
until the app is opened again; install.sh sets the agent's `KeepAlive` to `SuccessfulExit` false
for that.

`bash scripts/install.sh` copies `target/bundle.noindex/Cleat.app` to `/Applications/Cleat.app` (moving aside an
`/Applications/Cleat-rs.app` from before the rename), where Raycast finds it, points the existing `~/Library/LaunchAgents/ai.jetto.cleat.rs.plist` at that
copy and restarts the daemon. The bundle is ad-hoc signed, so a rebuilt one may ask for the
microphone again. Builds and moved-aside copies stay under `target/bundle.noindex/`, which
Spotlight skips, so Raycast lists only the installed one.
