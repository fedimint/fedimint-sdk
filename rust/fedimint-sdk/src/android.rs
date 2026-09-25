//! The Android platform wiring that a Kotlin host cannot do for a Rust
//! library, and that nothing else in an ordinary app would do either.
//!
//! Logging, in the `logcat` submodule: a `tracing` subscriber and panic hook
//! writing to logcat, started by [`init_logging`].

// The crate's one exception to `deny(unsafe_code)` (see `lib.rs`), covering
// this module and `logcat` below it. Everything unsafe in either is FFI:
// declaring C functions from liblog and libc and calling them. None of it has
// a safe formulation, and all of it is confined to these two files.
#![allow(unsafe_code)]

mod logcat;

/// Starts routing logs to logcat. See the `logcat` module for what that
/// installs and how to change its filter on a device.
///
/// Called at the start of `create_fedimint_sdk`, the first call a host is
/// certain to make. Idempotent, so repeated SDK creation is harmless.
pub(crate) fn init_logging() {
    logcat::init();
}
