//! Integration test harness stub.
//!
//! This file currently defines no real tests because `fedimint-sdk` is a
//! public API skeleton: every effectful or fallible method body is
//! `unimplemented!()` (see [`fedimint-sdk#344`], the RFC this crate
//! implements). Tests land here as each capability facade — ecash,
//! lightning, onchain, meta, activity — grows a real implementation, rather
//! than all at once.
//!
//! # Integration over mocking
//!
//! This crate deliberately tests against a real federation instead of
//! mocking `fedimint-client`. The client's behavior — consensus rounds,
//! peg-ins, gateway routing, module state machines — is not something a
//! hand-rolled mock can stand in for without silently drifting from the real
//! thing, and this SDK's whole purpose is to be a thin, faithful facade over
//! that client. A unit test that only calls a mock reduced to fixture
//! assertions long before it caught a fedimint-side behavior change, so
//! correctness here is judged against a live federation and gateway, not a
//! simulated one.
//!
//! # How the harness will work
//!
//! The harness runs against `devimint`, which this repository already
//! consumes for its JS test suite via the `fedimint` flake input (see
//! `flake.nix`); the `wasm-tests` devShell puts `devimint` on `PATH` by
//! listing `fedimint.packages.${system}.devimint` among its
//! `nativeBuildInputs`. The Rust harness is expected to follow the same
//! pattern already established by `scripts/setup_test_shell.sh`: a
//! `devimint wasm-test-setup --exec ...` wrapper that stands up a
//! federation and a gateway, exports the faucet and port environment the
//! test process reads (for example `FM_FAUCET_PORT`/`FM_PORT_FAUCET`), runs
//! the test binary, and tears the federation down afterward.
//!
//! # Module generation matters for this SDK specifically
//!
//! Today's fedimint defaults a federation to its v2 mint, wallet, and
//! lightning modules. `scripts/setup_test_shell.sh` opts back into the v1
//! generation explicitly, because this SDK speaks v1, by setting:
//!
//! - `FM_ENABLE_MODULE_MINT=1`
//! - `FM_ENABLE_MODULE_WALLET=1`
//! - `FM_ENABLE_MODULE_LNV1=1`
//! - `FM_ENABLE_MODULE_MINTV2=0`
//! - `FM_ENABLE_MODULE_WALLETV2=0`
//! - `FM_ENABLE_MODULE_LNV2=0`
//!
//! and switches the v2 modules off rather than leaving them alongside v1,
//! because a federation carrying both generations of a module breaks
//! devimint's own gateway peg-in.
//!
//! This SDK additionally enforces a stricter rule than devimint does: a
//! federation must be all-v1 or all-v2 across mint, wallet, and lightning,
//! and a federation mixing generations is rejected outright rather than
//! merely mishandled. Because that rule is part of this crate's contract,
//! not just a devimint quirk to work around, the eventual harness needs to
//! stand up three shapes of federation, not one:
//!
//! - an all-v1 federation (the `scripts/setup_test_shell.sh` configuration
//!   above), exercising the generation this SDK targets today;
//! - an all-v2 federation (the module defaults, with the v1 variables above
//!   flipped), exercising the other accepted generation; and
//! - a deliberately mixed federation, to cover the rejection path and
//!   confirm the SDK refuses it rather than silently misrouting operations
//!   across generations.
//!
//! # Not wired into CI yet
//!
//! Nothing in this file runs today: the single test below is `#[ignore]`d,
//! and no workflow invokes it. The workflow will start running these tests
//! once the first capability facade has a real implementation to exercise.
//!
//! [`fedimint-sdk#344`]: https://github.com/fedimint/fedimint-sdk/issues/344

#[test]
#[ignore = "devimint harness lands with the first implemented facade"]
fn devimint_harness_placeholder() {
    // Intentionally empty: see the module documentation above.
}
