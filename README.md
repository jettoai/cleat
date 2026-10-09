<h1 align="center">Cleat</h1>
<p align="center"><sub>by <a href="https://jetto.ai">Jetto</a></sub></p>

<p align="center">Your Mac's audio devices, held where you put them:<br>the right speaker, the right microphone at the right level, and your AirPods back from the phone.</p>

<p align="center"><b>English</b> · <a href="README.zh-TW.md">繁體中文</a> · <a href="README.zh-CN.md">简体中文</a> · <a href="README.ja.md">日本語</a> · <a href="README.ko.md">한국어</a></p>

A cleat is the fitting a rope gets tied to so the boat stops drifting. This one is for audio
devices: you say which output, which microphone, what gain and what balance you want, and Cleat
holds it. It reacts to CoreAudio events instead of polling, so it costs next to no CPU, and it
lives in the menu bar with a settings window for everything it holds.

macOS keeps moving audio devices on you. Connecting AirPods Max takes over the microphone (and
drops Bluetooth into call-quality HFP). Conferencing apps "automatically adjust microphone volume"
and leave the gain somewhere else. A virtual machine resets the output volume when it starts.
Balance drifts off centre after some reconnects. AirPods that a phone borrowed stay with the phone,
and the Mac plays through its speakers for the rest of the day. And a wireless receiver whose
transmitter is switched off is still a perfectly good CoreAudio device that happens to be sending
nothing at all.

**What Cleat does**

- **Output priority list.** Sound plays from the first connected device on your list. While a device
  on your list is connected, devices ticked "Exclude" are moved off, even when macOS lands on
  them by itself.
- **Microphone priority list.** The first connected microphone on your list is the default input;
  while one of them is connected, a blocklist keeps AirPods Max (or Zoom's and Teams' virtual
  devices) out of that slot.
- **Bluetooth headphones take over.** When a headset connects, the sound goes to it, the way it
  already does for wired headphones.
- **AirPods back from the phone.** When a headset you listed is connected but a phone or iPad holds
  its audio, and the Mac is playing while you are at it, Cleat asks for it back. A phone that is
  actually playing or on a call keeps it. A headset that says no because nobody is wearing it yet
  is asked again every 8 seconds, for up to 3 minutes, while the Mac keeps playing.
- **Microphone gain held.** One level for every microphone, with per-device overrides.
- **Output volume held against the programs you name.** If Parallels Desktop (or any app you add)
  changes the output volume, Cleat puts it back. Your own changes stay.
- **Balance held at centre** (or wherever you set it).
- **Silent devices count as gone.** A receiver sending exact digital silence is skipped and the
  next microphone on the list takes over.
- **Your own choices are left alone.** A microphone you picked yourself that is on neither list
  stays picked.
- **A settings window and a menu bar item**, showing what is in use right now and what Cleat itself
  costs in CPU and memory.
- **Error reports only if you ask.** Off by default. Turning them off deletes the reports still
  waiting to go and cuts off one already on its way.

<p align="center">
  <img src="assets/screenshot-output.png" alt="Cleat's settings window, Output page, in dark mode with the English interface: a sidebar with Output, Input, Headphones and General; a status card reading Cleat is running, with CPU, memory and reclaim speed (nothing put back yet); an output priority list with 外接耳機 (the name macOS gives the external headphones on this Mac) first and marked In use; other output devices below it, with Mac Studio的揚聲器 (Mac Studio's speakers) and the Maono AI Microphone marked Excluded; and an output volume and balance card undoing volume changes made by Parallels Desktop and holding the balance, with the note that the current output, 外接耳機, has no balance control, left alone" width="720">
</p>

<p align="center">
  <img src="assets/screenshot-input.png" alt="Cleat's settings window, Input page, in dark mode with the English interface: an input priority list of Wireless microphone (In use) and Brio 100; other input devices with AirPods Max (Offline), Microsoft Teams Audio and ZoomAudioDevice marked Excluded, and the Maono AI Microphone; and an input volume card holding the default level for every microphone at 100 percent, with the reading Now: Wireless microphone 100%" width="720">
</p>

<p align="center">
  <img src="assets/screenshot-headphones.png" alt="Cleat's settings window, Headphones page, in dark mode with the English interface: the switches Switch to Bluetooth headphones when they connect and Ask for headphones back from other devices, both on; AirPods Max and AirPods Pro (Offline) ticked; and a collapsed row, Other Bluetooth devices (3), with a note on when to tick one" width="720">
</p>

<p align="center">
  <img src="assets/screenshot-output-light.png" alt="The same Output page of Cleat's settings window in light mode, English interface" width="720">
</p>

<p align="center"><sub>The settings window and the menu follow the system language: Traditional Chinese on a Traditional Chinese system, English otherwise. The config file and the <code>cleat</code> commands are in English.</sub></p>

