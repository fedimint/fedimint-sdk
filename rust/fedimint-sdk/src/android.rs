//! The Android platform wiring that a Kotlin host cannot do for a Rust
//! library, and that nothing else in an ordinary app would do either.
//!
//! Two things live here. Logging, in the `logcat` submodule: a `tracing`
//! subscriber and panic hook writing to logcat, started by
//! [`init_logging`]. And the `ndk_context` publish below, which hands the
//! platform's `JavaVM` and `Context` to the dependencies that expect to find
//! them.
//!
//! # The `ndk_context` publish
//!
//! Android gives apps no readable `/etc/resolv.conf`, so a Rust DNS stack has
//! to ask the framework for the active network's resolvers over JNI:
//! `Context.getSystemService("connectivity")`, then `getActiveNetwork`,
//! `getLinkProperties` and `getDnsServers`. `hickory-resolver` does exactly
//! that, and it finds the `JavaVM` and `Context` it needs to make those calls
//! in [`ndk_context`] — a process-global slot holding the two pointers, so a
//! crate buried in a dependency tree can reach the JVM without every API
//! between here and there passing it down.
//!
//! That slot is normally filled by `ndk-glue` or `android-activity`, which is
//! to say: by the crates used when *Rust owns the app*, as in a
//! `NativeActivity`. This SDK is the opposite arrangement — Kotlin owns the
//! Activity and loads a `cdylib` through `System.loadLibrary` — so nothing
//! ever filled it, and [`ndk_context::android_context`] is an `expect` on an
//! empty `Option`. Reaching it was fatal rather than merely broken, because
//! the release profile sets `panic = "abort"`: the panic took the whole host
//! app down with a `SIGABRT` and an `android context was not initialized`
//! abort message, from a `DefaultDispatch` worker thread, with no Rust error
//! ever reaching the caller.
//!
//! The path that got there in practice was a Lightning receive:
//! `select_available_gateway` probes *every* gateway a federation announces,
//! in parallel, to find out which are online. A federation running a gateway
//! that registers itself twice — once under `http`, once under `iroh`, which
//! is what devimint does — puts an `iroh://` URL in that list, dialing it
//! builds an iroh endpoint, and an iroh endpoint builds a DNS resolver. The
//! reachable `http` gateway would have won the subsequent ranking; the
//! process never survived the survey to get there.
//!
//! Guardians are the same story through a different door: `connect_guardian`
//! and `connect_gateway` share one scheme-to-connector map, so an `iroh://`
//! URL in an invite code would abort at join, before any gateway exists.
//! Filling the slot once, here, covers both — and every future dependency
//! that wants the platform's DNS configuration — rather than teaching one
//! call site to avoid one transport.
//!
//! # Where the publish happens
//!
//! `JNI_OnLoad` is the one hook that is handed the `JavaVM`: ART's library
//! loader looks it up and calls it when `System.loadLibrary` maps this
//! library. There is no way to express "pass me an
//! `android.content.Context`" through UniFFI's type system, so this is the
//! only entry point that reaches the VM at all. It records the VM and does
//! nothing else with it.
//!
//! **uniffi's bindings alone would never call it.** UniFFI's Kotlin loads this
//! library through JNA's `Native.register`, and JNA tries a plain `dlopen`
//! first, falling back to `System.loadLibrary` only if that fails. For a
//! library shipped in the APK's `jniLibs` the `dlopen` succeeds, so ART's
//! loader is never involved and `JNI_OnLoad` is never called. That was
//! confirmed on a device: the library loaded and served SDK calls, `nm -D`
//! showed `JNI_OnLoad` exported, and logcat showed ART's loader handling only
//! JNA's own `libjnidispatch.so`.
//!
//! So `scripts/generate-kotlin-bindings.sh` patches the generated loader to
//! call `System.loadLibrary` ahead of each `Native.register`. ART then loads
//! the library and calls this, and JNA's `dlopen` that follows finds the same
//! copy already loaded — also confirmed on a device: one copy, and
//! `JNI_OnLoad` runs. Every app embedding the SDK gets this without
//! doing anything. The script fails, rather than skipping the patch, if a
//! uniffi upgrade changes the lines it anchors on, because without it this
//! function silently never runs.
//!
//! The VM is not enough on its own: `ndk_context` also wants a `Context`,
//! which is fetched from Rust through `ActivityThread.currentApplication()`.
//! That is null until the process's `Application` is in place, and the
//! library can be loaded before then — by any synchronous binding, such as
//! `InviteCode.parse`, called from an `Application` field initializer or
//! `attachBaseContext`. So the publish is not done at load time but by
//! [`publish_context`], at the start of `create_fedimint_sdk`: every path
//! that reads the platform's DNS configuration goes through an SDK instance,
//! and by the time a host creates one its `Application` exists. A publish
//! that still fails is retried on the next `create_fedimint_sdk`, and only a
//! successful one is final.

