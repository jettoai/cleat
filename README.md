# Cleat

A cleat is the fitting a rope gets tied to so the boat stops drifting. This one is for audio
devices: you declare the state you want in a config file, and Cleat holds it, event driven, for
about zero percent of a CPU.

macOS keeps moving audio devices on you. Connecting AirPods Max takes over the microphone (and
drops Bluetooth into call-quality HFP). Conferencing apps "automatically adjust microphone volume"
and leave the gain somewhere else. Balance drifts off centre after some reconnects. And a wireless
receiver whose transmitter is switched off is still a perfectly good CoreAudio device that happens
to be sending nothing at all.

Cleat does seven things. Six are the same shape: subscribe to a CoreAudio property, compare
against the config, write back if they differ. The seventh asks another daemon for a headset back,
because a headset a phone has taken is not a CoreAudio property to write.

| | What | Config |
|---|---|---|
| 1 | Input device priority list, with a blocklist | `input`, `blockedInput` |
| 2 | Treat a device sending exact digital silence as absent | `liveness` |
| 3 | Hold the output balance | `balance` |
| 4 | Output device priority list | `output` |
| 5 | Hold input gain per device | `inputVolume` |
| 6 | Bluetooth headphones take over the output when they connect | `headphonesTakeOver` |
| 7 | Ask a Bluetooth headset back when a phone has taken it | `reclaim` |
| 8 | Undo output volume changes made by listed apps | `outputVolumeHoldAgainst` |

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

**Read the example before you copy it.** It is the author's own setup, not a neutral starting
point: it holds every input device's gain at 100 percent (`"inputVolume": {"*": 100, ...}`) and it
turns headphone takeover on, so any Bluetooth headset becomes the output as soon as it connects.
The device names in it are the author's, so the rules that name devices do nothing on your machine
until you put your own there - but two settings in it name no device and take effect at once:
`balance` pulls whatever output you are on back to centre, and `launchAtLogin` registers Cleat as a
login item. Edit it for your devices first, or start from `{}` and add one rule at a time.

```sh
mkdir -p ~/.config/cleat
cp /Applications/Cleat.app/Contents/Resources/config.example.json ~/.config/cleat/config.json
cleat restart
```

Cleat has no window. `cleat restart` registers the launchd agent that ships inside the bundle and
starts the daemon through it; from then on launchd starts Cleat at login and starts it again if it
is ever killed or crashes. A clean quit is left alone on purpose - that is what `brew upgrade`
does to the old copy, and the cask starts the new one for you. `cleat status` says which agent
state you are in. Setting `launchAtLogin` to `false` unregisters the agent, and Cleat exits with
it.

