#!/usr/bin/env bash
# Fails when a view styles a button, a menu, a spinner or a field by hand instead of using the
# components in Sources/MotileKit/Shared/Views/UI.
#
#   packages/apple/scripts/check-ui.sh
set -euo pipefail
cd "$(dirname "$0")/../Sources/MotileKit"

PATTERNS='\.buttonStyle\(\.(plain|bordered|borderedProminent|borderless|link)\)|\.menuStyle\(|ProgressView\(\)|\.textFieldStyle\(\.roundedBorder\)'

# Rows and what lies on glass or over a picture have looks of their own.
ALLOWED=(
    Shared/Views/UI/
    Shared/Views/Sidebar/SidebarView.swift
    Mac/ThreadPane.swift
    Mac/MediaViewer.swift
    iOS/ThreadScreen.swift
    iOS/ThreadSettingsSheet.swift
    iOS/SidebarScreen.swift
    iOS/RowSwipe.swift
)

found="$(grep -rnE "$PATTERNS" --include='*.swift' . | sed 's|^\./||' || true)"
for allowed in "${ALLOWED[@]}"; do
    found="$(printf '%s\n' "$found" | grep -v "^$allowed" || true)"
done

if [ -n "$found" ]; then
    echo "These style a control by hand. Use ActionButton, ActionMenu, Spinner or InputField:"
    printf '%s\n' "$found"
    exit 1
fi
echo "✓ Every control comes from Shared/Views/UI"
