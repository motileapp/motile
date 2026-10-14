#!/usr/bin/env bash
# Has Apple notarize a signed app and staples the ticket to it. Takes APPLE_API_KEY (the .p8's
# contents), APPLE_API_KEY_ID and APPLE_API_ISSUER_ID.
#
#   scripts/notarize.sh build/Motile.app
set -euo pipefail

APP="${1:?Give the app to notarize.}"
WORK="$(mktemp -d)"
echo "$APPLE_API_KEY" > "$WORK/api-key.p8"
CREDENTIALS=(--key "$WORK/api-key.p8" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER_ID")
ditto -c -k --keepParent "$APP" "$WORK/notarize.zip"
ls -lh "$WORK/notarize.zip"
SUBMISSION="$(xcrun notarytool submit "$WORK/notarize.zip" "${CREDENTIALS[@]}" --output-format json | jq -r .id)"
echo "Uploaded as submission $SUBMISSION"
STATUS=0
xcrun notarytool wait "$SUBMISSION" "${CREDENTIALS[@]}" --timeout 30m || STATUS=$?
xcrun notarytool info "$SUBMISSION" "${CREDENTIALS[@]}"
xcrun notarytool log "$SUBMISSION" "${CREDENTIALS[@]}" || true
rm -rf "$WORK"
[ "$STATUS" -eq 0 ] || exit "$STATUS"
xcrun stapler staple "$APP"
spctl --assess --type execute --verbose "$APP"