Installing by hand instead: open the app once from Finder, which is both the first microphone
prompt and the moment it registers its agent. A copy started by hand while the agent is already
running steps aside for the supervised one, so there is never a second daemon.

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
  "outputVolumeHoldAgainst": ["Parallels Desktop"],
  "liveness": { "Wireless microphone": { "zeroSeconds": 3 } },
  "reclaim": ["AirPods Max"],
  "launchAtLogin": true,
  "errorReports": false
}
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `input` | array of strings | `[]` | Input priority, most preferred first. Empty turns the rule off |
| `blockedInput` | array of strings | `[]` | Never allowed to be the default input |
| `output` | array of strings | `[]` | Output priority. Empty turns the rule off |
| `blockedOutput` | array of strings | `[]` | Never allowed to be the default output |
| `headphonesTakeOver` | boolean | `false` (the example turns it on) | Bluetooth output devices take the output when they connect |
| `balance` | number or null | `null` | 0.0 (left) to 1.0 (right); 0.5 is centred. `null` turns the rule off |
| `inputVolume` | object | `{}` | Device name, or `"*"` for every input device, to percent, 0-100 |
| `liveness` | object | `{}` | Device name to `{ "zeroSeconds": N }`, N at least 1 |
| `outputVolumeHoldAgainst` | array of strings | `[]` | Apps (e.g. `"Parallels Desktop"`) or executable names (e.g. `"prl_vm_app"`) whose changes to the output volume are undone within half a second. Bluetooth output only. Empty turns the rule off |
| `reclaim` | array of strings | `[]` | Bluetooth headsets to ask back when another device holds them, by name or address |
| `launchAtLogin` | boolean | `true` | Register the launchd agent that starts Cleat at login and restarts it if it dies |
| `errorReports` | boolean | `false` | Send crash and error reports to Sentry. See [Privacy](#privacy) |

**Input gain.** `"*"` sets the target for every input device present, and a named entry overrides
it for that device: `{"*": 100, "Brio 100": 75}` holds everything at 100 percent except the Brio,
which is held at 75. Without a `"*"` entry, a device the config does not name is left alone. The
wildcard covers blocked devices too, so an AirPods Max kept out of the input slot by `blockedInput`
still has its gain held, and devices whose gain cannot be read - some virtual devices - are left
alone either way.

**Output volume.** Some apps move the Mac's volume on their own: Parallels follows Windows' mixer
and nudges it down a step at a time. Each entry in `outputVolumeHoldAgainst` is matched against the
path of the program that wrote the volume: either its executable name, or an app it lives inside
(`"Parallels Desktop"` matches anything under `Parallels Desktop.app`, helpers included). A write
by a listed program is put back to what it was; any other change - the keyboard, Control Center,
the Digital Crown, another app - is yours and becomes the new value to keep. Three things to know:

- Only Bluetooth output is covered. Who wrote the volume is read from coreaudiod's Bluetooth
  driver log (`log stream`), and other outputs log nothing; when that log cannot be read,
  `cleat status` says so and nothing is undone.
- Turning the volume inside Windows is undone too. To change it there, take Parallels off the list
  or turn off Parallels' "Sync volume with Mac".
- Three reverts within a minute are allowed; a fourth pauses the rule for ten minutes and logs why.

**Headphones.** When a Bluetooth output device appears, it becomes the output. Choosing another
device by hand while it stays connected is respected: with `headphonesTakeOver` on, `output` never
moves the sound off a connected Bluetooth device and never moves it onto one, unless that device is
on `blockedOutput` - that list says "never this one", and it outranks "this one is a headset". So
the priority list decides what plays when no headphones hold the output. macOS does this for wired
headphones already and iOS does it for AirPods;
over Bluetooth on a Mac, reconnecting a headset that was last paired to a phone leaves the sound
coming out of the speakers, which is the gap this fills. Headphones already connected when Cleat
starts are not treated as having just arrived, so restarting Cleat never moves the output.

`blockedOutput` is the other half of it. Some USB microphones carry a speaker end, and that is
where macOS lands when the headphones leave. It is not on your priority list, so without the
blocked list Cleat would read it as an output you picked yourself and leave it there. A blocked
device is moved off even when the priority list has nowhere to send the sound - when every device
it names is a headset Cleat may not touch, the sound goes to the first output present that is
neither blocked nor a headset, and only stays put when there is no such device.

**Reclaim.** AirPods paired to both a Mac and a phone belong to whichever one last asked for
them. The phone asks by playing something; when it stops, nothing on the Mac asks again, so the
headset stays with the phone - still connected over Bluetooth, gone from the audio device list -
and the Mac's sound comes out of the speakers for the rest of the day. macOS only reclaims the
headset when an app on this side *starts* playing, and by then the sound has already gone
somewhere else.

Listing a headset under `reclaim` asks for it back. Cleat sends the same routing request macOS
itself sends, and the accessory decides between the two devices by what each one is doing: the
request says this Mac has a playback session, which outranks a phone sitting idle and is outranked
by a phone playing media or on a call. So an idle phone gives the headset up and a phone actually
using it keeps it - the phone comes first, and Cleat only asks for a headset it is not using.
Four things have to be
true before a request goes out - the headset is listed, it is connected to this Mac, it has no
audio device here, and something is playing through whatever holds the output - so an idle Mac
never takes a headset off anybody. A headset the phone keeps is left alone for a minute before
asking again, and said once in the log rather than once a beat.

A headset may be named by its Bluetooth address (`"70:F9:4A:B6:0C:C9"`, dashes and lower case
accepted) as well as by name, which is how two headsets called the same thing are told apart.
`cleat reclaim` sends one request by hand and prints the answer, which is the way to see what the
arbitration is saying. This rule uses a private system interface: on a macOS that does not have
it, the rule turns itself off, says so once in the log, and `cleat status` reports it as
unavailable rather than on.

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
cleat restart     # start the daemon, or replace the running one, through its launchd agent
cleat reclaim     # ask for the headsets under "reclaim", once, and print the answer
cleat version
```

`cleat restart` is the one to reach for when Cleat is not running: it registers the agent if that
has not happened yet, then has launchd replace the process. Nothing else has to be started by
hand.

`cleat status` reads `~/Library/Application Support/Cleat/status.json`; `cleat log` reads
`~/Library/Logs/Cleat/cleat.log`. Only actions that changed something are logged, so a quiet log
means a quiet day, not a broken daemon - `status` is what tells you it is alive.

## Microphone permission

Only `liveness` needs it. macOS asks the first time Cleat runs; if you decline, every other rule
keeps working and `cleat status` shows `microphone: denied`. To change your mind: System Settings
> Privacy & Security > Microphone, then `cleat restart` - Cleat reads the permission at launch and
does not watch that switch.

This is also why Cleat is an .app rather than a bare binary on a LaunchAgent - a command-line tool
started by launchd is often never asked, and the request fails silently instead. The agent that
supervises the daemon starts the app bundle's own binary (`BundleProgram`), so the process launchd
brings back is the same app the microphone was granted to, not a loose executable.

## Privacy

Cleat sends nothing off your Mac unless you ask it to. The one exception is opt-in: set
`"errorReports": true` in the config and the daemon sends crash and error reports to Sentry, so
crashes reach the author without anyone filing an issue. A report carries the stack trace, the
Cleat and macOS versions, the Mac model, language and time zone, and the rough region Sentry
infers from the connection. It never carries your IP address, your config, or the contents of any
file, and your home folder in file paths is replaced with `~`. There is no usage tracking, no
session tracking and no screen recording.

Set it back to `false`, or delete the line, and reporting stops as soon as the config is re-read;
no restart needed. `cleat status` shows which way it is set. The `cleat` commands never send
anything.

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

Or, for a manual install: unload the agent with `launchctl bootout gui/$UID/ai.jetto.cleat`, delete
`/Applications/Cleat.app`, and remove `~/Library/Application Support/Cleat`, `~/Library/Logs/Cleat`
and `~/.config/cleat`.

## License

MIT
