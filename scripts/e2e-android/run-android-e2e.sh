#!/usr/bin/env bash
#
# Picks or boots an Android device, installs the example APK (android/app) on it,
# and runs the Appium test runner against it.
#
# The app under test is the native example app in android/, not a React Native
# example: the same app kotlin-sdk.yaml compiles, driven on a device so the
# generated bindings and the .so behind them are exercised at runtime rather
# than merely compiled.
#
# This script builds nothing. It expects a finished debug APK where Gradle
# leaves it — built by `just build-android-apk` locally, or by
# android-apk.yaml on another machine in CI — and stops early if there isn't
# one. That is deliberate rather than tidy: it runs inside a devimint
# federation, on a machine that is about to boot an emulator too, and a Gradle
# build alongside them starved the emulator until Android's own System UI
# stopped responding and every test failed against that dialog instead of
# against the app.

set -euo pipefail

REPO_ROOT=$(git rev-parse --show-toplevel)
ANDROID_DIR="$REPO_ROOT/android"
PKG_DIR="$REPO_ROOT/js/android/integration-tests"
APK_DIR="$ANDROID_DIR/app/build/outputs/apk/debug"

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

# Checked before anything slow starts, so a missing APK costs seconds rather
# than an emulator boot. `find` rather than a fixed name: AGP owns the file's
# name, and this is the one place that would otherwise have to guess it.
#
# `|| true` is what makes the message below reachable at all: with `set -e` and
# `pipefail`, `find` failing on a directory that does not exist yet — the usual
# way to get here — would end the script inside this assignment, silently.
APK_PATH=$(find "$APK_DIR" -maxdepth 1 -name '*.apk' 2>/dev/null | head -1 || true)
if [[ -z "$APK_PATH" || ! -f "$APK_PATH" ]]; then
  cat >&2 <<MSG
No example APK in $APK_DIR.

This script installs a finished APK and never builds one. Build it first:

  just build-android-apk

or run the whole thing, which does that first:

  just test-android-e2e

