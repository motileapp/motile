#!/usr/bin/env bash
# Paints the canvas cases of crates/core/fixtures/canvas/reference on every client and lays each
# client's pictures beside the reference painter's, for a person to look at:
# build/canvas/sheet-dark.png and build/canvas/sheet-light.png.
#
#   scripts/compare-canvas.sh                    # every client, every case
#   scripts/compare-canvas.sh chart-pie svg-     # only the cases whose names start so
#   MOTILE_CANVAS_CLIENTS="gpui android" scripts/compare-canvas.sh
#
# The Apple tests link the core built in target/release; the Android ones need ANDROID_HOME and a
# JDK 21 in JAVA_HOME, as the Android checks do.
set -euo pipefail
cd "$(dirname "$0")/.."

for client in ${MOTILE_CANVAS_CLIENTS:-apple gpui android}; do
    echo "Painting the cases on $client"
    case "$client" in
        apple)
            cargo build --release -p motile-core
            swift test --package-path packages/apple --filter CanvasPainterTests
            ;;
        gpui)
            (cd apps/gpui && scripts/patch-crates.sh && cargo test paints_scenes_as_the_reference_does)
            ;;
        android)
            (cd apps/android && ./gradlew -Pmotile.skipCore testDebugUnitTest --tests 'app.motile.transcript.canvas.ScenePaintTest')
            ;;
    esac
done

for theme in dark light; do
    cargo run -q -p motile-core --example canvas_sheet -- "$theme" "build/canvas/sheet-$theme.png" "$@"
done
echo "build/canvas/sheet-dark.png"
echo "build/canvas/sheet-light.png"
