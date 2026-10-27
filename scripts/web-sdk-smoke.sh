#!/usr/bin/env bash
#
# Installs the packed `@fedimint/web-sdk` tarball into a fresh application, builds it with Vite and
# opens the SDK in headless Chromium.
#
#   scripts/web-sdk-smoke.sh <tarball>
#
# Before the package is published, the release has to prove that the tarball works, not the
# workspace it was packed from (scripts/web-sdk-pack.sh checks its contents, this script runs
# them). The script
#
# 1. stages an application in a temporary directory outside the repository, so that nothing
#    resolves from the JavaScript workspace's `node_modules`: scripts/web-sdk-smoke/index.html and
#    main.js, which imports `openSdk` from `@fedimint/web-sdk`, and a `package.json` of its own,
# 2. runs `npm install <tarball>` in it, as a user would, so the package's dependencies come from
#    the npm registry,
# 3. builds it with `vite build`, which needs no configuration, and serves the build with
#    `vite preview` on `localhost`, a secure context, which the SDK's storage needs, and
# 4. opens the page with scripts/web-sdk-smoke/driver.mjs. The page opens a session over a storage
#    name nobody else uses, reads the 12 word mnemonic, checks that no federation is joined and
#    closes the session.
#
# Vite and Playwright are the workspace's own, from `js/node_modules` (run `pnpm --dir js install`
# first), so they are the versions the lockfile fixes. Playwright's version matches the Chromium
# that the `.#wasm` Nix shell provides through PLAYWRIGHT_BROWSERS_PATH, so run the script inside
# that shell.
#
# The run passes when the page reports success. Otherwise the script prints what the page
# reported, its console output and its uncaught errors, and exits 1. The preview server is stopped
# and the application removed however the script ends.
#
# SMOKE_TIMEOUT in the environment is how many seconds the page may take to report, 120 when it is
# not set: the module is 12 MB, and a slow machine needs time to compile it.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SMOKE_DIR="$ROOT/scripts/web-sdk-smoke"
JS_DIR="$ROOT/js"
VITE="$JS_DIR/node_modules/.bin/vite"
SMOKE_TIMEOUT="${SMOKE_TIMEOUT:-120}"
# How long `vite preview` may take to print the address it listens on, in seconds.
SERVER_TIMEOUT=30

TMP=""
SERVER_PID=""

die() {
    echo "$*" >&2
    exit 1
}

usage() {
    sed -n '/^#   scripts/p' "${BASH_SOURCE[0]}" | sed 's/^#   /usage: /' >&2
    exit 2
}

# Stops the background process $1, waiting at most 30 seconds for it to end, and collects it.
stop_process() {
    local pid="$1" i
    kill "$pid" 2>/dev/null || true
    for ((i = 0; i < 300; i++)); do
        kill -0 "$pid" 2>/dev/null || break
        sleep 0.1
    done
    kill -9 "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
}

# Stops what the script started and removes its temporary files.
cleanup() {
    [[ -z "$SERVER_PID" ]] || stop_process "$SERVER_PID"
    [[ -z "$TMP" ]] || rm -rf "$TMP"
}

# Runs the command given as arguments with its output in $TMP/<name>.log, where $1 is the name,
# and prints that log when the command fails.
run_logged() {
    local name="$1"
    shift
    if ! "$@" >"$TMP/$name.log" 2>&1; then
        cat "$TMP/$name.log" >&2
        die "the smoke test failed: '$*' did not succeed; its output is above"
    fi
}

# Starts `vite preview` on the build in $TMP/app and leaves its address in $url. Vite takes the
# next free port when the default one is in use, and prints the address it ended up with.
start_preview() {
    local log="$TMP/preview.log" i
    NO_COLOR=1 "$VITE" preview "$TMP/app" --host localhost >"$log" 2>&1 &
    SERVER_PID=$!
    for ((i = 0; i < SERVER_TIMEOUT * 10; i++)); do
        url="$(grep -Eom1 'http://localhost:[0-9]+/?' "$log" || true)"
        [[ -z "$url" ]] || return 0
        if ! kill -0 "$SERVER_PID" 2>/dev/null; then
            cat "$log" >&2
            die "vite preview exited before it listened; its output is above"
        fi
        sleep 0.1
    done
    cat "$log" >&2
    die "vite preview did not report an address within $SERVER_TIMEOUT seconds"
}

[[ $# -eq 1 && "$1" != -* ]] || usage
[[ -f "$1" ]] || die "$1 does not exist; pack the tarball with scripts/web-sdk-pack.sh first"
tarball="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"

[[ -x "$VITE" ]] || die "$VITE is missing; run 'pnpm --dir js install' first"
[[ -d "$JS_DIR/node_modules/playwright" ]] ||
    die "playwright is not installed in js/node_modules; run 'pnpm --dir js install' first"
command -v node >/dev/null || die "node is not on PATH; run this inside the .#wasm shell"
command -v npm >/dev/null || die "npm is not on PATH; run this inside the .#wasm shell"
[[ -n "${PLAYWRIGHT_BROWSERS_PATH:-}" ]] ||
    die "PLAYWRIGHT_BROWSERS_PATH is not set; run this inside the .#wasm shell"

TMP="$(mktemp -d)"
trap cleanup EXIT
trap 'exit 1' INT TERM

echo "staging the application"
mkdir "$TMP/app"
cp "$SMOKE_DIR/index.html" "$SMOKE_DIR/main.js" "$TMP/app/"
echo '{ "name": "web-sdk-smoke-app", "private": true, "type": "module" }' \
    >"$TMP/app/package.json"

echo "installing $(basename "$tarball") with npm"
run_logged install npm --prefix "$TMP/app" install --no-audit --no-fund "$tarball"
[[ -f "$TMP/app/node_modules/@fedimint/web-sdk/package.json" ]] ||
    die "npm did not install @fedimint/web-sdk"

echo "building the application with vite"
run_logged build "$VITE" build "$TMP/app"

echo "serving the build"
url=""
start_preview

echo "opening $url in headless Chromium (up to $SMOKE_TIMEOUT seconds)"
if ! node "$SMOKE_DIR/driver.mjs" "$JS_DIR" "$url" "$SMOKE_TIMEOUT"; then
    die "the smoke test failed: the page did not report success; what it reported is above"
fi
echo "the application runs with the installed package"
