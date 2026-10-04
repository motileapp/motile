#!/usr/bin/env bash
# Archives the app for devices and uploads it to TestFlight, with the version in Cargo.toml and
# a build number that counts up by the minute. It is signed with the Apple Distribution
# certificate in the keychain and the given profile, so no machine makes a certificate of its own.
#
#   APPLE_TEAM_ID=… IOS_BUNDLE_ID=… IOS_PROVISIONING_PROFILE=… APPLE_API_KEY=… APPLE_API_KEY_ID=… \
#       APPLE_API_ISSUER_ID=… scripts/testflight.sh
#
# IOS_BUNDLE_ID is the bundle identifier of the app's record in App Store Connect,
# IOS_PROVISIONING_PROFILE its App Store profile in base64. APPLE_API_KEY is an App Store Connect
# API key (the .p8's text) that may upload the app.
set -euo pipefail
cd "$(dirname "$0")/.."

ROOT="$(cd ../.. && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
BUILD="$(date -u +%Y%m%d%H%M)"
KEY="$(mktemp -d)/AuthKey_$APPLE_API_KEY_ID.p8"
echo "$APPLE_API_KEY" > "$KEY"

PROFILE="$(mktemp -d)/Motile.mobileprovision"
echo "$IOS_PROVISIONING_PROFILE" | base64 --decode > "$PROFILE"
PROFILE_NAME="$(security cms -D -i "$PROFILE" | plutil -extract Name raw -o - -)"
PROFILE_UUID="$(security cms -D -i "$PROFILE" | plutil -extract UUID raw -o - -)"
PROFILES="$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles"
mkdir -p "$PROFILES"
cp "$PROFILE" "$PROFILES/$PROFILE_UUID.mobileprovision"

xcodebuild archive -project Motile.xcodeproj -scheme Motile -configuration Release \
    -destination 'generic/platform=iOS' -archivePath build/Motile.xcarchive -derivedDataPath build/derived \
    DEVELOPMENT_TEAM="$APPLE_TEAM_ID" PRODUCT_BUNDLE_IDENTIFIER="$IOS_BUNDLE_ID" \
    MARKETING_VERSION="$VERSION" CURRENT_PROJECT_VERSION="$BUILD" \
    CODE_SIGN_STYLE=Manual CODE_SIGN_IDENTITY="Apple Distribution" PROVISIONING_PROFILE_SPECIFIER="$PROFILE_NAME"

cat > build/export.plist <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>method</key><string>app-store-connect</string>
    <key>destination</key><string>upload</string>
    <key>teamID</key><string>$APPLE_TEAM_ID</string>
    <key>signingStyle</key><string>manual</string>
    <key>signingCertificate</key><string>Apple Distribution</string>
    <key>provisioningProfiles</key><dict><key>$IOS_BUNDLE_ID</key><string>$PROFILE_NAME</string></dict>
</dict>
</plist>
PLIST
xcodebuild -exportArchive -archivePath build/Motile.xcarchive -exportOptionsPlist build/export.plist \
    -exportPath build/export -authenticationKeyPath "$KEY" -authenticationKeyID "$APPLE_API_KEY_ID" \
    -authenticationKeyIssuerID "$APPLE_API_ISSUER_ID"
echo "✓ Uploaded Motile $VERSION ($BUILD) to TestFlight"
