#!/usr/bin/env bash
# Archives the app for devices and uploads it to TestFlight, with the version in Cargo.toml and
# a build number that counts up by the minute. Xcode signs it with the team's own certificate
# and profile, and makes them when they are missing.
#
#   APPLE_TEAM_ID=… IOS_BUNDLE_ID=… APPLE_API_KEY=… APPLE_API_KEY_ID=… APPLE_API_ISSUER_ID=… scripts/testflight.sh
#
# IOS_BUNDLE_ID is the bundle identifier of the app's record in App Store Connect. APPLE_API_KEY
# is an App Store Connect API key (the .p8's text) that may manage the app.
set -euo pipefail
cd "$(dirname "$0")/.."

ROOT="$(cd ../.. && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
BUILD="$(date -u +%Y%m%d%H%M)"
KEY="$(mktemp -d)/AuthKey_$APPLE_API_KEY_ID.p8"
echo "$APPLE_API_KEY" > "$KEY"
CREDENTIALS=(-allowProvisioningUpdates -authenticationKeyPath "$KEY" -authenticationKeyID "$APPLE_API_KEY_ID"
    -authenticationKeyIssuerID "$APPLE_API_ISSUER_ID")

xcodebuild archive -project Motile.xcodeproj -scheme Motile -configuration Release \
    -destination 'generic/platform=iOS' -archivePath build/Motile.xcarchive -derivedDataPath build/derived \
    DEVELOPMENT_TEAM="$APPLE_TEAM_ID" PRODUCT_BUNDLE_IDENTIFIER="$IOS_BUNDLE_ID" \
    MARKETING_VERSION="$VERSION" CURRENT_PROJECT_VERSION="$BUILD" \
    "${CREDENTIALS[@]}"

cat > build/export.plist <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>method</key><string>app-store-connect</string>
    <key>destination</key><string>upload</string>
    <key>teamID</key><string>$APPLE_TEAM_ID</string>
</dict>
</plist>
PLIST
xcodebuild -exportArchive -archivePath build/Motile.xcarchive -exportOptionsPlist build/export.plist \
    -exportPath build/export "${CREDENTIALS[@]}"
echo "✓ Uploaded Motile $VERSION ($BUILD) to TestFlight"