(CI downloads it from android-apk.yaml's artifact into that same directory.)
MSG
  exit 1
fi
echo "Using APK: $APK_PATH"

APP_ID=$(grep 'applicationId' "$ANDROID_DIR/app/build.gradle.kts" | head -1 | awk -F '"' '{print $2}')
if [[ -z "$APP_ID" ]]; then
  echo "Could not extract applicationId from android/app/build.gradle.kts."
  exit 1
fi
# The one activity android/app/src/main/AndroidManifest.xml declares, exported
# with the LAUNCHER intent. Named once here because both the launch below and
# the runner's own capabilities need it.
APP_ACTIVITY="$APP_ID.MainActivity"

# Appium is a plain npm devDependency of the test package; the `android-tests`
# shell puts the workspace's hoisted js/node_modules/.bin on PATH, but the
# install itself still has to have happened.
if ! command -v appium >/dev/null 2>&1; then
  echo "appium not found on PATH — installing workspace deps..."
  pnpm --dir "$REPO_ROOT/js" install --frozen-lockfile
fi
if ! command -v appium >/dev/null 2>&1; then
  echo "appium still not on PATH. Run this inside 'nix develop .#android-tests'," \
    "which puts $REPO_ROOT/js/node_modules/.bin on it."
  exit 1
fi

LOG_DIR="${APPIUM_HOME:-$PKG_DIR/.appium}"
mkdir -p "$LOG_DIR"

# Appium is started with nohup, so it outlives this script unless stopped. In
# CI the runner goes away with the job anyway; on a developer machine it would
# otherwise keep a shell-enabled server running after the tests are done.
# Stops whichever server the PID file names, including one a previous run
# left behind and this run reused, so the next run starts with current flags.
stop_appium() {
  local state_dir="${APPIUM_HOME:-$LOG_DIR}"
  local pid_file="$state_dir/appium_pid.txt"
  if [[ -f "$pid_file" ]]; then
    kill "$(cat "$pid_file")" 2>/dev/null || true
    rm -f "$pid_file" "$state_dir/appium_port.txt"
  fi
}
trap stop_appium EXIT

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

# How long the emulator gets to reach a full boot. A wedged one — no KVM,
# swiftshader falling over, the host out of memory — otherwise leaves
# `adb wait-for-device` blocking forever, and in CI that is the job's entire
# 120-minute timeout spent waiting for a device that is never going to appear,
# with the emulator log cancelled along with the job instead of uploaded.
BOOT_TIMEOUT_SECS="${BOOT_TIMEOUT_SECS:-600}"

# The workflow uploads $LOG_DIR/*.log, but a job killed by its own timeout
# takes that step with it. Printing the tail inline means the reason a boot
# failed is in the job log either way.
boot_failure_log() {
  echo "--- last 50 lines of $LOG_DIR/emulator.log ---" >&2
  tail -n 50 "$LOG_DIR/emulator.log" >&2 || true
  echo "--- end of emulator.log ---" >&2
}

# The first even console port, from the emulator's own 5554-5682 range, that
# no attached emulator is using. An emulator's adb serial is
# `emulator-<console port>`, so choosing the port is how this script knows
# which device it booted.
free_emulator_port() {
  local port
  for port in $(seq 5554 2 5682); do
    if ! adb devices | awk 'NR>1 {print $1}' | grep -qx "emulator-$port"; then
      echo "$port"
      return
    fi
  done
  echo "No free emulator console port in 5554-5682." >&2
  exit 1
}

# Sets BOOTED_SERIAL to the new emulator's adb serial.
boot_avd() {
  ensure_avd_exists
  local port
  port=$(free_emulator_port)
  BOOTED_SERIAL="emulator-$port"
  echo "Booting AVD: $AVD_NAME as $BOOTED_SERIAL"
  nohup emulator -avd "$AVD_NAME" -port "$port" -no-snapshot -no-boot-anim -no-window -wipe-data -gpu swiftshader_indirect \
    >"$LOG_DIR/emulator.log" 2>&1 &
  local emu_pid=$!

  # One loop for the transport and the boot both: `getprop` against an absent
  # device simply fails, so waiting on the property covers `wait-for-device`
  # too, and there is only one wait left to bound. Always by serial: with
  # another device attached, a bare `adb shell` either reads that device's
  # boot state or fails with "more than one device/emulator".
  local deadline=$((SECONDS + BOOT_TIMEOUT_SECS))
  until [[ "$(adb -s "$BOOTED_SERIAL" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" == "1" ]]; do
    if ! kill -0 "$emu_pid" 2>/dev/null; then
      echo "Emulator exited before the device finished booting." >&2
      boot_failure_log
      exit 1
    fi
    if ((SECONDS >= deadline)); then
      echo "Emulator did not finish booting within ${BOOT_TIMEOUT_SECS}s; giving up." >&2
      boot_failure_log
      kill "$emu_pid" 2>/dev/null || true
      exit 1
    fi
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
    DEVICE_ID=$BOOTED_SERIAL
  else
    DEVICE_ID=${device_ids[0]}
  fi
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
      DEVICE_ID=$BOOTED_SERIAL
      break
    elif [[ "$choice" == "2" ]]; then
      continue
    elif [[ "$choice" =~ ^[0-9]+$ ]] && [ "$choice" -ge 3 ] && [ "$choice" -le "$((${#device_ids[@]} + 2))" ]; then
      DEVICE_ID=${device_ids[$((choice - 3))]}
      break
    else
      if [[ ${#device_ids[@]} -eq 0 ]]; then
        echo "Invalid choice, and there is no device attached to fall back to."
        continue
      fi
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

# ── The SDK's own logcat, for the whole run ─────────────────────────────
#
# The runner dumps logcat only when a test fails, and some SDK failures never
# fail a test. The one this exists for: iroh's DNS resolver reads the device's
# DNS servers through ConnectivityManager, which needs the Android context
# published and ACCESS_NETWORK_STATE granted. When either is missing it logs a
# warning and falls back to Google's DNS servers, and the tests still pass
# because the Lightning test can use the plain HTTP gateway.
#
# So the property below turns on hickory's trace line that lists the servers
# it read ("Got DNS servers: …"), the SDK's tag is recorded to a file for the
# whole run (a ring-buffer dump at the end could have rotated early lines
# out), and check_dns_config fails the run if the fallback warning appears.
# When the Lightning test runs it also requires the trace line: that test
# dials devimint's iroh gateway, which is what builds a DNS resolver, so a
# run without the line has not exercised the path at all and a missing
# warning would prove nothing.
# The property is read once per app process when logging starts, so it is set
# before the first launch. The rest of the filter is the SDK's default, and the
# property is cleared again on exit (empty means "use the default"), since on
# a developer's own phone it would otherwise outlive the run until a reboot.
SDK_LOGCAT="$LOG_DIR/fedimint-sdk-logcat.log"
adb -s "$DEVICE_ID" shell setprop debug.fedimint_sdk.log \
  "'warn,fm=info,fedimint=info,hickory_resolver::system_conf=trace'"
adb -s "$DEVICE_ID" logcat -c
adb -s "$DEVICE_ID" logcat -v time -s fedimint-sdk >"$SDK_LOGCAT" 2>&1 &
LOGCAT_PID=$!
# On any exit, not only the normal one through check_dns_config.
cleanup() {
  kill "$LOGCAT_PID" 2>/dev/null || true
  adb -s "$DEVICE_ID" shell setprop debug.fedimint_sdk.log "''" 2>/dev/null || true
  stop_appium
}
trap cleanup EXIT

check_dns_config() {
  kill "$LOGCAT_PID" 2>/dev/null || true
  if grep -q "Failed to read the system's DNS config" "$SDK_LOGCAT"; then
    echo "The SDK could not read the device's DNS configuration and fell back to Google DNS:" >&2
    grep -B2 -A2 "Failed to read the system's DNS config" "$SDK_LOGCAT" >&2
    return 1
  fi

  local found
  found=$(grep "Got DNS servers" "$SDK_LOGCAT" || true)
  if [[ -n "$found" ]]; then
    echo "The SDK read the device's DNS configuration:"
    echo "$found"
    return 0
  fi
  # Word-split on purpose: TESTS_TO_RUN is the runner's argument list.
  # shellcheck disable=SC2086
  if printf '%s\n' $TESTS_TO_RUN | grep -qxE 'all|lightning'; then
    echo "The Lightning test ran, but no DNS resolver read the device's DNS configuration" \
      "(no \"Got DNS servers\" in $SDK_LOGCAT), so the platform DNS path went untested." >&2
    return 1
  fi
  echo "No DNS resolver read the device's DNS configuration during this run."
}

echo "Launching app..."
# `am start` rather than `monkey`: monkey launches the activity and then injects
# the requested number of random events into it, so the app can be tapped or a
# field focused before a test has connected. The activity is exported under a
# name this script already knows, and -W waits for the launch to finish.
adb -s "$DEVICE_ID" shell am start -W -n "$APP_ID/$APP_ACTIVITY"

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
  APP_ACTIVITY="$APP_ACTIVITY" \
  FAUCET="${FAUCET:-http://localhost:${FM_PORT_FAUCET:-15243}}" \
  pnpm exec ts-node --project tsconfig.json src/runner.ts $TESTS_TO_RUN &&
  tests_status=0 || tests_status=$?

if ! check_dns_config && [[ "$tests_status" -eq 0 ]]; then
  tests_status=1
fi
exit "$tests_status"