## What it holds

Six rules are the same shape: subscribe to a CoreAudio property, compare against the config, write
back if they differ. The output volume rule also has to know which program made a change, which
CoreAudio does not say, so it reads that from the system log. The reclaim rule asks another daemon
for a headset back, because a headset a phone has taken is not a CoreAudio property to write.

| | What | Config |
|---|---|---|
| 1 | Input device priority list, with a blocklist | `input`, `blockedInput` |
| 2 | Treat a device sending exact digital silence as absent | `liveness` |
| 3 | Hold the output balance | `balance` |
| 4 | Output device priority list, with a blocklist | `output`, `blockedOutput` |
| 5 | Hold input gain per device | `inputVolume` |
| 6 | Bluetooth headphones take over the output when they connect | `headphonesTakeOver` |
| 7 | Ask a Bluetooth headset back when a phone has taken it | `reclaim`, `reclaimEnabled` |
| 8 | Undo output volume changes made by the programs you list | `outputVolumeHoldAgainst`, `outputVolumeHoldEnabled` |

It never takes over a device you picked yourself. If the current default input is not on your
priority list and not on the blocklist (a mic you chose in System Settings, Zoom's or Teams'
virtual device), Cleat leaves it alone.

## The window and the menu bar

The daemon puts an item in the menu bar. Its menu shows the output and input in use right now,
opens the settings window, shows About, and quits Cleat.

The settings window has four pages: Output, Input, Headphones and General. Across the top of each
page is a status card: whether Cleat is running, its CPU (one core is 100 percent, averaged over the last
minute, the same figure Activity Monitor shows), its memory (the same as Activity Monitor's Memory
column), and its reclaim speed (how long Cleat takes to put a setting back after something changed
it, the median of the recent ones).

- **Output**: the priority list, every other output device with an "Add to order" button and an
  "Exclude" box, the programs whose volume changes are undone (with the time of the last undo),
  and the balance with a live reading.
- **Input**: the microphone priority list, every other input device, and the gain: one slider for
  every microphone plus a slider for each device you add.
- **Headphones**: the takeover switch, the reclaim switch, and a box for each paired Bluetooth
  headset. Bluetooth devices the system does not identify as audio sit in a collapsed group,
  for speakers or headphones that do not announce themselves.
- **General**: whether Cleat starts at login and restarts when it dies (`launchAtLogin`).

Right-click a device in a priority list to move it up or down. Every change is written to the
config file a moment later, and the daemon picks it up from there; keys the window does not show
are left exactly as they were. If the file changes on disk while the window is open, the window
stops writing and offers to reload.

Open it from the menu bar item, with `cleat settings`, or by opening Cleat.app from Finder,
Spotlight or Raycast while the daemon is running. Only one settings window is open at a time.

## Install

