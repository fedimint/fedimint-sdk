#!/usr/bin/env bash
#
# Builds the Android demo app (android/app) against the generated Kotlin SDK,
# picks or boots an Android device, installs the APK, and runs the Appium test
# runner against it.
#
# The app under test is the native demo in android/, not a React Native
# example: the same APK that kotlin-sdk.yaml assembles, driven on a device so
# the generated bindings and the .so behind them are exercised at runtime
# rather than merely compiled.

set -euo pipefail

REPO_ROOT=$(git rev-parse --show-toplevel)
ANDROID_DIR="$REPO_ROOT/android"
PKG_DIR="$REPO_ROOT/js/android/integration-tests"

# jniLibs and the generated Kotlin are both gitignored build outputs, so the
# two halves of the SDK payload have to be present before Gradle runs.
JNI_LIBS="$ANDROID_DIR/fedimint-sdk/src/main/jniLibs"
GENERATED_KT="$ANDROID_DIR/fedimint-sdk/src/main/java/org/fedimint/sdk"

for bin in adb emulator java; do
  if ! command -v "$bin" >/dev/null 2>&1; then
    echo "$bin not found on PATH. Run this inside 'nix develop .#android-tests'."
    exit 1
  fi
done

# Every test in the suite runs against a devimint federation — there is no
# federation-free mode, so that what CI runs and what a contributor runs are
# the same thing. devimint exports these when it execs this script; without
# them the federation-backed tests would fail one by one on a connection
# refused, which is a slower and much less obvious way to learn the same
# thing.
if [[ -z "${FM_FEDERATION_BASE_PORT:-}" ]]; then
  cat >&2 <<'MSG'
No federation in the environment (FM_FEDERATION_BASE_PORT is unset).

This suite runs inside devimint, the same way the wasm one does. Start it with:

  just test-android-e2e

or, by hand:

  nix develop .#android-tests -c scripts/setup_test_shell.sh bash scripts/e2e-android/run-android-e2e.sh
MSG
  exit 1
fi

echo "=== Android E2E (SDK) tests ==="

cd "$REPO_ROOT"
if [[ "${SKIP_BINDINGS_BUILD:-}" == "true" ]]; then
  # CI path: android-native.yaml cross-compiled the .so and the workflow
  # generated the Kotlin from it, both restored into the Gradle project
  # before this script runs.
  echo "SKIP_BINDINGS_BUILD=true — expecting jniLibs + generated Kotlin already in place."
  for dir in "$JNI_LIBS" "$GENERATED_KT"; do
    if [[ ! -d "$dir" ]] || [[ -z "$(ls -A "$dir" 2>/dev/null)" ]]; then
      echo "$dir is missing or empty — nothing to build the app against." >&2
      echo "Unset SKIP_BINDINGS_BUILD to build it here, or restore the artifact first." >&2
      exit 1
    fi
  done
else
  # Cross-compiles the .so via Nix (cached) and regenerates the Kotlin from
  # it — the same two scripts CI runs as android-native.yaml and the bindings
  # step of kotlin-sdk.yaml, so a local run and CI can never diverge.
  "$REPO_ROOT/scripts/build-android-sdk.sh"
fi

# Appium is a plain npm devDependency of the test package; the `android-tests`
# shell puts its node_modules/.bin on PATH, but the install itself still has
# to have happened.
if ! command -v appium >/dev/null 2>&1; then
  echo "appium not found on PATH — installing workspace deps..."
  pnpm --dir "$REPO_ROOT/js" install --frozen-lockfile
fi
if ! command -v appium >/dev/null 2>&1; then
  echo "appium still not on PATH. Run this inside 'nix develop .#android-tests'," \
    "which puts $PKG_DIR/node_modules/.bin on it."
  exit 1
fi

LOG_DIR="${APPIUM_HOME:-$PKG_DIR/.appium}"
mkdir -p "$LOG_DIR"

bash "$REPO_ROOT/scripts/e2e-android/setup-and-start-appium.sh"

# setup-and-start-appium.sh retries on the next port up if 4723 is taken, and
# records where it landed. Without this the runner would fall back to its
# 4723 default and talk to nothing.
if [[ -f "${APPIUM_HOME:-$LOG_DIR}/appium_port.txt" ]]; then
  APPIUM_PORT=$(cat "${APPIUM_HOME:-$LOG_DIR}/appium_port.txt")
  export APPIUM_PORT
  echo "Appium is serving on port $APPIUM_PORT"
