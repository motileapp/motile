#!/usr/bin/env bash
# Archives the client for devices and uploads it to TestFlight, with the version in Cargo.toml and
# a build number that counts up by the minute. It is signed with the Apple Distribution
# certificate in the keychain and the given profiles, so no machine makes a certificate of its own.
#
#   APPLE_TEAM_ID=… IOS_BUNDLE_ID=… IOS_PROVISIONING_PROFILE=… IOS_NOTIFICATIONS_PROFILE=… \
#       IOS_WIDGETS_PROFILE=… APPLE_API_KEY=… APPLE_API_KEY_ID=… APPLE_API_ISSUER_ID=… scripts/testflight.sh
#
# IOS_BUNDLE_ID is the bundle identifier of the client's record in App Store Connect; its
# extensions are IOS_BUNDLE_ID.notifications and IOS_BUNDLE_ID.widgets. IOS_PROVISIONING_PROFILE,
# IOS_NOTIFICATIONS_PROFILE and IOS_WIDGETS_PROFILE are their App Store profiles in base64.
# APPLE_API_KEY is an App Store Connect API key (the .p8's text) that may upload the client.
set -euo pipefail
cd "$(dirname "$0")/.."

ROOT="$(cd ../.. && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
BUILD="$(date -u +%Y%m%d%H%M)"
KEY="$(mktemp -d)/AuthKey_$APPLE_API_KEY_ID.p8"
echo "$APPLE_API_KEY" > "$KEY"

PROFILES="$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles"
mkdir -p "$PROFILES"
# Installs a profile given in base64 where Xcode looks for them, and says its name.
install_profile() {
    local profile
    profile="$(mktemp -d)/profile.mobileprovision"
    echo "$1" | base64 --decode > "$profile"
    cp "$profile" "$PROFILES/$(security cms -D -i "$profile" | plutil -extract UUID raw -o - -).mobileprovision"
    security cms -D -i "$profile" | plutil -extract Name raw -o - -
}
APP_PROFILE="$(install_profile "$IOS_PROVISIONING_PROFILE")"
NOTIFICATIONS_PROFILE="$(install_profile "$IOS_NOTIFICATIONS_PROFILE")"
WIDGETS_PROFILE="$(install_profile "$IOS_WIDGETS_PROFILE")"

xcodebuild archive -project Motile.xcodeproj -scheme Motile -configuration Release \
    -destination 'generic/platform=iOS' -archivePath build/Motile.xcarchive -derivedDataPath build/derived \
    DEVELOPMENT_TEAM="$APPLE_TEAM_ID" MOTILE_BUNDLE_ID="$IOS_BUNDLE_ID" \
    MARKETING_VERSION="$VERSION" CURRENT_PROJECT_VERSION="$BUILD" \
    CODE_SIGN_STYLE=Manual CODE_SIGN_IDENTITY="Apple Distribution" IOS_PROFILE_NAME="$APP_PROFILE" \
    IOS_NOTIFICATIONS_PROFILE_NAME="$NOTIFICATIONS_PROFILE" IOS_WIDGETS_PROFILE_NAME="$WIDGETS_PROFILE"

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
    <key>provisioningProfiles</key>
    <dict>
        <key>$IOS_BUNDLE_ID</key><string>$APP_PROFILE</string>
        <key>$IOS_BUNDLE_ID.notifications</key><string>$NOTIFICATIONS_PROFILE</string>
        <key>$IOS_BUNDLE_ID.widgets</key><string>$WIDGETS_PROFILE</string>
    </dict>
</dict>
</plist>
PLIST
xcodebuild -exportArchive -archivePath build/Motile.xcarchive -exportOptionsPlist build/export.plist \
    -exportPath build/export -authenticationKeyPath "$KEY" -authenticationKeyID "$APPLE_API_KEY_ID" \
    -authenticationKeyIssuerID "$APPLE_API_ISSUER_ID"
echo "✓ Uploaded Motile $VERSION ($BUILD) to TestFlight"