Download the zip from [Releases](https://github.com/jettoai/cleat/releases), unzip it into
`/Applications`, and open Cleat once from Finder. That first launch asks for the microphone and
registers the launchd agent that ships inside the bundle, so from then on Cleat starts at login and
starts again if it is ever killed or crashes. The switch on the settings window's General page
turns that off. Opening the app again while the agent is running opens the settings window, so
there is never a second daemon.

Or install it with Homebrew, which also puts `cleat` on your path:

```sh
brew tap jettoai/tap
# Homebrew requires third-party taps to be trusted before it will load their casks.
brew trust jettoai/tap
brew install --cask cleat
```

Then write a config and start it:

**Read the example before you copy it.** It is the author's own setup, not a neutral starting
point: it holds every input device's gain at 100 percent (`"inputVolume": {"*": 100, ...}`) and it
turns headphone takeover on, so any Bluetooth headset becomes the output as soon as it connects.
The device names in it are the author's, so the rules that name devices do nothing on your machine
until you put your own there. Two settings in it name no device and take effect at once: `balance`
pulls whatever output you are on back to centre, and `launchAtLogin` registers Cleat as a login
item. Edit it for your devices first, or start from `{}` and add one rule at a time; the settings
window can do the editing for you.

```sh
mkdir -p ~/.config/cleat
cp /Applications/Cleat.app/Contents/Resources/config.example.json ~/.config/cleat/config.json
cleat restart
```

`cleat restart` registers the launchd agent that ships inside the bundle and starts the daemon
through it; from then on launchd starts Cleat at login and starts it again if it is ever killed or
crashes. A clean quit (the menu's Quit, or `brew upgrade` replacing the old copy) is left alone on
purpose; Cleat comes back at the next login, when you open the app again, or with `cleat restart`.
`cleat status` says which agent state you are in. Setting `launchAtLogin` to `false` unregisters
the agent, and Cleat exits with it.

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
  "reclaim": ["AirPods Max"],
  "outputVolumeHoldAgainst": ["Parallels Desktop"],
  "launchAtLogin": true,
  "errorReports": false
}
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `input` | array of strings | `[]` | Input priority, most preferred first. Empty turns the rule off |
| `blockedInput` | array of strings | `[]` | Never the default input. See [Not used](#not-used) for where Cleat moves it |
| `output` | array of strings | `[]` | Output priority. Empty turns the rule off |
| `blockedOutput` | array of strings | `[]` | Never the default output. See [Not used](#not-used) for where Cleat moves it |
| `headphonesTakeOver` | boolean | `false` | Bluetooth output devices take the output when they connect |
| `balance` | number or null | `null` | 0.0 (left) to 1.0 (right); 0.5 is centred. `null` turns the rule off |
| `inputVolume` | object | `{}` | Device name, or `"*"` for every input device, to percent, 0-100 |
| `liveness` | object | `{}` | Device name to `{ "zeroSeconds": N }`, N at least 1 |
| `reclaim` | array of strings | `[]` | Bluetooth headsets to ask back when another device holds them, by name or address |
| `reclaimEnabled` | boolean | `true` | Turns reclaim off without emptying `reclaim` |
| `outputVolumeHoldAgainst` | array of strings | `[]` | Programs whose changes to the output volume are undone |
| `outputVolumeHoldEnabled` | boolean | `true` | Turns the volume hold off without emptying its list |
| `launchAtLogin` | boolean | `true` | Register the launchd agent that starts Cleat at login and restarts it if it dies |
| `errorReports` | boolean | `false` | Send crash and error reports to Sentry. See [Privacy](#privacy) |

**Input gain.** `"*"` sets the target for every input device present, and a named entry overrides
it for that device: `{"*": 100, "Brio 100": 75}` holds everything at 100 percent except the Brio,
which is held at 75. Without a `"*"` entry, a device the config does not name is left alone. The
wildcard covers blocked devices too, so an AirPods Max kept out of the input slot by `blockedInput`
still has its gain held, and devices whose gain cannot be read (some virtual devices) are left
alone either way.

**Headphones.** When a Bluetooth output device appears, it becomes the output. Choosing another
device by hand while it stays connected is respected: with `headphonesTakeOver` on, `output` never
moves the sound off a connected Bluetooth device and never moves it onto one, unless that device is
on `blockedOutput`. That list says "never this one", and it outranks "this one is a headset". So
the priority list decides what plays when no headphones hold the output. macOS does this for wired
headphones already and iOS does it for AirPods; over Bluetooth on a Mac, reconnecting a headset
that was last paired to a phone leaves the sound coming out of the speakers, which is the gap this
fills. Headphones already connected when Cleat starts are not treated as having just arrived, so
restarting Cleat never moves the output.

`blockedOutput` is the other half of it. Some USB microphones carry a speaker end, and that is
where macOS lands when the headphones leave. It is not on your priority list, so without the
blocked list Cleat would read it as an output you picked yourself and leave it there.

<a id="not-used"></a>**Not used.** A device on `blockedInput` or `blockedOutput` is never picked
by Cleat, and when macOS makes it the default anyway, Cleat moves off it:

- Input: to the first listed microphone with signal, else to the built-in microphone, else to a
  listed microphone that is silent or still being measured (it gives way once a listed one has
  signal). Apart from the built-in microphone, a microphone that is not on `input` is never
  used as the way out.
- Output: to the first listed output, else to any physical output, built-in first, then by name.
  Virtual, aggregate, Continuity and AirPlay devices, and a headset that `headphonesTakeOver`
  owns, are never used as the way out.

When there is nowhere to go, the device stays, `status.json` names it under `stuck`, and the
settings window says so. A blocked device that is not physical (a meeting app's virtual
microphone, an iPhone over Continuity) is only never picked: if an app switches to it itself,
Cleat leaves it there. If macOS keeps putting a blocked device back, Cleat gives up after three
moves in a minute and tries again once a device is added or removed, the config reloads, or a
microphone's signal changes.

**Reclaim.** AirPods paired to both a Mac and a phone belong to whichever one last asked for
them. The phone asks by playing something; when it stops, nothing on the Mac asks again, so the
headset stays with the phone (still connected over Bluetooth, gone from the audio device list)
and the Mac's sound comes out of the speakers for the rest of the day. macOS only reclaims the
headset when an app on this side *starts* playing, and by then the sound has already gone
somewhere else.

Listing a headset under `reclaim` asks for it back. Cleat sends the same routing request macOS
itself sends, and the accessory decides between the two devices by what each one is doing: the
request says this Mac has a playback session, which outranks a phone sitting idle and is outranked
by a phone playing media or on a call. So an idle phone gives the headset up and a phone actually
using it keeps it. Five things have to be true before a request goes out: the headset is listed,
it is connected to this Mac, it has no audio device here, something is playing through whatever
holds the output, and someone is at the Mac (a key or mouse touched in the last 30 seconds, or
the frontmost app playing a video). A display kept awake by a meeting or `caffeinate` does not
count as someone. Moving the output off the headset by hand is respected for the rest of that
playback. A headset the phone keeps is left alone for a minute; after that Cleat asks again at the next
audio change it reacts to, such as playback starting or a device connecting. The log says it
once rather than once a beat. When a request is accepted, the log says how long the headset
took to become the output, or that it did not come back.

A headset may be named by its Bluetooth address (`"70:F9:4A:B6:0C:C9"`, dashes and lower case
accepted) as well as by name, which is how two headsets called the same thing are told apart.
`cleat reclaim` sends one request by hand and prints the answer, which is the way to see what the
arbitration is saying. This rule uses a private system interface: on a macOS that does not have
it, the rule turns itself off, says so once in the log, and `cleat status` reports it as
unavailable rather than on.

**Output volume.** CoreAudio says that the volume changed, not who changed it, so Cleat reads the
writer from the system's audio log. A change by a program on `outputVolumeHoldAgainst` is put back
to the value before it; a change by you or by any program not on the list becomes the new value to
hold. An entry is an executable's name or an app's name: `Parallels Desktop` matches anything
inside `Parallels Desktop.app`.

**Naming a device.** Use the name shown in System Settings, or its CoreAudio UID if two devices
share a name. Names are compared after Unicode normalisation, because some devices carry a
no-break space in their name that you cannot type (Maono's, for one). Case is not normalised.

A malformed or out-of-range config never replaces a working one: the previous settings stay in
force and `cleat status` says why. Before any config has loaded, every rule is off. Removing the
file also turns every rule off, which is the way to switch Cleat off without quitting it.

**Silence detection** (`liveness`) is the one feature that opens the microphone. Cleat runs a HAL
IOProc on the listed device and checks whether every sample in the buffer is exactly zero: a real
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
cleat settings    # open the settings window
cleat version
```

`cleat restart` is the one to reach for when Cleat is not running: it registers the agent if that
has not happened yet, then has launchd replace the process. Nothing else has to be started by
hand.

`cleat status` reads `~/Library/Application Support/Cleat/status.json`; `cleat log` reads
`~/Library/Logs/Cleat/cleat.log`. The commands never talk to the daemon, they read the files
it writes. Only actions that changed something are logged, so a quiet log means a quiet day, not a
broken daemon; `status` is what tells you it is alive.

## Microphone permission

Only `liveness` needs it. macOS asks the first time Cleat runs; if you decline, every other rule
keeps working and `cleat status` shows `microphone: denied`. To change your mind: System Settings
> Privacy & Security > Microphone, then `cleat restart`. Cleat reads the permission at launch and
does not watch that switch.

This is also why Cleat is an .app rather than a bare binary on a LaunchAgent: a command-line tool
started by launchd is often never asked, and the request fails silently instead. The agent that
supervises the daemon starts the app bundle's own binary (`BundleProgram`), so the process launchd
brings back is the same app the microphone was granted to, not a loose executable.

## Privacy

Cleat sends nothing off your Mac unless you ask it to. The one exception is opt-in: set
`"errorReports": true` in the config and the daemon sends crash and error reports to Sentry, so
crashes reach the author without anyone filing an issue. A report carries the stack trace or the
error message, the Cleat and macOS versions, and the time. It carries no IP address, no user, not
your config and not the contents of any file, and your home folder in file paths is replaced with
`~`. Sentry, like any server, sees the connection the report arrives on. There is no usage
tracking, no session tracking and no screen recording.

Set it back to `false`, or delete the line, and reporting stops as soon as the config is re-read;
no restart needed. Reports still waiting to go are deleted, and one already on its way is cut off. `cleat status` shows which way it is set. The `cleat` commands and the settings
window never send anything.

## Build from source

```sh
cd rs
cargo build --release
cargo test
bash scripts/bundle.sh    # target/bundle.noindex/Cleat.app
```

`scripts/bundle.sh` builds an ad-hoc signed bundle with the development identity
`ai.jetto.cleat.rs`, which never registers a login agent; `rs/README.md` explains how to run it
under launchd for testing.

## Uninstall

```sh
brew uninstall --cask --zap cleat
```

Or, for a manual install: unload the agent with `launchctl bootout gui/$UID/ai.jetto.cleat`, delete
`/Applications/Cleat.app`, and remove `~/Library/Application Support/Cleat`,
`~/Library/Logs/Cleat` and `~/.config/cleat`.

## License

MIT
