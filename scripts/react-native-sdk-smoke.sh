#!/usr/bin/env bash
#
# Installs the example app's APK on an Android emulator, launches it and waits for the app's
# self-check to report.
#
#   scripts/react-native-sdk-smoke.sh [--boot] <apk>
#
# Run it inside the `.#rn-android-emulator` shell, which provides `adb` and the emulator. The APK
# is built from the app scripts/react-native-sdk-example.sh stages, with the entry file
# index.smoke.js (scripts/react-native-sdk-smoke.js). That file logs exactly one line through
# `console.log`, which Android keeps in the device log under the tag `ReactNativeJS`:
#
#   FEDIMINT_RN_SMOKE {"status":"ok","generated":12,"stored":12}
#   FEDIMINT_RN_SMOKE {"status":"fail","error":"<text>"}
#
# A failure is logged as soon as it happens, a success only once the app's root component has
# mounted. An app whose native library is missing or does not match its bindings crashes instead:
# the device log then has `FATAL EXCEPTION` from the Android runtime, or `Fatal signal` from the
# C library. The run passes when the verdict is `ok`, no crash was logged and the app is still
# running five seconds later. Every other outcome, including a missing verdict, fails and prints
# the captured log and the end of the device log.
#
# Without --boot a device must already be online. With --boot the script starts
# scripts/rn-android-emulator.sh headless and without a snapshot, waits until Android has booted
# and stops the emulator when it exits, however it exits.
#
# The environment sets two timeouts in seconds: SMOKE_TIMEOUT (default 180) from the launch to the
# verdict, and BOOT_TIMEOUT (default 600) for the emulator to boot.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP=com.reactnativeexample
ACTIVITY="$APP/.MainActivity"
MARKER=FEDIMINT_RN_SMOKE
CRASH_RE='FATAL EXCEPTION|Fatal signal'
SMOKE_TIMEOUT="${SMOKE_TIMEOUT:-180}"
BOOT_TIMEOUT="${BOOT_TIMEOUT:-600}"
# How long the package manager of a freshly booted device may take to answer, in seconds.
PM_TIMEOUT=120
# How long the app has to keep running after it reported ok, in seconds.
SETTLE=5

TMP=""
LOGCAT_PID=""
EMULATOR_PID=""

die() {
    echo "$*" >&2
    exit 1
}

usage() {
    sed -n '/^#   scripts/p' "${BASH_SOURCE[0]}" | sed 's/^#   /usage: /' >&2
    exit 2
}

# Stops the background process $1, waiting at most 30 seconds for it to end, and collects it.
stop_process() {
    local pid="$1" i
    kill "$pid" 2>/dev/null || true
    for ((i = 0; i < 300; i++)); do
        kill -0 "$pid" 2>/dev/null || break
        sleep 0.1
    done
    kill -9 "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
}

# Stops what the script started and removes its temporary files.
cleanup() {
    [[ -z "$LOGCAT_PID" ]] || stop_process "$LOGCAT_PID"
    [[ -z "$EMULATOR_PID" ]] || stop_process "$EMULATOR_PID"
    [[ -z "$TMP" ]] || rm -rf "$TMP"
}

# Succeeds when the app has a process on the device.
app_running() {
    [[ -n "$(adb shell pidof "$APP" 2>/dev/null | tr -d '\r')" ]]
}

# Succeeds when the captured log holds a line that matches the extended expression $1.
logged() {
    grep -Eq "$1" "$TMP/logcat.txt"
}

# Prints the first line of the captured log that matches the extended expression $1.
first_logged() {
    grep -Em1 "$1" "$TMP/logcat.txt"
}

# Fails the run with the one-line reason $1, then the captured log and the end of the device log.
fail_with_logs() {
    {
        echo "the smoke test failed: $1"
        echo "--- the app's log"
        cat "$TMP/logcat.txt"
        echo "--- the last 200 lines of the device log"
        adb logcat -d -t 200 || true
    } >&2
    exit 1
}

