#!/usr/bin/env bash
#
# Builds js/examples/react-native's debug APK, picks or boots an Android
# device, installs the APK, and runs the Appium test runner against it.

set -euo pipefail

REPO_ROOT=$(git rev-parse --show-toplevel)
EXAMPLE_DIR="$REPO_ROOT/js/examples/react-native"
PKG_DIR="$REPO_ROOT/js/react-native/integration-tests-android"

required_bins=(adb emulator)
[[ "${SKIP_BINDINGS_BUILD:-}" == "true" ]] || required_bins+=(just)
for bin in "${required_bins[@]}"; do
  if ! command -v "$bin" >/dev/null 2>&1; then
    echo "$bin not found on PATH. Run this inside 'nix develop .#android-tests'."
    exit 1
  fi
done

echo "=== Android E2E (SDK) tests ==="

cd "$REPO_ROOT"
if [[ "${SKIP_BINDINGS_BUILD:-}" == "true" ]]; then
  echo "SKIP_BINDINGS_BUILD=true — assuming bindings + node_modules are already in place."
else
  # Builds the native FFI library (fedimint-client-uniffi cross-compiled for
  # Android) and the generated Kotlin/JS glue js/examples/react-native depends on.
  # Without this, `pnpm install`'s postinstall may leave a stale prebuilt
  # binary in place (or download a previously-published release) instead of
  # reflecting the current source tree — see js/react-native/react-native-bindings's
  # postinstall (scripts/download-binaries.js) and Justfile's build-android
  # recipe. This also runs `pnpm --dir js install --frozen-lockfile` for the whole
  # workspace, so it must happen before anything below that needs
  # node_modules (e.g. the `appium` binary).
  just build-android
fi

if ! command -v appium >/dev/null 2>&1; then
  echo "appium not found on PATH. Make sure bindings/deps were built (see SKIP_BINDINGS_BUILD) and js/react-native/integration-tests-android/node_modules/.bin exists."
  exit 1
fi

LOG_DIR="${APPIUM_HOME:-$PKG_DIR/.appium}"
mkdir -p "$LOG_DIR"

bash "$REPO_ROOT/scripts/e2e-android/setup-and-start-appium.sh"

AVD_NAME="fedimint-e2e"

ensure_avd_exists() {
  if emulator -list-avds | grep -qx "$AVD_NAME"; then return; fi
  local abi
  abi=$([[ "$(uname -m)" == arm64 || "$(uname -m)" == aarch64 ]] && echo arm64-v8a || echo x86_64)
  local system_image="system-images;android-34;google_apis;$abi"
  echo "No '$AVD_NAME' AVD found — creating one ($system_image)..."
  echo "no" | avdmanager create avd -n "$AVD_NAME" -k "$system_image" --device "pixel_6" --force
}

boot_avd() {
  ensure_avd_exists
  echo "Booting AVD: $AVD_NAME"
  nohup emulator -avd "$AVD_NAME" -no-snapshot -no-boot-anim -no-window -gpu swiftshader_indirect \
    >"$LOG_DIR/emulator.log" 2>&1 &
  adb wait-for-device
  # Wait for full boot, not just the adb transport.
  until [[ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" == "1" ]]; do
    sleep 2
  done
}

get_device_ids() {
  adb devices | awk 'NR>1 && $2=="device" {print $1}'
}

if [[ "${CI:-}" == "true" ]]; then
  # Non-interactive: reuse a running device if one exists, else boot the AVD.
  device_ids=()
  while IFS= read -r line; do device_ids+=("$line"); done < <(get_device_ids)
  if [[ ${#device_ids[@]} -eq 0 ]]; then
    boot_avd
    while IFS= read -r line; do device_ids+=("$line"); done < <(get_device_ids)
  fi
  DEVICE_ID=${device_ids[0]}
  TESTS_TO_RUN=${TESTS_TO_RUN:-all}
else
  while true; do
    device_ids=()
    while IFS= read -r line; do
      device_ids+=("$line")
    done < <(get_device_ids)

    if [[ ${#device_ids[@]} -eq 0 ]]; then
      echo "No Android devices/emulators running."
    fi

    echo -e "\nChoose an option for running the Android E2E tests:"
    echo -e "\e[1;33m⚠️  APP DATA WILL BE WIPED FROM THE SELECTED DEVICE!\e[0m"
    echo "1) Boot the '$AVD_NAME' AVD, creating it if needed (default)"
    echo "2) Refresh device list"
    for i in "${!device_ids[@]}"; do
      echo "$((i + 3))) Select device: ${device_ids[$i]}"
    done

    read -rp "Enter choice: " choice
    choice=${choice:-1}

    if [[ "$choice" == "1" ]]; then
      boot_avd
      continue
    elif [[ "$choice" == "2" ]]; then
      continue
    elif [[ "$choice" =~ ^[0-9]+$ ]] && [ "$choice" -ge 3 ] && [ "$choice" -le "$((${#device_ids[@]} + 2))" ]; then
      DEVICE_ID=${device_ids[$((choice - 3))]}
      break
    else
      echo "Invalid choice, using first device"
      DEVICE_ID=${device_ids[0]}
      break
    fi
  done

  if [[ -z "${TESTS_TO_RUN:-}" ]]; then
    echo "Which tests to run? (mnemonic, all)"
    read -r TESTS_TO_RUN
    TESTS_TO_RUN=${TESTS_TO_RUN:-all}
  fi
fi

echo "Building APK..."
pushd "$EXAMPLE_DIR/android" >/dev/null
./gradlew assembleDebug
APK_PATH=$(find "$PWD/app/build/outputs/apk/debug" -name "*.apk" | head -1)
popd >/dev/null

if [[ ! -f "$APK_PATH" ]]; then
  echo "APK not found after build!"
  exit 1
fi

APP_ID=$(grep applicationId "$EXAMPLE_DIR/android/app/build.gradle" | head -1 | awk -F '"' '{print $2}')
if [[ -z "$APP_ID" ]]; then
  echo "Could not extract applicationId from build.gradle."
  exit 1
fi

echo "Installing APK on $DEVICE_ID..."
adb -s "$DEVICE_ID" install -r "$APK_PATH"

echo "Clearing app data for a fresh run..."
adb -s "$DEVICE_ID" shell pm clear "$APP_ID" || true

echo "Starting Metro..."
pushd "$EXAMPLE_DIR" >/dev/null
nohup pnpm start >"$LOG_DIR/metro.log" 2>&1 &
METRO_PID=$!
popd >/dev/null
trap 'kill "$METRO_PID" 2>/dev/null || true' EXIT

echo "Launching app..."
adb -s "$DEVICE_ID" shell monkey -p "$APP_ID" -c android.intent.category.LAUNCHER 1

echo "Running tests: $TESTS_TO_RUN"
cd "$PKG_DIR"
PLATFORM=android \
  DEVICE_ID="$DEVICE_ID" \
  BUNDLE_PATH="$APK_PATH" \
  APP_PACKAGE="$APP_ID" \
  APP_ACTIVITY="$APP_ID.MainActivity" \
  pnpm exec ts-node --project tsconfig.json src/runner.ts $TESTS_TO_RUN
