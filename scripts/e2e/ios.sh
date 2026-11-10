#!/usr/bin/env bash
#
# The iOS target, sourced by scripts/e2e/run.sh. `provision` creates or boots
# the `fedimint-e2e` simulator (or uses DEVICE_ID if set), installs the
# harness app (ios/Harness), starts the SDK's log capture and Appium, and
# exports what js/e2e/src/targets/ios.ts reads: DEVICE_ID, BUNDLE_ID,
# APP_PATH, SDK_LOG_FILE.
#
# Nothing is forwarded: the simulator shares the host's network, so the
# 127.0.0.1 in devimint's invite code is devimint.
#
# With IOS_DEVICE=<udid> and IOS_TEAM_ID it uses that connected device
# instead, with the signed build `just build-e2e-ios-device` leaves. A device
# cannot reach the host's loopback, where devimint is, so only the
# federation-free tests can pass there.
#
# It builds nothing: it installs the app `just build-e2e-ios` leaves in
# ios/Harness/build (or CI's artifact, unpacked to the same place).

IOS_PRODUCTS="$REPO_ROOT/ios/Harness/build/Build/Products"
IOS_APP_PATH="$IOS_PRODUCTS/Debug-iphonesimulator/FedimintHarness.app"
IOS_BUNDLE_ID="org.fedimint.harness"
IOS_SIM_NAME="fedimint-e2e"

# simctl's JSON, read with node, which every e2e shell has.
ios_json() { node -e "let s='';process.stdin.on('data',d=>s+=d).on('end',()=>{const j=JSON.parse(s);$1})"; }

# The newest available iOS runtime, and the newest plain iPhone it supports.
ios_create_simulator() {
  local picked runtime device_type
  picked=$(xcrun simctl list -j runtimes | ios_json '
    const ios = j.runtimes.filter(r => r.isAvailable && r.platform === "iOS")
      .sort((a, b) => a.version.localeCompare(b.version, undefined, {numeric: true}))
    const runtime = ios[ios.length - 1]
    const phones = (runtime ? runtime.supportedDeviceTypes : []).filter(d => /^iPhone \d+$/.test(d.name))
      .sort((a, b) => parseInt(a.name.slice(7)) - parseInt(b.name.slice(7)))
    if (runtime && phones.length) console.log(runtime.identifier, phones[phones.length - 1].identifier)')
  read -r runtime device_type <<<"$picked"
  if [[ -z "${device_type:-}" ]]; then
    echo "No iOS simulator runtime with an iPhone is installed (Xcode → Settings → Components)." >&2
    exit 1
  fi
  echo "Creating the $IOS_SIM_NAME simulator ($device_type, $runtime)" >&2
  xcrun simctl create "$IOS_SIM_NAME" "$device_type" "$runtime"
}

ios_choose_device() {
  if [[ -z "${DEVICE_ID:-}" ]]; then
    DEVICE_ID=$(xcrun simctl list -j devices available | ios_json "
      const all = Object.values(j.devices).flat().filter(d => d.name === '$IOS_SIM_NAME')
      console.log(all.length ? all[0].udid : '')")
    [[ -n "$DEVICE_ID" ]] || DEVICE_ID=$(ios_create_simulator)
  fi
  echo "Booting simulator $DEVICE_ID"
  # -b boots it if it is not already, then waits for the boot to finish.
  xcrun simctl bootstatus "$DEVICE_ID" -b >/dev/null
}

# The SDK's os_log lines for the run, for the DNS check in
# js/e2e/src/targets/logs.ts. The filter itself is set at each launch
# (targets/ios.ts); trace lines are private unless the subsystem says otherwise.
ios_capture_logs() {
  SDK_LOG_FILE="$ARTIFACTS_DIR/fedimint-sdk-oslog.log"
  xcrun simctl spawn "$DEVICE_ID" log config --subsystem org.fedimint.sdk \
    --mode "level:debug,private_data:on" >/dev/null 2>&1 || true
  xcrun simctl spawn "$DEVICE_ID" log stream --level debug --style compact \
    --predicate 'subsystem == "org.fedimint.sdk"' >"$SDK_LOG_FILE" 2>&1 &
  IOS_LOG_PID=$!
  e2e_on_exit ios_stop_logs
}

ios_stop_logs() { kill "$IOS_LOG_PID" 2>/dev/null || true; }

provision() {
  # The host's Xcode, not a nix SDK: simctl, and the xcodebuild Appium runs
  # to build WebDriverAgent, need the full Xcode xcode-select points at.
  unset DEVELOPER_DIR SDKROOT
  export PATH="/usr/bin:$PATH"
  local bin
  for bin in xcrun node; do
    command -v "$bin" >/dev/null 2>&1 || {
      echo "$bin not found on PATH. Run this inside 'nix develop .#ios-tests'." >&2
      exit 1
    }
  done
  if [[ -n "${IOS_DEVICE:-}" ]]; then
    ios_provision_device
    return
  fi
  if [[ ! -d "$IOS_APP_PATH" ]]; then
    echo "No harness app at $IOS_APP_PATH. Build it first: just build-e2e-ios" >&2
    exit 1
  fi

  ios_choose_device
  xcrun simctl uninstall "$DEVICE_ID" "$IOS_BUNDLE_ID" >/dev/null 2>&1 || true
  echo "Installing $IOS_APP_PATH on $DEVICE_ID"
  xcrun simctl install "$DEVICE_ID" "$IOS_APP_PATH"
  ios_capture_logs
  e2e_start_appium xcuitest
  # The session launches the harness itself (targets/ios.ts).
  BUNDLE_ID="$IOS_BUNDLE_ID"
  APP_PATH="$IOS_APP_PATH"
  export DEVICE_ID BUNDLE_ID APP_PATH SDK_LOG_FILE
}

# A connected device: the signed build, its own bundle id (a free team cannot
# sign `org.fedimint.harness`), installed with devicectl. Its log is not
# captured: os_log on a device is not reachable from here the way a
# simulator's is.
ios_provision_device() {
  : "${IOS_TEAM_ID:?set IOS_TEAM_ID to the team the build and WebDriverAgent are signed with}"
  IOS_APP_PATH="$IOS_PRODUCTS/Debug-iphoneos/FedimintHarness.app"
  if [[ ! -d "$IOS_APP_PATH" ]]; then
    echo "No device build at $IOS_APP_PATH. Build it first: just build-e2e-ios-device <team> <bundle id>" >&2
    exit 1
  fi
  IOS_BUNDLE_ID=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$IOS_APP_PATH/Info.plist")
  DEVICE_ID="$IOS_DEVICE"
  echo "Installing $IOS_BUNDLE_ID on device $DEVICE_ID"
  xcrun devicectl device uninstall app --device "$DEVICE_ID" "$IOS_BUNDLE_ID" >/dev/null 2>&1 || true
  xcrun devicectl device install app --device "$DEVICE_ID" "$IOS_APP_PATH"
  e2e_start_appium xcuitest
  BUNDLE_ID="$IOS_BUNDLE_ID"
  APP_PATH="$IOS_APP_PATH"
  IOS_REAL_DEVICE=1
  export DEVICE_ID BUNDLE_ID APP_PATH IOS_REAL_DEVICE IOS_TEAM_ID
}
