#!/usr/bin/env bash
# Lays every program of the corpus (crates/core/fixtures/openui) out on each client at 390 and 720
# points, compares their frames node by node, and lays their pictures side by side, dark, for a
# person to look at: build/programs/sheets/<program>.png.
#
#   scripts/compare-frames.sh                         # every client
#   MOTILE_FRAME_CLIENTS="mac gpui" scripts/compare-frames.sh
#   scripts/compare-frames.sh --bless                 # the Mac's and iOS's frames become the references
#
# The clients are compared as LAYOUT.md pairs them, the Mac with GPUI and iOS with Android, and each
# with the references in crates/core/fixtures/frames, which CI compares every client with. A
# program's line in crates/core/fixtures/frames/known.txt lets a component it names differ until the
# client that strays is fixed. iOS needs a simulator (MOTILE_SIM, "iPhone 17" by default); Android
# needs ANDROID_HOME and a JDK 21 in JAVA_HOME, as its checks do.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$(pwd)"
bless=false
[ "${1:-}" = "--bless" ] && bless=true
clients="${MOTILE_FRAME_CLIENTS:-mac ios gpui android}"

for client in $clients; do
    echo "Laying the corpus out on $client"
    rm -rf "build/frames/$client" "build/programs/$client"
    case "$client" in
        mac)
            cargo build --release -p motile-core
            swift test --package-path packages/apple --filter 'ProgramFramesTests|ProgramPicturesTests'
            ;;
        ios)
            cargo build --release --target aarch64-apple-ios-sim -p motile-core
            (cd packages/apple && xcodebuild test -quiet -scheme MotileKit -destination "platform=iOS Simulator,name=${MOTILE_SIM:-iPhone 17}" \
                -only-testing:MotileKitTests/ProgramFramesTests -only-testing:MotileKitTests/ProgramPicturesTests)
            ;;
        gpui)
            (cd apps/gpui && scripts/patch-crates.sh && cargo test golden_tree)
            ;;
        android)
            (cd apps/android && scripts/build-core.sh && ./gradlew -Pmotile.skipCore testDebugUnitTest --tests 'app.motile.transcript.openui.ProgramFramesTest')
            ;;
    esac
done

if $bless; then
    for client in mac ios; do
        rm -rf "crates/core/fixtures/frames/$client"
        cp -R "build/frames/$client" "crates/core/fixtures/frames/$client"
    done
fi

cargo build -q --release -p motile-core --example compare_frames --example program_sheet
compare() {
    echo "$2 against $1:"
    target/release/examples/compare_frames "$1" "$2" --known crates/core/fixtures/frames/known.txt "${@:3}" || drift=true
}
drift=false
has() { [[ " $clients " == *" $1 "* ]]; }
has mac && compare crates/core/fixtures/frames/mac build/frames/mac
has ios && compare crates/core/fixtures/frames/ios build/frames/ios --mobile
has gpui && compare crates/core/fixtures/frames/mac build/frames/gpui
has android && compare crates/core/fixtures/frames/ios build/frames/android --mobile
has mac && has gpui && compare build/frames/mac build/frames/gpui
has ios && has android && compare build/frames/ios build/frames/android --mobile
target/release/examples/program_sheet build/programs/sheets
echo "$ROOT/build/programs/sheets"
! $drift
