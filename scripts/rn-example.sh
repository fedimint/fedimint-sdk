#!/usr/bin/env bash
#
# Builds one React Native example app, installs it on the running emulator (or the connected
# device) and launches it against a Metro dev server. Run inside the `.#android-emulator` shell
# (`just rn-example [react-native|expo-app]`), the shell that owns the device's `adb`.
#
#   scripts/rn-example.sh [react-native|expo-app]
#
# This is what `react-native run-android` does, spelled out: that command has been seen to sit
# silent for good on some desktops (it opens a terminal window for Metro and waits on it), and
# Gradle's output is hidden behind its spinner. Here Gradle prints its tasks, Metro is started
# in the background only when nothing serves port 8081 yet, and the app is launched with adb.
#
# Gradle is pointed at the SDK's own aapt2: the one it downloads from Maven cannot run on NixOS
# hosts. Needs `just build-rn-android` first, for the bindings and the packages' built output.
set -euo pipefail

APP="${1:-react-native}"
case "$APP" in
  react-native) PACKAGE=com.reactnativeexample ;;
  expo-app) PACKAGE=org.fedimint.expoapp ;;
  *) echo "unknown example '$APP'; expected react-native or expo-app" >&2; exit 1 ;;
esac

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="$ROOT/js/examples/$APP"

for tool in adb pnpm java; do
  command -v "$tool" >/dev/null ||
    { echo "$tool not on PATH; run in the .#android-emulator shell" >&2; exit 1; }
done
if [[ "$(adb get-state 2>/dev/null || true)" != "device" ]]; then
  echo "no device is online for adb; start one with 'just android-emulator' and wait for Ready" >&2
  exit 1
fi
if [[ ! -d "$DIR/android" ]]; then
  echo "$DIR/android does not exist; for the Expo app run 'pnpm --dir $DIR exec expo prebuild'" \
    "once to generate it" >&2
  exit 1
fi

SERIAL="$(adb get-serialno)"
echo "==> Target: $SERIAL (Android $(adb shell getprop ro.build.version.release | tr -d '\r'))," \
  "app $PACKAGE from $DIR"

step() { echo "==> [$(date +%H:%M:%S)] $*"; }
metro_up() { curl -sf http://localhost:8081/status >/dev/null 2>&1; }

if metro_up; then
  step "Metro is already serving on port 8081"
else
  step "Starting Metro in the background (log: $DIR/metro.log)"
  (cd "$DIR" && setsid pnpm start > metro.log 2>&1 < /dev/null &)
  for i in $(seq 1 60); do
    metro_up && break
    (( i % 10 == 0 )) && echo "    still waiting for Metro (${i}s)"
    sleep 1
  done
  metro_up || { echo "Metro did not come up within a minute; see $DIR/metro.log" >&2; exit 1; }
fi
adb reverse tcp:8081 tcp:8081 >/dev/null

step "Building and installing $APP (Gradle prints each task; the first run downloads a lot)"
cd "$DIR/android"
env "ORG_GRADLE_PROJECT_android.aapt2FromMavenOverride=$ANDROID_HOME/build-tools/36.0.0/aapt2" \
  ./gradlew app:installDebug --console=plain

step "Launching $PACKAGE"
adb shell monkey -p "$PACKAGE" -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1
step "Done. Metro serves the JavaScript and reloads it on edits; its log is $DIR/metro.log."
