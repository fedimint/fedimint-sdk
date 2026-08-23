#!/usr/bin/env bash
#
# Ensures Appium (3.x) + the uiautomator2 driver are installed and starts the
# server in the background.
#
# Run this inside `nix develop .#android-tests` — that shell puts the
# pnpm-installed `appium` binary (js/react-native/integration-tests-android's
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

echo "Checking for a running Appium server..."
APPIUM_PID=""
while IFS= read -r line; do
  pid=$(echo "$line" | awk '{print $1}')
  args=$(echo "$line" | cut -d' ' -f2-)
  if [[ "$args" == *"integration-tests-android"* && "$args" == *"appium"* ]]; then
    APPIUM_PID="$pid"
    break
  fi
done < <(ps -axo pid=,args= | grep appium || true)

if [[ -n "$APPIUM_PID" ]]; then
  echo "Appium already running (PID=$APPIUM_PID), no setup needed."
  exit 0
fi

echo "=== Ensuring Appium is installed & the uiautomator2 driver is ready ==="
echo "Using appium: $(command -v appium)"
echo "APPIUM_HOME: $APPIUM_HOME"
appium --version

if ! appium driver list --installed 2>&1 | grep -qi uiautomator2; then
  echo "Installing uiautomator2 driver..."
  appium driver install uiautomator2
fi

echo "Running uiautomator2 driver doctor..."
appium driver doctor uiautomator2 || {
  echo "⚠️  uiautomator2 driver doctor reported issues — check ANDROID_HOME/adb/Java above."
}

PID_FILE="$APPIUM_HOME/appium_pid.txt"
LOG_FILE="$APPIUM_HOME/appium.log"

echo "=== Starting Appium server in background ==="
APP_PORT=""
for attempt in 1 2 3; do
  CANDIDATE_PORT=$((4722 + attempt))
  echo "--- attempt $attempt: port $CANDIDATE_PORT ---"
  lsof -ti:"$CANDIDATE_PORT" 2>/dev/null | xargs kill -9 2>/dev/null || true
  [[ $attempt -gt 1 ]] && sleep 2

  if [[ "${DEBUG_MODE:-}" == "1" || "${DEBUG_MODE:-}" == "true" ]]; then
    APPIUM_LOG_LEVEL="debug"
  else
    APPIUM_LOG_LEVEL="info"
  fi

  ATTEMPT_LOG="$APPIUM_HOME/appium-attempt-${attempt}.log"
  nohup appium --port "$CANDIDATE_PORT" \
    --log-level "$APPIUM_LOG_LEVEL" \
    --allow-insecure=uiautomator2:adb_shell,uiautomator2:chromedriver_autodownload \
    >"$ATTEMPT_LOG" 2>&1 </dev/null &
  APP_PID=$!
  echo "$APP_PID" >"$PID_FILE"
  cp "$ATTEMPT_LOG" "$LOG_FILE" 2>/dev/null || true

  for _ in $(seq 1 80); do
    if ! kill -0 "$APP_PID" 2>/dev/null; then break; fi
    FOUND_PORT=$(lsof -Pan -p "$APP_PID" -a -iTCP -sTCP:LISTEN 2>/dev/null | awk 'NR>1 {split($9,a,":"); print a[2]}' | head -1 || true)
    if [[ -n "$FOUND_PORT" ]]; then APP_PORT="$FOUND_PORT"; break; fi
    sleep 0.5
  done

  if [[ -n "$APP_PORT" ]]; then
    echo "Appium started (PID=$APP_PID, port=$APP_PORT)"
    echo "$APP_PORT" >"$APPIUM_HOME/appium_port.txt"
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
