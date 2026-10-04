#!/usr/bin/env bash
# Builds "Motile GPUI.app" into apps/gpui/build. Needs Rust, and Xcode 26 or later for the icon.
#
#   scripts/build-app.sh [--open]
#
# MOTILE_AUTH_URL, if set, becomes the auth server the app signs in with (default https://auth.motile.app).
# MOTILE_SIGN_IDENTITY, if set, is the Developer ID certificate the app is signed with.
set -euo pipefail

cd "$(dirname "$0")/.."
APP="build/Motile GPUI.app"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"

echo "▸ Building the app…"
cargo build --release

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/motile-gpui "$APP/Contents/MacOS/Motile"

# The Mac app's Icon Composer icon becomes Assets.car for macOS 26, and AppIcon.icns for the ones before.
xcrun actool ../macos/Resources/AppIcon.icon --compile "$APP/Contents/Resources" \
    --platform macosx --target-device mac --minimum-deployment-target 14.0 \
    --app-icon AppIcon --output-partial-info-plist "$(mktemp)" >/dev/null
test -f "$APP/Contents/Resources/Assets.car" && test -f "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Motile</string>
    <key>CFBundleDisplayName</key><string>Motile</string>
    <key>CFBundleExecutable</key><string>Motile</string>
    <key>CFBundleIdentifier</key><string>app.motile.gpui</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundleIconName</key><string>AppIcon</string>
    <key>LSMinimumSystemVersion</key><string>14.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
    <key>NSPrincipalClass</key><string>NSApplication</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSSupportsAutomaticTermination</key><false/>
    <key>NSLocalNetworkUsageDescription</key><string>Motile connects straight to your server when it is on the same network.</string>
    <key>LSEnvironment</key>
    <dict>
        <key>MOTILE_AUTH_URL</key><string>${MOTILE_AUTH_URL:-https://auth.motile.app}</string>
    </dict>
</dict>
</plist>
PLIST

if [ -n "${MOTILE_SIGN_IDENTITY:-}" ]; then
    codesign --force --options runtime --timestamp --sign "$MOTILE_SIGN_IDENTITY" "$APP"
else
    codesign --force --deep --sign - "$APP" >/dev/null
fi
echo "✓ Built apps/gpui/$APP"
if [ "${1:-}" = "--open" ]; then open "$APP"; fi