// The crate's one exception to `deny(unsafe_code)` (see `lib.rs`), covering
// this module and `logcat` below it. Everything unsafe in either is FFI:
// declaring C functions from liblog and libc, trusting the `JavaVM` pointer
// the runtime hands `JNI_OnLoad`, and publishing that pointer with the
// application `Context` into `ndk_context`'s global slot. None of it has a
// safe formulation — `ndk-context` exists precisely to hold raw pointers — and
// all of it is confined to these two files.
#![allow(unsafe_code)]

mod logcat;

use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Mutex, PoisonError};

use jni::{JavaVM, jni_sig, jni_str};
use tracing::{info, warn};

/// Starts routing logs to logcat. See the `logcat` module for what that
/// installs and how to change its filter on a device.
///
/// Called from [`JNI_OnLoad`], the earliest point there is, and again at the
/// start of `create_fedimint_sdk`, the first call a host is certain to make.
/// Idempotent, so both may run, in either order.
///
/// Deliberately not dependent on `JNI_OnLoad` alone. The generated bindings
/// load the library through ART so that it runs, but that load is best
/// effort: if it fails, JNA's own `dlopen` loads the library instead and
/// `JNI_OnLoad` never runs — the one situation in which logs matter most.
/// Logging needs nothing from the JVM, only `liblog`, so unlike the
/// `ndk_context` publish below it can still start from an ordinary SDK call.
pub(crate) fn init_logging() {
    logcat::init();
}

/// The `JavaVM` [`JNI_OnLoad`] was handed, or null if it never ran.
static VM: AtomicPtr<jni::sys::JavaVM> = AtomicPtr::new(ptr::null_mut());

/// Whether the publish has succeeded.
///
/// [`ndk_context::initialize_android_context`] asserts that the slot was
/// empty, so a second call aborts the process. The lock is held across the
/// whole attempt, so concurrent `create_fedimint_sdk` calls cannot both find
/// it unset and both publish; and it is set only on success, so a publish
/// attempted too early is retried by the next call rather than given up on.
static PUBLISHED: Mutex<bool> = Mutex::new(false);

/// Called by the Android runtime when this library is loaded.
///
/// Returns the JNI version this library needs of its host. The return value
/// is not decoration: a version the runtime does not recognise makes
/// `System.loadLibrary` fail outright, so it is the plain 1.6 constant rather
/// than anything derived.
///
/// **This function must not panic.** `panic = "abort"` is on in release, so a
/// panic here would abort during `System.loadLibrary` — on every launch, on
/// every code path that touches the SDK. It only records the VM for
/// [`publish_context`], which does the part that can fail.
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "system" fn JNI_OnLoad(
    vm: *mut jni::sys::JavaVM,
    _reserved: *mut c_void,
) -> jni::sys::jint {
    // First, so that everything below — and everything the host does next —
    // is logged. When this runs it is the earliest point logging can start.
    init_logging();

    // Logged, because whether this runs at all is a property of how the host
    // loaded the library — `System.loadLibrary` calls `JNI_OnLoad`, a bare
    // `dlopen` does not, and JNA reaches for `dlopen` first — and without it
    // there is nothing for `publish_context` to publish.
    info!("JNI_OnLoad: recorded the JavaVM");
    VM.store(vm, Ordering::Release);
    jni::sys::JNI_VERSION_1_6
}

