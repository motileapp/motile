#!/usr/bin/env bash
# Builds the client for release as build/release/Motile-android.apk, for phones and for x86_64
# devices, with the version in Cargo.toml: in CI, the release's, which scripts/release-version.sh
# writes there first. Its third number, the commit count, is the version code, so that every
# release replaces the one before. It is signed with the key in ANDROID_KEYSTORE_BASE64 (a keystore
# in base64), which every release must share for the client to update itself; without one it is
# signed with the debug key.
#
#   ANDROID_KEYSTORE_BASE64=… ANDROID_KEYSTORE_PASSWORD=… ANDROID_KEY_ALIAS=… scripts/release.sh
set -euo pipefail
cd "$(dirname "$0")/.."
export JAVA_HOME="${JAVA_HOME:-/opt/homebrew/opt/openjdk@21}"

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' ../../Cargo.toml | head -1)"
export MOTILE_VERSION_CODE="${VERSION##*.}"
if [ -n "${ANDROID_KEYSTORE_BASE64:-}" ]; then
    export ANDROID_KEYSTORE="$(mktemp -d)/release.jks"
    echo "$ANDROID_KEYSTORE_BASE64" | base64 --decode > "$ANDROID_KEYSTORE"
else
    echo "No ANDROID_KEYSTORE_BASE64: the release is signed with the debug key." >&2
fi

MOTILE_ABIS=arm64-v8a,x86_64 scripts/build-core.sh
./gradlew -Pmotile.skipCore assembleRelease
mkdir -p build/release
cp app/build/outputs/apk/release/app-release.apk build/release/Motile-android.apk
echo "✓ Built Motile $VERSION ($MOTILE_VERSION_CODE) in build/release"