# Starts the emulator script and waits until Android has booted.
boot_emulator() {
    local log="$TMP/emulator.log" i
    RN_ANDROID_EMULATOR_BOOT_TIMEOUT="$BOOT_TIMEOUT" \
        "$ROOT/scripts/rn-android-emulator.sh" -no-window -no-snapshot >"$log" 2>&1 &
    EMULATOR_PID=$!
    for ((i = 0; i < BOOT_TIMEOUT + 30; i++)); do
        if grep -q '^==> Ready' "$log"; then
            return
        fi
        if ! kill -0 "$EMULATOR_PID" 2>/dev/null; then
            cat "$log" >&2
            die "the emulator script exited before Android booted; its output is above"
        fi
        sleep 1
    done
    cat "$log" >&2
    die "Android did not boot within BOOT_TIMEOUT=$BOOT_TIMEOUT seconds; the emulator output is" \
        "above"
}

# Waits until the device's package manager answers: a device accepts `adb` before it can install.
wait_for_package_manager() {
    local i
    for ((i = 0; i < PM_TIMEOUT; i++)); do
        if adb shell pm path android >/dev/null 2>&1; then
            return
        fi
        sleep 1
    done
    die "the package manager of the device did not answer within $PM_TIMEOUT seconds"
}

# Waits for the app's verdict. Leaves the reason it stopped waiting in $outcome: verdict, crash,
# gone or timeout.
wait_for_verdict() {
    local elapsed=0
    outcome=timeout
    while ((elapsed < SMOKE_TIMEOUT)); do
        sleep 1
        elapsed=$((elapsed + 1))
        if logged "$MARKER \\{"; then
            outcome=verdict
            return
        fi
        if logged "$CRASH_RE"; then
            outcome=crash
            return
        fi
        if ((elapsed >= 10)) && ! app_running; then
            outcome=gone
            return
        fi
    done
}

boot=0
if [[ "${1:-}" == --boot ]]; then
    boot=1
    shift
fi
[[ $# -eq 1 && "$1" != -* ]] || usage
apk="$1"

[[ -f "$apk" ]] || die "$apk does not exist; build the example app's APK first"
command -v adb >/dev/null ||
    die "adb is not on PATH; run this inside the .#rn-android-emulator shell"

TMP="$(mktemp -d)"
trap cleanup EXIT
trap 'exit 1' INT TERM

if ((boot)); then
    boot_emulator
elif [[ "$(adb get-state 2>/dev/null || true)" != device ]]; then
    die "no Android device is online; start one with 'just rn-android-emulator' in another" \
        "terminal, or pass --boot"
fi
wait_for_package_manager

adb install -r "$apk" || die "adb install of $apk failed"
adb shell pm clear "$APP" >/dev/null || die "cannot clear the data of $APP"

adb logcat -c
adb logcat -v time 'ReactNativeJS:V' 'AndroidRuntime:E' 'DEBUG:F' 'libc:F' '*:S' \
    >"$TMP/logcat.txt" 2>&1 &
LOGCAT_PID=$!

adb shell am start -W -n "$ACTIVITY" || die "cannot start $ACTIVITY"

wait_for_verdict
case "$outcome" in
    verdict)
        verdict="$(first_logged "$MARKER \\{")"
        if [[ "$verdict" != *'"status":"ok"'* ]]; then
            fail_with_logs "the self-check reported a failure: $verdict"
        fi
        sleep "$SETTLE"
        if logged "$CRASH_RE"; then
            fail_with_logs "the app crashed after reporting ok"
        fi
        if ! app_running; then
            fail_with_logs "the app stopped after reporting ok"
        fi
        echo "$verdict"
        echo "the example app runs with the installed packages"
        ;;
    crash) fail_with_logs "the app crashed without a verdict" ;;
    gone) fail_with_logs "the app is not running and gave no verdict" ;;
    timeout) fail_with_logs "no verdict within SMOKE_TIMEOUT=$SMOKE_TIMEOUT seconds" ;;
esac
