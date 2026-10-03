//! The Apple platform wiring that a Swift host cannot do for a Rust library.
//!
//! One thing lives here: logging, in the `oslog` submodule — a `tracing`
//! subscriber and panic hook writing to Apple's unified logging, started by
//! [`init_logging`].
//!
//! It is much thinner than `crate::android`, because Apple needs nothing
//! published. Android's `ndk_context` slot exists because a Rust DNS stack
//! there has to reach the JVM to learn the resolvers; on Apple targets
//! `/etc/resolv.conf` is readable and nothing in the dependency tree expects
//! a platform handle to have been filled in. So there is no `publish_context`
//! counterpart, and none of the retry bookkeeping `android.rs` carries for it.
//!
//! Gated on `target_vendor = "apple"` rather than `target_os = "ios"`: the
//! XCFramework ships a `macos-arm64` slice precisely so `swift test` runs on
//! the host without a simulator, and an iOS-only gate would leave that slice
//! — the one CI exercises — without the code this module adds.

// The crate's second exception to `deny(unsafe_code)` (see `lib.rs` for the
// argument), covering this module and `oslog` below it. Everything unsafe in
// either is the `extern "C"` declaration of, and calls into, two libSystem
// functions and the two oslog_shim.c wrappers around `os_log_with_type`. No
// pointer is published anywhere, nothing is exported, and all of it is
// confined to these two files.
#![allow(unsafe_code)]

mod oslog;

/// Starts routing logs to os_log. See the `oslog` module for what that
/// installs and how to change its filter.
///
/// Called at the start of `create_fedimint_sdk`, the first call a host is
/// certain to make. Idempotent: every call after the first is a no-op.
///
/// Deliberately *not* also run from a load-time constructor
/// (`__mod_init_func`), the nearest Apple equivalent of `JNI_OnLoad`. That
/// would run before the host app's `main`, inside someone else's binary,
/// where a panic during installation aborts the app at launch on every code
/// path — and unlike Android, nothing here needs to run before the first
/// SDK call. Revisit only if a panic that precedes it is actually observed.
pub(crate) fn init_logging() {
    oslog::init();
}