/// Hands the `JavaVM` and this process's `Application` to [`ndk_context`],
/// unless that has already been done.
///
/// Called at the start of `create_fedimint_sdk`; see the module docs for why
/// there and not at load time. Must not panic, for the same reason as
/// [`JNI_OnLoad`]: every failure is logged and leaves the slot empty for the
/// next call to try again.
pub(crate) fn publish_context() {
    let vm = VM.load(Ordering::Acquire);
    if vm.is_null() {
        warn!(
            "JNI_OnLoad never ran, so there is no JavaVM to publish; \
             code that reads the platform DNS configuration will abort"
        );
        return;
    }

    // `publish` does not panic, so the lock cannot really be poisoned; if it
    // somehow were, the flag inside is still accurate.
    let mut published = PUBLISHED.lock().unwrap_or_else(PoisonError::into_inner);
    if *published {
        return;
    }
    match publish(vm) {
        Ok(()) => {
            *published = true;
            info!("published the Android JavaVM and Context for ndk-context");
        }
        // Deliberately not fatal: see the note on panicking above. A miss
        // leaves anything that reads the platform's DNS configuration
        // aborting, which is bad but is not made better by also failing SDK
        // creation for hosts that never reach an `iroh://` URL.
        Err(err) => warn!(
            %err,
            "could not publish the Android Context; will retry on the next SDK creation, \
             and until then code that reads the platform DNS configuration will abort"
        ),
    }
}

/// Fetches this process's `Application` and hands it, with the `JavaVM`, to
/// [`ndk_context`].
fn publish(vm: *mut jni::sys::JavaVM) -> Result<(), jni::errors::Error> {
    // SAFETY: `vm` is the pointer the runtime handed `JNI_OnLoad`, and that VM
    // outlives every thread in the process — including whichever thread later
    // reads it back out of `ndk_context`. `JavaVM` here is a plain wrapper
    // around the pointer with no `Drop`, so this borrows rather than takes
    // ownership of anything.
    let handle = unsafe { JavaVM::from_raw(vm) };

    // Reuses the calling thread's attachment if it is a Java thread, and
    // attaches it otherwise. Exceptions thrown inside the closure are caught
    // by `attach_current_thread` and returned as `Error::JavaException`, so no
    // exception is left pending on the host's thread when a lookup fails.
    let context = handle.attach_current_thread(|env| {
        // `ActivityThread.currentApplication()` is the documented-by-usage
        // way to reach the process's own `Application` without being handed
        // one. It is hidden rather than public API, and has been present and
        // stable since the framework's earliest versions; the alternative is
        // requiring the host to call an initializer, which is the coupling
        // this module exists to avoid. (`AppGlobals.getInitialApplication()`
        // is no fallback: AOSP implements it as this same call.)
        let class = env.find_class(jni_str!("android/app/ActivityThread"))?;
        let app = env
            .call_static_method(
                &class,
                jni_str!("currentApplication"),
                jni_sig!("()Landroid/app/Application;"),
                &[],
            )?
            .l()?;

        // Null before the `Application` is in place, which is what the retry
        // in `publish_context` is for.
        if app.is_null() {
            return Err(jni::errors::Error::NullPtr(
                "no Application is in place for this process yet",
            ));
        }

        // A global reference, not the local one: `ndk_context` stores a raw
        // pointer that is dereferenced much later and from other threads,
        // and a local reference dies when this JNI frame returns.
        env.new_global_ref(&app)
    })?;

    // SAFETY: `PUBLISHED`, held by the caller and unset, makes this the only
    // call in the process, which is what `initialize_android_context`
    // requires — it asserts the slot was empty and would abort otherwise.
    //
    // `into_raw` deliberately leaks the global reference. `ndk_context` hands
    // out copies of the pointer for the rest of the process's life and has no
    // way to signal that the last reader is done, so there is no point at
    // which deleting it would be safe. It is the *application* context, which
    // lives as long as the process regardless, so the leak costs one
    // reference rather than keeping an Activity alive.
    unsafe {
        ndk_context::initialize_android_context(vm.cast(), context.into_raw().cast());
    }
    Ok(())
}
