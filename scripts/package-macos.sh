#!/usr/bin/env bash
# Builds Crate.app as a universal binary (Apple Silicon + Intel) and the .dmg
# into target/dist/: Crate-<version>-universal.dmg and SHA256SUMS. The release
# workflow runs this on macos-latest; on a Mac:
#
#   rustup target add aarch64-apple-darwin x86_64-apple-darwin   # once
#   scripts/package-macos.sh
#   SKIP_BUILD=1 scripts/package-macos.sh                        # reuse the builds
#
# Signing: with MACOS_SIGN_IDENTITY set ("Developer ID Application: …", in
# the keychain) the app and the .dmg are signed with the hardened runtime;
# without it they're signed ad hoc, which runs on this Mac but makes
# Gatekeeper warn on others.
#
# Notarization: with APPLE_API_KEY_PATH (the App Store Connect key's .p8),
# APPLE_API_KEY_ID and APPLE_API_ISSUER_ID set too, the signed .dmg goes to
# Apple's notary service and the ticket is stapled to it, so Gatekeeper
# accepts it offline as well.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
DIST=target/dist
STAGE=target/packaging/macos
APP="$STAGE/Crate.app"
DMG="$DIST/Crate-$VERSION-universal.dmg"
IDENTITY=${MACOS_SIGN_IDENTITY:--}
# The oldest macOS Crate supports (Info.plist's LSMinimumSystemVersion).
export MACOSX_DEPLOYMENT_TARGET=12.0

if [[ -z "${SKIP_BUILD:-}" ]]; then
  for target in "${TARGETS[@]}"; do
    cargo build --release --locked --target "$target" -p crate-app -p crate-cli
  done
fi

rm -rf "$STAGE" "$DIST"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$DIST"

# One binary with both architectures for each program. The CLI sits next to
# the app's; the Homebrew cask links it onto PATH.
for bin in crate-app crate; do
  lipo -create -output "$APP/Contents/MacOS/$bin" \
    "target/${TARGETS[0]}/release/$bin" "target/${TARGETS[1]}/release/$bin"
  lipo "$APP/Contents/MacOS/$bin" -verify_arch arm64 x86_64
done

sed "s/@VERSION@/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist"
printf 'APPL????' > "$APP/Contents/PkgInfo"
cp assets/icons/crate.icns "$APP/Contents/Resources/crate.icns"
cp LICENSE "$APP/Contents/Resources/LICENSE"
# The Arabic for Info.plist's prompts (Crate's own strings are compiled in).
cp -R packaging/macos/ar.lproj "$APP/Contents/Resources/"

# Inside out: the CLI first, then the bundle (which seals the app binary).
sign=(codesign --force --timestamp --options runtime --sign "$IDENTITY")
[[ "$IDENTITY" == "-" ]] && sign=(codesign --force --sign -)
"${sign[@]}" "$APP/Contents/MacOS/crate"
"${sign[@]}" "$APP"
codesign --verify --strict --verbose=2 "$APP"

# The disk image: the app and a link to /Applications to drag it onto.
DMG_ROOT="$STAGE/dmg"
mkdir -p "$DMG_ROOT"
cp -R "$APP" "$DMG_ROOT/"
ln -s /Applications "$DMG_ROOT/Applications"
hdiutil create -quiet -volname "Crate $VERSION" -srcfolder "$DMG_ROOT" \
  -fs HFS+ -format UDZO -ov "$DMG"
if [[ "$IDENTITY" != "-" ]]; then
  codesign --force --timestamp --sign "$IDENTITY" "$DMG"
  if [[ -n "${APPLE_API_KEY_PATH:-}" ]]; then
    notary=(--key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER_ID")
    # notarytool exits 0 on a rejection too: read the status, and print
    # Apple's log when it isn't "Accepted".
    result=$(xcrun notarytool submit "$DMG" --wait --timeout 30m --output-format json "${notary[@]}")
    echo "$result"
    id=$(plutil -extract id raw -o - - <<< "$result")
    status=$(plutil -extract status raw -o - - <<< "$result")
    if [[ "$status" != "Accepted" ]]; then
      xcrun notarytool log "$id" "${notary[@]}"
      exit 1
    fi
    xcrun stapler staple "$DMG"
    xcrun stapler validate "$DMG"
  fi
fi

(cd "$DIST" && shasum -a 256 ./*.dmg | sed 's| \./| |' > SHA256SUMS && cat SHA256SUMS)
