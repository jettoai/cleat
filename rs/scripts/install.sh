#!/bin/bash
# Installs target/Cleat-rs.app (built by bundle.sh) as /Applications/Cleat-rs.app, beside the
# Swift /Applications/Cleat.app and never over it, where Finder, Spotlight and Raycast find it.
# Points the launchd agent ~/Library/LaunchAgents/<label>.plist at the installed copy, so the
# daemon and the settings window run one bundle, and restarts the daemon.
# A previous install is moved aside, never deleted. The plist must already exist (README).
set -euo pipefail

RS="$(cd "$(dirname "$0")/.." && pwd)"
LABEL="ai.jetto.cleat.rs"
SRC="$RS/target/Cleat-rs.app"
DEST="/Applications/Cleat-rs.app"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
DOMAIN="gui/$(id -u)"

[ -d "$SRC" ] || { echo "install.sh: no $SRC, run scripts/bundle.sh first" >&2; exit 1; }
[ -f "$PLIST" ] || { echo "install.sh: no $PLIST, see README" >&2; exit 1; }

if [ -e "$DEST" ]; then
    mv "$DEST" "/Applications/.old-$(date +%Y%m%d-%H%M%S)-Cleat-rs.app"
fi
ditto "$SRC" "$DEST"
codesign --verify --strict "$DEST"

plutil -remove Program "$PLIST" 2>/dev/null || true
plutil -replace ProgramArguments -json "[\"$DEST/Contents/MacOS/Cleat-rs\"]" "$PLIST"
plutil -lint "$PLIST"

launchctl bootout "$DOMAIN/$LABEL" 2>/dev/null || true
launchctl bootstrap "$DOMAIN" "$PLIST"
"$DEST/Contents/MacOS/Cleat-rs" status | head -3
