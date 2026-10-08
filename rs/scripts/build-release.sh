#!/bin/bash
# Build a signed, notarized, stapled Cleat zip from the Rust port (universal: arm64 + x86_64).
#
# Same flow as the Swift scripts/build-release.sh, with the xcodebuild steps replaced by two cargo
# builds, lipo and rs/scripts/bundle.sh. Notarization uses an App Store Connect API key read from
# 1Password at build time; no credential is ever written into the repo.
#
# The dSYM stays local (dist/Cleat-<version>.dSYM.zip, for atos): crash events from
# src/report/ips.rs carry no instruction_addr / debug_meta, so Sentry could not use it. The binary
# keeps its symbol table instead, which is what puts function names into .ips reports.
#
# Prereqs (one-time):
#   op signin        # 1Password session for the ASC notary key (op://dev/global-shared/ASC_*)
#   rustup target add aarch64-apple-darwin x86_64-apple-darwin
#
# SKIP_NOTARIZE=1 stops after the Developer ID signature and zip: no 1Password read, no notary
# submission. For local verification only; a release never sets it.
set -euo pipefail

cd "$(dirname "$0")/../.."

TEAM_ID="87Z993GX39"
SIGN_IDENTITY="Developer ID Application: Jetto AI, LLC (${TEAM_ID})"
ASC_NOTARY_ITEM="op://dev/global-shared"
SKIP_NOTARIZE="${SKIP_NOTARIZE:-}"

BUILD=build/rs
DIST=dist
APP="$BUILD/Cleat.app"
TARGETS="aarch64-apple-darwin x86_64-apple-darwin"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/rs/target}"
rm -rf build/rs
mkdir -p "$BUILD" "$DIST"

if [ -z "$SKIP_NOTARIZE" ]; then
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
else
  echo "==> SKIP_NOTARIZE set: no notary key read, no notarization"
fi

echo "==> test"
cargo test --manifest-path rs/Cargo.toml

echo "==> build (dist profile, both architectures)"
for t in $TARGETS; do
  MACOSX_DEPLOYMENT_TARGET=14.0 cargo build --manifest-path rs/Cargo.toml --profile dist --target "$t"
  cp "$CARGO_TARGET_DIR/$t/dist/cleat-rs" "$BUILD/Cleat-$t"
done

# dsymutil first (it needs the debug map), then strip -S drops the debug info but keeps the symbol
# table, then lipo. The dSYM UUIDs match the stripped binaries: strip does not change LC_UUID.
echo "==> dSYM"
for t in $TARGETS; do
  dsymutil "$BUILD/Cleat-$t" -o "$BUILD/dSYM/Cleat-$t.dSYM"
  strip -S "$BUILD/Cleat-$t"
done
lipo -create -output "$BUILD/Cleat" "$BUILD"/Cleat-{aarch64,x86_64}-apple-darwin

echo "==> bundle"
CLEAT_LABEL=ai.jetto.cleat CLEAT_APP="$PWD/$APP" CLEAT_BINARY="$PWD/$BUILD/Cleat" bash rs/scripts/bundle.sh

# The cask links Contents/MacOS/Cleat onto the PATH, so the binary itself has to run on both
# architectures - an Intel Mac would otherwise install an app whose `cleat` cannot start.
lipo -archs "$APP/Contents/MacOS/Cleat" | grep -q arm64 \
  && lipo -archs "$APP/Contents/MacOS/Cleat" | grep -q x86_64 \
  || { echo "App binary is not universal" >&2; exit 1; }

echo "==> sign"
# bundle.sh signed ad hoc; --force replaces that with the Developer ID signature.
codesign --force --options runtime --timestamp \
  --entitlements rs/bundle/Cleat-rs.entitlements --sign "$SIGN_IDENTITY" "$APP"
codesign --verify --strict --deep "$APP"

VERSION=$(/usr/libexec/PlistBuddy -c "Print CFBundleShortVersionString" "$APP/Contents/Info.plist")
ZIP="$DIST/Cleat-$VERSION.zip"
DSYM_ZIP="$DIST/Cleat-$VERSION.dSYM.zip"

rm -f "$DSYM_ZIP"
ditto -c -k "$BUILD/dSYM" "$DSYM_ZIP"

echo "==> zip ($ZIP)"
rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"

if [ -n "$SKIP_NOTARIZE" ]; then
  echo "==> done (NOT notarized): $ZIP"
  echo "dSYM: $DSYM_ZIP"
  exit 0
fi

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
echo "dSYM (local only, for atos): $DSYM_ZIP"
echo "sha256: $(shasum -a 256 "$ZIP" | awk '{print $1}')"
echo
echo "Next: create the GitHub release, then update Casks/cleat.rb in jettoai/homebrew-tap"
echo "with this version and sha256."