fi

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
  nohup emulator -avd "$AVD_NAME" -no-snapshot -no-boot-anim -no-window -wipe-data -gpu swiftshader_indirect \
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
    echo "Which tests to run? (mnemonic, inviteCode, federation, lightning, mint, all)"
    read -r TESTS_TO_RUN
    TESTS_TO_RUN=${TESTS_TO_RUN:-all}
  fi
fi

echo "Building the demo APK..."
pushd "$ANDROID_DIR" >/dev/null
./gradlew :app:assembleDebug
APK_PATH=$(find "$PWD/app/build/outputs/apk/debug" -name "*.apk" | head -1)
popd >/dev/null

if [[ ! -f "$APK_PATH" ]]; then
  echo "APK not found after build!"
  exit 1
fi

APP_ID=$(grep 'applicationId' "$ANDROID_DIR/app/build.gradle.kts" | head -1 | awk -F '"' '{print $2}')
if [[ -z "$APP_ID" ]]; then
  echo "Could not extract applicationId from android/app/build.gradle.kts."
  exit 1
fi

# ── The federation, as seen from inside the emulator ────────────────────
#
# devimint binds every guardian, gateway and the faucet to 127.0.0.1 on the
# host, and the invite code it hands out carries those URLs verbatim
# (`ws://127.0.0.1:<api port>`, see devimint/src/vars.rs's FM_API_URL). Inside
# the emulator 127.0.0.1 is the emulator, so the app would dial itself. The
# usual fix of rewriting the host to 10.0.2.2 is not available: the URLs are
# sealed inside a bech32m invite code the app parses.
#
# `adb reverse` instead makes the device's own 127.0.0.1:<port> tunnel out to
# the host's, so the invite code works unmodified and the app needs no
# test-only awareness of where the federation is.
#
# Cleartext ws:// is fine here: Android's cleartext policy governs the
# platform HTTP stacks and WebView, not the raw sockets the Rust client opens.
#
# Ports, all from what devimint exports into this process:
#   guardians  FM_FEDERATION_BASE_PORT .. + 4 per peer (PORTS_PER_FEDIMINTD),
#              covering p2p/api/ui/metrics — the whole window, since reversing
#              a port nothing listens on is harmless and cheaper than working
#              out which peer owns which offset.
#   gateways   FM_PORT_GW_LND / FM_PORT_GW_LDK — the client fetches an invoice
#              from the gateway's own API, not through the federation.
#   faucet     FM_PORT_FAUCET, so a test could reach it from the device too;
#              the runner itself talks to it from the host.
reverse_devimint_ports() {
  local base="$FM_FEDERATION_BASE_PORT"
  local fed_size="${FM_FED_SIZE:-4}"
  local ports_per_peer=4
  local last=$((base + fed_size * ports_per_peer - 1))

  echo "Forwarding devimint ports into the emulator: $base-$last plus gateways/faucet"
  local port
  for port in $(seq "$base" "$last") \
    "${FM_PORT_GW_LND:-}" "${FM_PORT_GW_LDK:-}" "${FM_PORT_FAUCET:-}"; do
    [[ -n "$port" ]] || continue
    adb -s "$DEVICE_ID" reverse "tcp:$port" "tcp:$port" >/dev/null
  done
}

echo "Installing APK on $DEVICE_ID..."
adb -s "$DEVICE_ID" install -r "$APK_PATH"

reverse_devimint_ports

echo "Clearing app data for a fresh run..."
adb -s "$DEVICE_ID" shell pm clear "$APP_ID" || true

echo "Launching app..."
adb -s "$DEVICE_ID" shell monkey -p "$APP_ID" -c android.intent.category.LAUNCHER 1

echo "Running tests: $TESTS_TO_RUN"
cd "$PKG_DIR"
# FAUCET is read by src/faucet/FaucetClient.ts, which runs here on the host,
# so it keeps the host's own port. js/vitest.config.ts builds the same URL the
# same way for the wasm suite, including the 15243 fallback devimint used
# before it started allocating a free port per run.
PLATFORM=android \
  DEVICE_ID="$DEVICE_ID" \
  BUNDLE_PATH="$APK_PATH" \
  APP_PACKAGE="$APP_ID" \
  APP_ACTIVITY="$APP_ID.MainActivity" \
  FAUCET="${FAUCET:-http://localhost:${FM_PORT_FAUCET:-15243}}" \
  pnpm exec ts-node --project tsconfig.json src/runner.ts $TESTS_TO_RUN
