# Cleat

A cleat is the fitting a rope gets tied to so the boat stops drifting. This one is for audio
devices: you declare the state you want in a config file, and Cleat holds it, event driven, for
about zero percent of a CPU.

macOS keeps moving audio devices on you. Connecting AirPods Max takes over the microphone (and
drops Bluetooth into call-quality HFP). Conferencing apps "automatically adjust microphone volume"
and leave the gain somewhere else. Balance drifts off centre after some reconnects. And a wireless
receiver whose transmitter is switched off is still a perfectly good CoreAudio device that happens
to be sending nothing at all.

Cleat does six things, all the same shape: subscribe to a CoreAudio property, compare against the
config, write back if they differ.

| | What | Config |
|---|---|---|
| 1 | Input device priority list, with a blocklist | `input`, `blockedInput` |
| 2 | Treat a device sending exact digital silence as absent | `liveness` |
| 3 | Hold the output balance | `balance` |
| 4 | Output device priority list | `output` |
| 5 | Hold input gain per device | `inputVolume` |
| 6 | Bluetooth headphones take over the output when they connect | `headphonesTakeOver` |

It never takes over a device you picked yourself. If the current default input is not on your
priority list and not on the blocklist - a mic you chose in System Settings, Zoom's or Teams'
virtual device - Cleat leaves it alone.

## Install

```sh
brew tap jettoai/tap
# Homebrew requires third-party taps to be trusted before it will load their casks.
brew trust jettoai/tap
brew install --cask cleat
```

Or download the zip from [Releases](https://github.com/jettoai/cleat/releases), unzip it into
`/Applications`, and open it once.

Then write a config and start it:

```sh
mkdir -p ~/.config/cleat
cp /Applications/Cleat.app/Contents/Resources/config.example.json ~/.config/cleat/config.json
open -a Cleat
```

Cleat has no window. It registers itself as a login item (unless `launchAtLogin` is `false`) and
runs from then on.

## Config

`~/.config/cleat/config.json`, re-read within a second of being saved:

```json
{
  "input": ["Wireless microphone", "Brio 100"],
  "blockedInput": ["AirPods Max"],
  "output": ["外接耳機", "Mac Studio的揚聲器"],
  "blockedOutput": ["Maono AI Microphone"],
  "headphonesTakeOver": true,
  "balance": 0.5,
  "inputVolume": { "*": 100, "Wireless microphone": 88, "Brio 100": 75 },
  "liveness": { "Wireless microphone": { "zeroSeconds": 3 } },
  "launchAtLogin": true
}
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `input` | array of strings | `[]` | Input priority, most preferred first. Empty turns the rule off |
| `blockedInput` | array of strings | `[]` | Never allowed to be the default input |
| `output` | array of strings | `[]` | Output priority. Empty turns the rule off |
| `blockedOutput` | array of strings | `[]` | Never allowed to be the default output |
| `headphonesTakeOver` | boolean | `false` | Bluetooth output devices take the output when they connect |
| `balance` | number or null | `null` | 0.0 (left) to 1.0 (right); 0.5 is centred. `null` turns the rule off |
| `inputVolume` | object | `{}` | Device name, or `"*"` for every input device, to percent, 0-100 |
| `liveness` | object | `{}` | Device name to `{ "zeroSeconds": N }`, N at least 1 |
| `launchAtLogin` | boolean | `true` | Register with SMAppService as a login item |

**Input gain.** `"*"` sets the target for every input device present, and a named entry overrides
it for that device: `{"*": 100, "Brio 100": 75}` holds everything at 100 percent except the Brio,
which is held at 75. Without a `"*"` entry, a device the config does not name is left alone. The
wildcard covers blocked devices too, so an AirPods Max kept out of the input slot by `blockedInput`
still has its gain held, and devices whose gain cannot be read - some virtual devices - are left
alone either way.

**Headphones.** When a Bluetooth output device appears, it becomes the output. Choosing another
device by hand while it stays connected is respected. `output` then only decides what plays when no
headphones are around. macOS does this for wired headphones already and iOS does it for AirPods;
over Bluetooth on a Mac, reconnecting a headset that was last paired to a phone leaves the sound
coming out of the speakers, which is the gap this fills. Headphones already connected when Cleat
starts are not treated as having just arrived, so restarting Cleat never moves the output.

`blockedOutput` is the other half of it. Some USB microphones carry a speaker end, and that is
where macOS lands when the headphones leave. It is not on your priority list, so without the
blocked list Cleat would read it as an output you picked yourself and leave it there.

**Naming a device.** Use the name shown in System Settings, or its CoreAudio UID if two devices
share a name. Names are compared after Unicode normalisation, because some devices carry a
no-break space in their name that you cannot type (Maono's, for one). Case is not normalised.

A malformed or out-of-range config never replaces a working one: the previous settings stay in
force and `cleat status` says why. Before any config has loaded, every rule is off. Removing the
file also turns every rule off - that is the way to switch Cleat off without quitting it.

**Silence detection** (`liveness`) is the one feature that opens the microphone. Cleat runs a HAL
IOProc on the listed device and checks whether every sample in the buffer is exactly zero - a real
microphone always has some noise floor, a receiver with its transmitter off sends nothing. After
`zeroSeconds` of that, the device counts as absent and the next device on the priority list takes
over. Because the input is open, macOS shows the orange microphone dot while Cleat is watching.

## Commands

The app bundle is the CLI. The Homebrew cask links it as `cleat`.

```sh
cleat status      # what it is holding right now, and why
cleat log -n 50   # recent events
cleat version
```

`cleat status` reads `~/Library/Application Support/Cleat/status.json`; `cleat log` reads
`~/Library/Logs/Cleat/cleat.log`. Only actions that changed something are logged, so a quiet log
means a quiet day, not a broken daemon - `status` is what tells you it is alive.

## Microphone permission

Only `liveness` needs it. macOS asks the first time Cleat runs; if you decline, every other rule
keeps working and `cleat status` shows `microphone: denied`. To change your mind: System Settings
> Privacy & Security > Microphone, then restart Cleat - it reads the permission at launch and does
not watch that switch.

This is also why Cleat is an .app rather than a bare binary on a LaunchAgent - a command-line tool
started by launchd is often never asked, and the request fails silently instead.

## Build from source

```sh
brew install xcodegen
xcodegen generate
xcodebuild build -project Cleat.xcodeproj -scheme Cleat -destination 'platform=macOS' -quiet
xcodebuild test  -project Cleat.xcodeproj -scheme Cleat -destination 'platform=macOS' -quiet
```

`scripts/build-release.sh` produces the signed, notarized zip that the cask points at.

## Uninstall

```sh
brew uninstall --cask --zap cleat
```

Or, for a manual install: quit Cleat, delete `/Applications/Cleat.app`, and remove
`~/Library/Application Support/Cleat`, `~/Library/Logs/Cleat` and `~/.config/cleat`.

## License

MIT
