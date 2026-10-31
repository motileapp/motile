# The Android emulator the dev app and the tests run in, sourced by dev-app.sh, shot.sh, do.sh and
# ui-test.sh. MOTILE_AVD names it ("motile" without it); it is made with a Pixel's screen the first
# time, and found among the running emulators by its name, so other trees' emulators are left alone.

export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
export JAVA_HOME="${JAVA_HOME:-/opt/homebrew/opt/openjdk@21}"
ADB="$ANDROID_HOME/platform-tools/adb"
AVD="${MOTILE_AVD:-motile}"
IMAGE="system-images;android-36;google_apis;$( [ "$(uname -m)" = arm64 ] && echo arm64-v8a || echo x86_64 )"
PACKAGE=app.motile.motile
# The file of steps the dev app reads, in its own folder; scripts/do.sh adds to it.
STEPS=/data/data/$PACKAGE/files/demo.steps

# The serial of the emulator running $AVD, or nothing.
serial() {
    local device
    for device in $("$ADB" devices | awk '/^emulator-/ {print $1}'); do
        [ "$("$ADB" -s "$device" emu avd name 2>/dev/null | head -1 | tr -d '\r')" = "$AVD" ] && { echo "$device"; return; }
    done
}

boot_emulator() {
    if [ ! -d "$HOME/.android/avd/${AVD}.avd" ]; then
        echo "▸ Making the emulator ${AVD}…"
        "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" "$IMAGE" >/dev/null
        echo no | "$ANDROID_HOME/cmdline-tools/latest/bin/avdmanager" create avd -n "$AVD" -k "$IMAGE" -d pixel_8 >/dev/null
        sed -i '' 's/^hw.ramSize=.*/hw.ramSize=2048/; s/^hw.keyboard=.*/hw.keyboard=yes/' "$HOME/.android/avd/${AVD}.avd/config.ini"
    fi
    SERIAL="$(serial)"
    if [ -z "$SERIAL" ]; then
        echo "▸ Starting the emulator ${AVD}…"
        nohup "$ANDROID_HOME/emulator/emulator" -avd "$AVD" -no-window -no-audio -no-boot-anim -gpu swiftshader_indirect \
            -no-snapshot-save > "${DEV:-/tmp}/emulator.log" 2>&1 &
        for _ in $(seq 1 120); do
            SERIAL="$(serial)"
            [ -n "$SERIAL" ] && break
            sleep 1
        done
    fi
    [ -n "$SERIAL" ] || { echo "The emulator $AVD didn't start" >&2; exit 1; }
    until [ "$("$ADB" -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ]; do sleep 1; done
    export SERIAL
}

stop_emulator() {
    SERIAL="$(serial)"
    [ -z "$SERIAL" ] || "$ADB" -s "$SERIAL" emu kill >/dev/null 2>&1 || true
    # It shuts down on its own in a few seconds; one that doesn't is stopped.
    for _ in $(seq 1 20); do
        pgrep -f "emulator.*-avd $AVD( |$)" >/dev/null || return 0
        sleep 1
    done
    pkill -f "emulator.*-avd $AVD( |$)" || true
}
