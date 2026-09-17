# shellcheck shell=bash
#
# Turns a module shape into the environment devimint and fedimintd read to decide which modules a
# federation runs. Meant to be sourced, not run, with the shape in $1:
#
#   . scripts/devimint-shape.sh [v1|v2|mixed]
#
# Used by scripts/run-sdk-integration-tests.sh and scripts/run-sdk-examples.sh.

shape="${1:-v1}"

# fedimintd decides the module set from these; devimint only passes the
# environment through. The v1 modules default off and the v2 modules default on.
# Every variable is spelled out as 0 or 1 rather than left to a default, because
# fedimintd and devimint disagree about what an unrecognised value means:
# fedimintd's module servers read it through is_env_var_set_opt and fall back to
# their own default, while devimint's own supports_* helpers read it through
# is_env_var_set and assume "on". A typo would give a federation of one shape and
# a harness expecting another.
case "$shape" in
  v1)
    export FM_ENABLE_MODULE_MINT=1 FM_ENABLE_MODULE_WALLET=1 FM_ENABLE_MODULE_LNV1=1
    export FM_ENABLE_MODULE_MINTV2=0 FM_ENABLE_MODULE_WALLETV2=0 FM_ENABLE_MODULE_LNV2=0
    ;;
  v2)
    export FM_ENABLE_MODULE_MINT=0 FM_ENABLE_MODULE_WALLET=0 FM_ENABLE_MODULE_LNV1=0
    export FM_ENABLE_MODULE_MINTV2=1 FM_ENABLE_MODULE_WALLETV2=1 FM_ENABLE_MODULE_LNV2=1
    ;;
  mixed)
    # Mix the lightning generations only. Mixing mint or wallet generations
    # instead breaks devimint's own gateway peg-in ("Polling gateway pegin
    # claim failed"), whereas ln + lnv2 side by side is the configuration
    # fedimint's own wasm test runs.
    export FM_ENABLE_MODULE_MINT=1 FM_ENABLE_MODULE_WALLET=1 FM_ENABLE_MODULE_LNV1=1
    export FM_ENABLE_MODULE_MINTV2=0 FM_ENABLE_MODULE_WALLETV2=0 FM_ENABLE_MODULE_LNV2=1
    ;;
  *)
    echo "usage: . devimint-shape.sh [v1|v2|mixed]" >&2
    exit 2
    ;;
esac
export FM_SDK_SHAPE="$shape"

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
