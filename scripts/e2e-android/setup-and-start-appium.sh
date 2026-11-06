#!/usr/bin/env bash
#
# Ensures Appium (3.x) + the uiautomator2 driver are installed and starts the
# server in the background.
#
# Run this inside `nix develop .#android-tests` — that shell puts the
# pnpm-installed `appium` binary (js/android/integration-tests's
# node_modules/.bin, see its package.json) on PATH, sets APPIUM_HOME to a
# repo-local dir, and provides the Android SDK/emulator/platform-tools.
# Appium itself is a plain npm devDependency — Nix supplies the Android
# toolchain around it, not Appium itself.

set -euo pipefail

if ! command -v appium >/dev/null 2>&1; then
  echo "appium not found on PATH. Run this inside 'nix develop .#android-tests'" \
    "(after 'pnpm --dir js install' so its node_modules/.bin exists)."
  exit 1
fi

if [[ -z "${APPIUM_HOME:-}" ]]; then
  echo "APPIUM_HOME is not set. Run this inside 'nix develop .#android-tests'."
  exit 1
fi
mkdir -p "$APPIUM_HOME"
STATE_DIR="${APPIUM_STATE_DIR:-$APPIUM_HOME}"
mkdir -p "$STATE_DIR"

echo "Checking for a running Appium server..."
PID_FILE="$STATE_DIR/appium_pid.txt"
if [[ -f "$PID_FILE" ]]; then
  EXISTING_PID=$(cat "$PID_FILE")
  # kill -0 checks the process exists without needing `ps`, which isn't
  # guaranteed to be installed on minimal (e.g. self-hosted) runners.
  if kill -0 "$EXISTING_PID" 2>/dev/null; then
    echo "Appium already running (PID=$EXISTING_PID), no setup needed."
    exit 0
  fi
fi

echo "=== Ensuring Appium is installed & the uiautomator2 driver is ready ==="
echo "Using appium: $(command -v appium)"
echo "APPIUM_HOME: $APPIUM_HOME"
echo "STATE_DIR: $STATE_DIR"
appium --version

# Concurrent runs can share APPIUM_HOME; serialize driver installation with an
# atomic lock directory (portable on macOS and Linux without flock).
DRIVER_LOCK="$APPIUM_HOME/.driver-install.lock"
DRIVER_LOCK_HELD=false
release_driver_lock() {
  if [[ "$DRIVER_LOCK_HELD" == true ]]; then
    rm -f "$DRIVER_LOCK/pid"
    rmdir "$DRIVER_LOCK" 2>/dev/null || true
    DRIVER_LOCK_HELD=false
  fi
}
trap release_driver_lock EXIT

acquire_driver_lock() {
  local deadline=$((SECONDS + 180))
  while ! mkdir "$DRIVER_LOCK" 2>/dev/null; do
    if [[ -f "$DRIVER_LOCK/pid" ]]; then
      local owner
      owner=$(cat "$DRIVER_LOCK/pid" 2>/dev/null || true)
      if [[ -n "$owner" ]] && ! kill -0 "$owner" 2>/dev/null; then
        rm -f "$DRIVER_LOCK/pid"
        rmdir "$DRIVER_LOCK" 2>/dev/null || true
        continue
      fi
    fi
    if ((SECONDS >= deadline)); then
      echo "Timed out waiting for Appium driver install lock ($DRIVER_LOCK)" >&2
      exit 1
    fi
    sleep 1
  done
  echo "$$" >"$DRIVER_LOCK/pid"
  DRIVER_LOCK_HELD=true
}

if ! appium driver list --installed 2>&1 | grep -qi uiautomator2; then
  acquire_driver_lock
  if ! appium driver list --installed 2>&1 | grep -qi uiautomator2; then
    # The version js/pnpm-lock.yaml pins, read from where pnpm installed it next
    # to appium. Without a version this takes the newest driver on npm, which may
    # need a newer Appium than the one pnpm pins, and then the install fails.
    DRIVER_PACKAGE="$(dirname "$(command -v appium)")/../appium-uiautomator2-driver/package.json"
    DRIVER_VERSION="$(node -p 'require(process.argv[1]).version' "$DRIVER_PACKAGE")"
    echo "Installing uiautomator2 driver $DRIVER_VERSION..."
    appium driver install "uiautomator2@$DRIVER_VERSION"
  fi
  release_driver_lock
fi

echo "Running uiautomator2 driver doctor..."
appium driver doctor uiautomator2 || {
  echo "⚠️  uiautomator2 driver doctor reported issues — check ANDROID_HOME/adb/Java above."
}

LOG_FILE="$STATE_DIR/appium.log"
BASE_PORT="${APPIUM_BASE_PORT:-4723}"

echo "=== Starting Appium server in background ==="
APP_PORT=""
for attempt in 1 2 3; do
  CANDIDATE_PORT=$((BASE_PORT + attempt - 1))
  echo "--- attempt $attempt: port $CANDIDATE_PORT ---"
  [[ $attempt -gt 1 ]] && sleep 2

  if [[ "${DEBUG_MODE:-}" == "1" || "${DEBUG_MODE:-}" == "true" ]]; then
    APPIUM_LOG_LEVEL="debug"
  else
    APPIUM_LOG_LEVEL="info"
  fi

  ATTEMPT_LOG="$STATE_DIR/appium-attempt-${attempt}.log"
  # Loopback only: adb_shell lets any client that can reach this server run
  # shell commands on the device, and Appium otherwise listens on 0.0.0.0.
  nohup appium --address 127.0.0.1 --port "$CANDIDATE_PORT" \
    --log-level "$APPIUM_LOG_LEVEL" \
    --allow-insecure=uiautomator2:adb_shell,uiautomator2:chromedriver_autodownload \
    >"$ATTEMPT_LOG" 2>&1 </dev/null &
  APP_PID=$!
  echo "$APP_PID" >"$PID_FILE"
  cp "$ATTEMPT_LOG" "$LOG_FILE" 2>/dev/null || true

  for _ in $(seq 1 80); do
    if ! kill -0 "$APP_PID" 2>/dev/null; then break; fi
    # We already know which port we asked Appium to bind, so confirm it's
    # actually serving there rather than inspecting open sockets (lsof isn't
    # guaranteed to be installed, e.g. on minimal self-hosted runners).
    if curl -sf -o /dev/null "http://127.0.0.1:$CANDIDATE_PORT/status"; then
      APP_PORT="$CANDIDATE_PORT"
      break
    fi
    sleep 0.5
  done

  if [[ -n "$APP_PORT" ]]; then
    echo "Appium started (PID=$APP_PID, port=$APP_PORT)"
    echo "$APP_PORT" >"$STATE_DIR/appium_port.txt"
    break
  fi

  if ! kill -0 "$APP_PID" 2>/dev/null; then
    echo "⚠️  attempt $attempt: appium died. Log:"
    cat "$ATTEMPT_LOG" 2>/dev/null || true
  else
    echo "⚠️  attempt $attempt: appium alive but no port bound after 40s. Log:"
    tail -40 "$ATTEMPT_LOG" 2>/dev/null || true
  fi
  kill -9 "$APP_PID" 2>/dev/null || true
done

if [[ -z "$APP_PORT" ]]; then
  echo "❌ Appium failed to start after 3 attempts"
  exit 1
fi

echo "APPIUM_PORT=$APP_PORT"
