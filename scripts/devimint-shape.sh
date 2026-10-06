# shellcheck shell=bash
#
# Turns a module shape into the environment devimint and fedimintd read to decide which modules a
# federation runs. Meant to be sourced, not run, with the shape in $1:
#
#   . scripts/devimint-shape.sh [v1|v2|mixed]
#
# Used by scripts/run-sdk-integration-tests.sh and scripts/run-sdk-examples.sh.

# The module set itself is shared with scripts/setup_test_shell.sh, so every harness agrees on what
# a shape means; this file adds the Rust harness's own settings on top of it.
# shellcheck source=scripts/devimint-modules.sh
. "$(dirname "${BASH_SOURCE[0]}")/devimint-modules.sh" "${1:-v1}"

# The federation is configured with the WebSocket API only, the same choice
# fedimint's own wasm test makes. FM_ENABLE_IROH already resolves to false under
# devimint, because fedimintd defaults it to !is_running_in_test_env(); this
# second switch is what also suppresses the transitional Iroh 1.0 endpoint,
# which is on by default in every environment.
export FM_IROH_NEXT_ENABLE=false

# One guardian rather than devimint's default of four (devimint/src/cli.rs:53).
# DKG and every per-guardian admin call scale with this, and upstream runs
# `devimint -n 1` for its own CLI tests. Override to 4 to match the JS suite's
# federation.
export FM_FED_SIZE="${FM_FED_SIZE:-1}"
