#!/usr/bin/env bash
#
# The Android target, sourced by scripts/e2e/run.sh. `provision` picks or boots
# a device, installs the debug APK (whose console harness is
# harness.ConsoleActivity), makes the host's devimint reachable from the
# device, starts the SDK's log capture and Appium, and exports what
# js/e2e/src/targets/android.ts reads: DEVICE_ID, APP_PACKAGE, SDK_LOG_FILE.
#
# It builds nothing: it installs the APK Gradle left in
# android/app/build/outputs/apk/debug (`just build-e2e-android`, or CI's
# android-apk.yaml artifact). A Gradle build alongside an emulator and a
# devimint federation starved the emulator until System UI stopped answering.

ANDROID_DIR="$REPO_ROOT/android"
APK_DIR="$ANDROID_DIR/app/build/outputs/apk/debug"
AVD_NAME="fedimint-e2e"
# A wedged emulator (no acceleration, out of memory) would otherwise hold
# the job until its own timeout, with nothing in the log saying why.
BOOT_TIMEOUT_SECS="${BOOT_TIMEOUT_SECS:-600}"

android_ensure_avd() {
  if emulator -list-avds | grep -qx "$AVD_NAME"; then return; fi
  local abi
  abi=$([[ "$(uname -m)" == arm64 || "$(uname -m)" == aarch64 ]] && echo arm64-v8a || echo x86_64)
  local image="system-images;android-34;google_apis;$abi"
  echo "No '$AVD_NAME' AVD — creating one ($image)..."
  echo "no" | avdmanager create avd -n "$AVD_NAME" -k "$image" --device "pixel_6" --force
}

# The first even console port in the emulator's 5554-5682 range that no
# attached emulator uses. Its adb serial is `emulator-<port>`, so choosing the
# port is how this script knows which device it booted.
android_free_port() {
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

android_boot() {
  android_ensure_avd
  local port log="$ARTIFACTS_DIR/emulator.log"
  port=$(android_free_port)
  DEVICE_ID="emulator-$port"
  echo "Booting $AVD_NAME as $DEVICE_ID"
  nohup emulator -avd "$AVD_NAME" -port "$port" -no-snapshot -no-boot-anim -no-window \
    -wipe-data -gpu swiftshader_indirect >"$log" 2>&1 &
  local pid=$!
  local deadline=$((SECONDS + BOOT_TIMEOUT_SECS))
  until [[ "$(adb -s "$DEVICE_ID" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" == "1" ]]; do
    if ! kill -0 "$pid" 2>/dev/null || ((SECONDS >= deadline)); then
      echo "The emulator did not finish booting; last lines of $log:" >&2
      tail -n 50 "$log" >&2 || true
      kill "$pid" 2>/dev/null || true
      exit 1
    fi
    sleep 2
  done
}

android_devices() { adb devices | awk 'NR>1 && $2=="device" {print $1}'; }

android_choose_device() {
  local devices=() line choice
  if [[ "${CI:-}" == "true" ]]; then
    # Non-interactive: reuse a running device, or boot the AVD.
    while IFS= read -r line; do devices+=("$line"); done < <(android_devices)
    if [[ ${#devices[@]} -gt 0 ]]; then DEVICE_ID=${devices[0]}; else android_boot; fi
    return
  fi
  while true; do
    devices=()
    while IFS= read -r line; do devices+=("$line"); done < <(android_devices)
    echo -e "\nChoose a device for the e2e run."
    echo -e "\e[1;33m⚠️  The app's data on the chosen device will be wiped.\e[0m"
    echo "1) Boot the '$AVD_NAME' AVD, creating it if needed (default)"
    echo "2) Refresh the device list"
    for i in "${!devices[@]}"; do echo "$((i + 3))) ${devices[$i]}"; done
    read -rp "Choice: " choice
    choice=${choice:-1}
    if [[ "$choice" == 1 ]]; then
      android_boot
      return
    elif [[ "$choice" =~ ^[0-9]+$ ]] && ((choice >= 3 && choice < ${#devices[@]} + 3)); then
      DEVICE_ID=${devices[$((choice - 3))]}
      return
    fi
  done
}

# devimint binds every guardian, gateway, the faucet and esplora to the host's
# 127.0.0.1, and the invite code carries those URLs sealed inside it. On the
# device 127.0.0.1 is the device, so `adb reverse` tunnels each port back to
# the host and the code works unmodified. Cleartext ws:// is fine: Android's
# cleartext policy governs the platform HTTP stacks, not the Rust client's
# sockets. The guardian window is reversed whole (4 ports per peer).
android_reverse_ports() {
  local base="$FM_FEDERATION_BASE_PORT"
  local last=$((base + ${FM_FED_SIZE:-4} * 4 - 1))
  local port
  echo "Forwarding devimint ports $base-$last plus gateways, faucet and esplora"
  for port in $(seq "$base" "$last") "${FM_PORT_GW_LND:-}" "${FM_PORT_GW_LDK:-}" \
    "${FM_PORT_FAUCET:-}" "${FM_PORT_ESPLORA:-}"; do
    [[ -n "$port" ]] || continue
    adb -s "$DEVICE_ID" reverse "tcp:$port" "tcp:$port" >/dev/null
  done
}

# The SDK's own logcat for the whole run, for the DNS check in
# js/e2e/src/targets/logs.ts. The property turns on the resolver's trace line
# listing the servers it read; it is read once per process when logging
# starts, so it is set before the first launch, and cleared on exit, since on
# a developer's phone it would otherwise outlive the run.
android_capture_logs() {
  SDK_LOG_FILE="$ARTIFACTS_DIR/fedimint-sdk-logcat.log"
  adb -s "$DEVICE_ID" shell setprop debug.fedimint_sdk.log \
    "'warn,fm=info,fedimint=info,n0_dns_resolver::system_config=trace'"
  adb -s "$DEVICE_ID" logcat -c
  adb -s "$DEVICE_ID" logcat -v time -s fedimint-sdk >"$SDK_LOG_FILE" 2>&1 &
  ANDROID_LOGCAT_PID=$!
  e2e_on_exit android_stop_logs
}

android_stop_logs() {
  kill "$ANDROID_LOGCAT_PID" 2>/dev/null || true
  # Empty means "use the default".
  adb -s "$DEVICE_ID" shell setprop debug.fedimint_sdk.log "''" 2>/dev/null || true
}

provision() {
  local bin
  for bin in adb emulator java; do
    command -v "$bin" >/dev/null 2>&1 || {
      echo "$bin not found on PATH. Run this inside 'nix develop .#android-tests'." >&2
      exit 1
    }
  done

  # `|| true`: under `set -e`, a missing directory would otherwise end the
  # script inside this assignment, before the message below.
  local apk
  apk=$(find "$APK_DIR" -maxdepth 1 -name '*.apk' 2>/dev/null | head -1 || true)
  if [[ -z "$apk" ]]; then
    echo "No debug APK in $APK_DIR. Build it first: just build-e2e-android" >&2
    exit 1
  fi
  APP_PACKAGE=$(grep 'applicationId' "$ANDROID_DIR/app/build.gradle.kts" | head -1 | awk -F '"' '{print $2}')

  android_choose_device
  echo "Installing $apk on $DEVICE_ID"
  adb -s "$DEVICE_ID" install -r "$apk"
  adb -s "$DEVICE_ID" shell pm clear "$APP_PACKAGE" >/dev/null || true
  android_reverse_ports
  android_capture_logs
  e2e_start_appium uiautomator2
  # The session launches the console itself (targets/android.ts).
  export DEVICE_ID APP_PACKAGE SDK_LOG_FILE
}
