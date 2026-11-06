#!/usr/bin/env bash
#
#   scripts/e2e/appium.sh <driver>      driver: uiautomator2 | xcuitest
#
# Ensures the Appium driver is installed, at the version js/pnpm-lock.yaml
# pins, and starts Appium in the background on the first free port from 4723,
# recording it in $APPIUM_HOME/appium_port.txt and its PID in appium_pid.txt.
# Reuses a server this script started earlier if it is still running.
#
# Appium and its drivers are plain npm dependencies of js/e2e; the target's
# nix shell puts js/node_modules/.bin on PATH.
set -euo pipefail

DRIVER="${1:?usage: appium.sh <uiautomator2|xcuitest>}"
case "$DRIVER" in
  # adb_shell lets the runner keep the screen on and start activities by name.
  uiautomator2) INSECURE="uiautomator2:adb_shell,uiautomator2:chromedriver_autodownload" ;;
  xcuitest) INSECURE="" ;;
  *)
    echo "unknown Appium driver '$DRIVER'" >&2
    exit 1
    ;;
esac

if ! command -v appium >/dev/null 2>&1; then
  echo "appium not found on PATH. Run 'pnpm --dir js install' inside the target's nix shell," \
    "which puts js/node_modules/.bin on PATH." >&2
  exit 1
fi
: "${APPIUM_HOME:?APPIUM_HOME is not set}"
mkdir -p "$APPIUM_HOME"

PID_FILE="$APPIUM_HOME/appium_pid.txt"
if [[ -f "$PID_FILE" ]] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null &&
  [[ -f "$APPIUM_HOME/appium_port.txt" ]]; then
  echo "Appium already running (PID=$(cat "$PID_FILE"))."
  exit 0
fi

echo "appium $(appium --version) at $(command -v appium), APPIUM_HOME=$APPIUM_HOME"
if ! appium driver list --installed 2>&1 | grep -qi "$DRIVER"; then
  # The version the lockfile pins, read from where pnpm installed it next to
  # appium. Without one, this would take the newest driver on npm, which may
  # need a newer Appium than the pinned one.
  PACKAGE="$(dirname "$(command -v appium)")/../appium-$DRIVER-driver/package.json"
  VERSION="$(node -p 'require(process.argv[1]).version' "$PACKAGE")"
  echo "Installing the $DRIVER driver $VERSION..."
  appium driver install "$DRIVER@$VERSION"
fi
appium driver doctor "$DRIVER" || echo "⚠️  $DRIVER driver doctor reported issues (above)."

LOG_LEVEL=info
[[ "${DEBUG_MODE:-}" == "1" || "${DEBUG_MODE:-}" == "true" ]] && LOG_LEVEL=debug

for PORT in 4723 4724 4725; do
  LOG="$APPIUM_HOME/appium-$PORT.log"
  # Loopback only: with adb_shell, any client that reaches this server can run
  # shell commands on the device, and Appium otherwise listens on 0.0.0.0.
  nohup appium --address 127.0.0.1 --port "$PORT" --log-level "$LOG_LEVEL" \
    ${INSECURE:+--allow-insecure="$INSECURE"} >"$LOG" 2>&1 </dev/null &
  PID=$!
  echo "$PID" >"$PID_FILE"
  for _ in $(seq 1 80); do
    kill -0 "$PID" 2>/dev/null || break
    if curl -sf -o /dev/null "http://127.0.0.1:$PORT/status"; then
      echo "$PORT" >"$APPIUM_HOME/appium_port.txt"
      cp "$LOG" "$APPIUM_HOME/appium.log" 2>/dev/null || true
      echo "Appium started (PID=$PID, port=$PORT)"
      exit 0
    fi
    sleep 0.5
  done
  echo "⚠️  Appium did not come up on port $PORT:" >&2
  tail -40 "$LOG" >&2 || true
  kill -9 "$PID" 2>/dev/null || true
done

echo "❌ Appium failed to start on ports 4723-4725" >&2
exit 1
