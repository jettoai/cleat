#!/bin/bash
# Build a signed, notarized, stapled Cleat zip (universal: arm64 + x86_64).
#
# Adapted from tally's build-release.sh, minus Sparkle: Cleat updates through Homebrew, so there is
# no appcast, no EdDSA key and no XPC service to strip. Notarization uses an App Store Connect API
# key read from 1Password at build time; no credential is ever written into the repo.
#
# Prereqs (one-time):
#   op signin        # 1Password session for the ASC notary key (op://dev/global-shared/ASC_*)
set -euo pipefail

cd "$(dirname "$0")/.."

TEAM_ID="87Z993GX39"
SIGN_IDENTITY="Developer ID Application: Jetto AI, LLC (${TEAM_ID})"
ASC_NOTARY_ITEM="op://dev/global-shared"

ARCHIVE=build/Cleat.xcarchive
EXPORT=build/export
DIST=dist
rm -rf "$ARCHIVE" "$EXPORT"
mkdir -p build "$DIST"

# Read the notary credentials before the build rather than after it: a vault that locks mid-build
# then cannot break notarization, and a missing key fails the release immediately. The .p8 only
# ever lands in a temp file removed on exit; nothing here echoes a value.
echo "==> preflight: App Store Connect notary key"
NOTARY_KEY_FILE=$(mktemp)
trap 'rm -f "$NOTARY_KEY_FILE"' EXIT
asc_read() {
  op read "$ASC_NOTARY_ITEM/$1" \
    || { echo "1Password not signed in or ASC notary key missing ($1) - run op signin" >&2; exit 1; }
}
asc_read ASC_NOTARY_KEY_P8 > "$NOTARY_KEY_FILE"
NOTARY_KEY_ID=$(asc_read ASC_NOTARY_KEY_ID) || exit 1
NOTARY_ISSUER_ID=$(asc_read ASC_NOTARY_ISSUER_ID) || exit 1

echo "==> xcodegen"
xcodegen generate

echo "==> test"
xcodebuild test -project Cleat.xcodeproj -scheme Cleat -destination 'platform=macOS' -quiet

echo "==> archive (universal, Developer ID)"
xcodebuild archive \
  -project Cleat.xcodeproj -scheme Cleat -configuration Release \
  -archivePath "$ARCHIVE" \
  ARCHS="arm64 x86_64" ONLY_ACTIVE_ARCH=NO \
  CODE_SIGN_IDENTITY="Developer ID Application" CODE_SIGN_STYLE=Manual \
  DEVELOPMENT_TEAM="$TEAM_ID" \
  -quiet

echo "==> export"
xcodebuild -exportArchive -archivePath "$ARCHIVE" \
  -exportOptionsPlist ExportOptions.plist -exportPath "$EXPORT" -quiet
APP="$EXPORT/Cleat.app"

# The cask links Contents/MacOS/Cleat onto the PATH, so the binary itself has to run on both
# architectures - an Intel Mac would otherwise install an app whose `cleat` cannot start.
lipo -archs "$APP/Contents/MacOS/Cleat" | grep -q arm64 \
  && lipo -archs "$APP/Contents/MacOS/Cleat" | grep -q x86_64 \
  || { echo "App binary is not universal" >&2; exit 1; }

echo "==> sign"
codesign --force --options runtime --timestamp \
  --entitlements Cleat/App/Cleat.entitlements --sign "$SIGN_IDENTITY" "$APP"
codesign --verify --strict --deep "$APP"

VERSION=$(/usr/libexec/PlistBuddy -c "Print CFBundleShortVersionString" "$APP/Contents/Info.plist")
ZIP="$DIST/Cleat-$VERSION.zip"

echo "==> zip ($ZIP)"
rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"

echo "==> notarize + staple"
# Credentials were read during preflight; no 1Password access happens after the build starts.
# The staple lands on the .app, so the app is re-zipped afterwards - stapling a zip is not a thing.
xcrun notarytool submit "$ZIP" \
  --key "$NOTARY_KEY_FILE" \
  --key-id "$NOTARY_KEY_ID" \
  --issuer "$NOTARY_ISSUER_ID" \
  --wait
xcrun stapler staple "$APP"
rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"
xcrun stapler validate "$APP"

echo "==> done: $ZIP"
echo "sha256: $(shasum -a 256 "$ZIP" | awk '{print $1}')"
echo
echo "Next: create the GitHub release, then update Casks/cleat.rb in dreamerhyde/homebrew-tap"
echo "with this version and sha256."
