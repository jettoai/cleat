#!/bin/bash
# Builds rs/target/Cleat-rs.app from the optimised binary, without Xcode: Info.plist and the
# launchd agent plist from rs/bundle/*.in, ad-hoc signed with the hardened runtime and the one
# entitlement the Swift build has (audio input), with bundle/AppIcon.icns as its icon. A previous
# bundle is moved aside, never deleted. CLEAT_LABEL / CLEAT_APP build a copy under another bundle
# id or path (keep the label ending in .rs), so a test copy never replaces the bundle launchd runs.
set -euo pipefail

RS="$(cd "$(dirname "$0")/.." && pwd)"
cd "$RS"
LABEL="${CLEAT_LABEL:-ai.jetto.cleat.rs}"
EXE="Cleat-rs"

cargo build --release

VERSION="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)"
BUILD="$(git rev-list --count HEAD)"
APP="${CLEAT_APP:-$RS/target/$EXE.app}"

if [ -e "$APP" ]; then
    mv "$APP" "$(dirname "$APP")/old-$(date +%Y%m%d-%H%M%S)-$EXE.app"
fi
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APP/Contents/Library/LaunchAgents"

cp "$RS/target/release/cleat-rs" "$APP/Contents/MacOS/$EXE"
cp bundle/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
sed -e "s/@LABEL@/$LABEL/" -e "s/@VERSION@/$VERSION/" -e "s/@BUILD@/$BUILD/" \
    bundle/Info.plist.in > "$APP/Contents/Info.plist"
sed -e "s/@LABEL@/$LABEL/" -e "s|@PROGRAM@|Contents/MacOS/$EXE|" \
    bundle/LaunchAgent.plist.in > "$APP/Contents/Library/LaunchAgents/$LABEL.plist"

codesign --force --sign - --options runtime --entitlements bundle/Cleat-rs.entitlements "$APP"

plutil -lint "$APP/Contents/Info.plist" "$APP/Contents/Library/LaunchAgents/$LABEL.plist"
codesign --verify --strict "$APP"
codesign -d --entitlements - "$APP" 2>/dev/null | grep -q 'com.apple.security.device.audio-input'
REPORTED="$("$APP/Contents/MacOS/$EXE" version)"
if [ "$REPORTED" != "$VERSION" ]; then
    echo "bundle.sh: the binary reports version '$REPORTED', Info.plist says '$VERSION'" >&2
    exit 1
fi
echo "$APP ($VERSION, build $BUILD)"
