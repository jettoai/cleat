#!/bin/bash
# Installs target/bundle.noindex/Cleat-rs.app (built by bundle.sh) as /Applications/Cleat-rs.app,
# beside the Swift /Applications/Cleat.app and never over it, where Finder, Spotlight and Raycast
# find it. Points the launchd agent ~/Library/LaunchAgents/<label>.plist at the installed copy, so
# the daemon and the settings window run one bundle, and restarts the daemon. A previous install
# is moved aside into target/bundle.noindex (out of Spotlight), never deleted. The plist must
# already exist (README).
set -euo pipefail

RS="$(cd "$(dirname "$0")/.." && pwd)"
LABEL="ai.jetto.cleat.rs"
OUT="$RS/target/bundle.noindex"
SRC="$OUT/Cleat-rs.app"
DEST="/Applications/Cleat-rs.app"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
DOMAIN="gui/$(id -u)"

[ -d "$SRC" ] || { echo "install.sh: no $SRC, run scripts/bundle.sh first" >&2; exit 1; }
[ -f "$PLIST" ] || { echo "install.sh: no $PLIST, see README" >&2; exit 1; }

if [ -e "$DEST" ]; then
    mv "$DEST" "$OUT/installed-old-$(date +%Y%m%d-%H%M%S)-Cleat-rs.app"
fi
ditto "$SRC" "$DEST"
codesign --verify --strict "$DEST"

plutil -remove Program "$PLIST" 2>/dev/null || true
plutil -replace ProgramArguments -json "[\"$DEST/Contents/MacOS/Cleat-rs\"]" "$PLIST"
# A clean quit (the menu's 結束 Cleat) stays down, anything else is restarted: the same KeepAlive as
# bundle/LaunchAgent.plist.in.
plutil -replace KeepAlive -json '{"SuccessfulExit":false}' "$PLIST"
plutil -lint "$PLIST"

# bootout returns before the job is gone; bootstrap on a job still unloading fails with
# "5: Input/output error". Wait (at most 10 s) until launchctl no longer knows the job.
launchctl bootout "$DOMAIN/$LABEL" 2>/dev/null || true
for _ in $(seq 1 20); do
    launchctl print "$DOMAIN/$LABEL" >/dev/null 2>&1 || break
    sleep 0.5
done
if launchctl print "$DOMAIN/$LABEL" >/dev/null 2>&1; then
    echo "install.sh: $LABEL still loaded 10 s after bootout, not bootstrapping" >&2
    exit 1
fi
launchctl bootstrap "$DOMAIN" "$PLIST"
"$DEST/Contents/MacOS/Cleat-rs" status | head -3
