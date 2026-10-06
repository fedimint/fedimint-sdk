# shellcheck shell=bash
#
# Turns a module shape into the FM_ENABLE_MODULE_* flags fedimintd reads to decide which modules a
# federation runs, and records the shape as FM_SDK_SHAPE. Meant to be sourced, not run, with the
# shape in $1:
#
#   . scripts/devimint-modules.sh [v1|v2|mixed]
#
# Only the module set lives here, so that every harness that stands up devimint agrees on what a
# shape means: scripts/devimint-shape.sh adds the Rust harness's own settings on top (a single
# guardian, no Iroh), while scripts/setup_test_shell.sh (the wasm and Android suites) keeps
# devimint's defaults for everything else.

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
    echo "usage: . devimint-modules.sh [v1|v2|mixed] (got '$shape')" >&2
    exit 2
    ;;
esac
export FM_SDK_SHAPE="$shape"
