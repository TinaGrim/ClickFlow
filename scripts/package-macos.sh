#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VERSION="$(awk -F'"' '/^version = / { print $2; exit }' Cargo.toml)"
ARCH="$(uname -m)"
APP_NAME="ClickFlow"
BUNDLE_ID="com.clickflow.autoclicker"
DIST="$ROOT/dist"
APP="$DIST/$APP_NAME.app"
STAGING="$DIST/dmg-staging"
DMG="$DIST/$APP_NAME-$VERSION-macos-$ARCH.dmg"

echo "Building $APP_NAME $VERSION for macOS ($ARCH)…"
cargo build --release

rm -rf "$APP" "$STAGING" "$DMG"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp target/release/auto-clicker "$APP/Contents/MacOS/$APP_NAME"
cp assets/icon/ClickFlow.icns "$APP/Contents/Resources/ClickFlow.icns"
cp assets/fonts/OFL.txt "$APP/Contents/Resources/OFL.txt"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleDisplayName</key>
    <string>$APP_NAME</string>
    <key>CFBundleExecutable</key>
    <string>$APP_NAME</string>
    <key>CFBundleIconFile</key>
    <string>ClickFlow</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.utilities</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSPrincipalClass</key>
    <string>NSApplication</string>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
</dict>
</plist>
PLIST

xattr -cr "$APP"
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict "$APP"

mkdir -p "$STAGING"
cp -R "$APP" "$STAGING/$APP_NAME.app"
cp packaging/README-macOS.txt "$STAGING/Read Me.txt"
ln -s /Applications "$STAGING/Applications"

echo "Creating $DMG…"
hdiutil create \
    -volname "$APP_NAME $VERSION" \
    -srcfolder "$STAGING" \
    -ov \
    -format UDZO \
    "$DMG" >/dev/null

hdiutil verify "$DMG" >/dev/null
rm -rf "$STAGING"

echo "Done: $DMG"
