#!/usr/bin/env bash
# On a CI runner, puts the Developer ID certificate in a keychain of its own for codesign, and has
# the next steps sign with it. Takes APPLE_CERTIFICATE (a .p12 in base64) and APPLE_CERTIFICATE_PASSWORD.
set -euo pipefail

KEYCHAIN="$RUNNER_TEMP/signing.keychain-db"
KEYCHAIN_PASSWORD="$(uuidgen)"
security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
security set-keychain-settings "$KEYCHAIN"
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
echo "$APPLE_CERTIFICATE" | base64 --decode > "$RUNNER_TEMP/certificate.p12"
security import "$RUNNER_TEMP/certificate.p12" -k "$KEYCHAIN" -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple: -s -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN" >/dev/null
security list-keychains -d user -s "$KEYCHAIN" login.keychain-db
echo "MOTILE_SIGN_IDENTITY=Developer ID Application" >> "$GITHUB_ENV"
