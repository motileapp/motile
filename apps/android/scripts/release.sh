#!/usr/bin/env bash
# Builds the client for release, for phones and for x86_64 devices, as an app bundle for Google
# Play and an APK, into build/release, with the version in Cargo.toml and a version code that counts
# up by the minute. It is signed with the upload key in ANDROID_KEYSTORE_BASE64 (a keystore in
# base64), or, without one, left signed with the debug key, which no store takes.
#
#   ANDROID_KEYSTORE_BASE64=… ANDROID_KEYSTORE_PASSWORD=… ANDROID_KEY_ALIAS=… scripts/release.sh
set -euo pipefail
cd "$(dirname "$0")/.."

export MOTILE_VERSION_CODE="$(( ($(date -u +%s) - 1735689600) / 60 ))"
if [ -n "${ANDROID_KEYSTORE_BASE64:-}" ]; then
    export ANDROID_KEYSTORE="$(mktemp -d)/upload.jks"
    echo "$ANDROID_KEYSTORE_BASE64" | base64 --decode > "$ANDROID_KEYSTORE"
else
    echo "No ANDROID_KEYSTORE_BASE64: the release is signed with the debug key." >&2
fi

MOTILE_ABIS=arm64-v8a,x86_64 scripts/build-core.sh
./gradlew -Pmotile.skipCore bundleRelease assembleRelease
mkdir -p build/release
cp app/build/outputs/bundle/release/app-release.aab build/release/Motile.aab
cp app/build/outputs/apk/release/app-release.apk build/release/Motile.apk
echo "✓ Built Motile $(sed -n 's/^version = "\(.*\)"/\1/p' ../../Cargo.toml | head -1) ($MOTILE_VERSION_CODE) in build/release"
