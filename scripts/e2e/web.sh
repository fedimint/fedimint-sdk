#!/usr/bin/env bash
#
# The web target, sourced by scripts/e2e/run.sh. `provision` serves the built
# web harness (js/web/sdk-harness) with `vite preview` on localhost, a secure
# context as the SDK's storage needs, and exports what
# js/e2e/src/targets/web.ts reads: HARNESS_URL, and SDK_LOG_FILE for the
# page's and the SDK worker's console.
#
# The browser runs on the host, so devimint's 127.0.0.1 needs no forwarding.
# It builds nothing: it serves what `just build-e2e-web` left in
# js/web/sdk-harness/dist.

WEB_HARNESS_DIR="$REPO_ROOT/js/web/sdk-harness"
# How long `vite preview` may take to print the address it listens on.
WEB_PREVIEW_TIMEOUT_SECS=30

web_start_preview() {
  local log="$ARTIFACTS_DIR/vite-preview.log" i
  # Vite takes the next free port when the default is in use, and prints the
  # address it ended up with.
  (cd "$WEB_HARNESS_DIR" && NO_COLOR=1 "$REPO_ROOT/js/node_modules/.bin/vite" preview --host localhost) \
    >"$log" 2>&1 &
  WEB_PREVIEW_PID=$!
  e2e_on_exit 'kill "$WEB_PREVIEW_PID" 2>/dev/null'
  for ((i = 0; i < WEB_PREVIEW_TIMEOUT_SECS * 10; i++)); do
    HARNESS_URL="$(grep -Eom1 'http://localhost:[0-9]+/?' "$log" || true)"
    [[ -z "$HARNESS_URL" ]] || return 0
    if ! kill -0 "$WEB_PREVIEW_PID" 2>/dev/null; then
      cat "$log" >&2
      echo "vite preview exited before it listened" >&2
      exit 1
    fi
    sleep 0.1
  done
  cat "$log" >&2
  echo "vite preview did not report an address within ${WEB_PREVIEW_TIMEOUT_SECS}s" >&2
  exit 1
}

provision() {
  if [[ ! -f "$WEB_HARNESS_DIR/dist/index.html" ]]; then
    echo "No web harness in $WEB_HARNESS_DIR/dist. Build it first: just build-e2e-web" >&2
    exit 1
  fi
  if [[ ! -d "$REPO_ROOT/js/node_modules/playwright" ]]; then
    echo "playwright is not installed; run 'pnpm --dir js install'" >&2
    exit 1
  fi
  web_start_preview
  echo "Serving the web harness at $HARNESS_URL"
  SDK_LOG_FILE="$ARTIFACTS_DIR/browser-console.log"
  export HARNESS_URL SDK_LOG_FILE
}
