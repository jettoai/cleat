# Cleat, in Rust

A port of the Swift daemon in `Cleat/`. Same config file (`~/.config/cleat/config.json`), its own
status file and log (`~/Library/Application Support/Cleat-rs/status.json`,
`~/Library/Logs/Cleat-rs/cleat-rs.log`) so it can run beside the Swift build.

```bash
cargo test
bash scripts/bundle.sh                 # target/bundle.noindex/Cleat-rs.app, ad-hoc signed, bundle id ai.jetto.cleat.rs
target/bundle.noindex/Cleat-rs.app/Contents/MacOS/Cleat-rs status
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

`bash scripts/install.sh` copies `target/bundle.noindex/Cleat-rs.app` to `/Applications/Cleat-rs.app` (beside the
Swift `Cleat.app`), where Raycast finds it, points the existing `~/Library/LaunchAgents/ai.jetto.cleat.rs.plist` at that
copy and restarts the daemon. The bundle is ad-hoc signed, so a rebuilt one may ask for the
microphone again. Builds and moved-aside copies stay under `target/bundle.noindex/`, which
Spotlight skips, so Raycast lists only the installed one.
