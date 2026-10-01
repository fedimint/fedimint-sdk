//! Routes this crate's logging — and every dependency's — into Apple's
//! unified logging (os_log).
//!
//! The reasoning is `android::logcat`'s, unchanged: a library should leave the
//! choice of `tracing` subscriber to its host, but a Swift host has no way to
//! install a Rust subscriber at all, so without one every event from this
//! crate, fedimint-client, the modules and the transports is discarded. Worse,
//! the default panic hook writes to stderr, which on a device goes nowhere and
//! is absent from the `.ips` crash report — so under the release profile's
//! `panic = "abort"`, a Rust panic on an iPhone left a crash report with an
//! `abort()` frame and no message, file or line. So on Apple targets the SDK
//! installs a subscriber and a panic hook itself, and steps aside if
//! something beat it to the subscriber.
//!
//! Everything lands under one subsystem and category, [`SUBSYSTEM`] /
//! [`CATEGORY`], so one predicate shows the whole of it:
//!
//! ```text
//! log stream --predicate 'subsystem == "org.fedimint.sdk"' --level debug
//! ```
//!
//! # Turning up the verbosity
//!
//! The filter is read once, when logging is installed, from the environment
//! variable `FEDIMINT_SDK_LOG`, in the same syntax as `RUST_LOG`: set it in
//! the Xcode scheme, with `xcrun simctl launch --setenv`, or in the shell for
//! `swift test`. Unset, empty, or set to something that does not parse, the
//! filter is [`DEFAULT_DIRECTIVES`] — and a value that does not parse is
//! reported rather than silently dropped. Either way the filter in force is
//! logged once at startup, as `logging to os_log filter=…`.
//!
//! There is deliberately no on-device knob beyond the environment, which on a
//! shipped app the user cannot set. Android's `debug.fedimint_sdk.log`
//! property is the opposite trade, made because `adb` needs no rebuild.
//!
//! # What must never reach this
//!
//! The contract in `android::logcat`'s module docs ("What must never reach
//! this") applies here unchanged; it is not restated so the two cannot drift.
//! os_log adds one layer on top: lines at `info` and above are written
//! `%{public}`, and lines below it `%{private}`, so the more detailed levels
//! show as `<private>` in a sysdiagnose unless the device carries a profile
//! that reveals them. That is defence in depth, not a licence: the guarantee
//! is still that nothing at `info` or above formats a secret.

use std::ffi::{CStr, CString, c_char, c_void};
use std::io;
use std::panic::{self, PanicHookInfo};
use std::sync::{Once, OnceLock};

use tracing::{Level, Metadata};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;

use crate::logging::chunks;

/// An `os_log_t`: an opaque, reference-counted handle that libSystem hands
/// out and this module never frees.
#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
struct OsLog(*mut c_void);

// SAFETY: an `os_log_t` is immutable once created and documented as safe to
// use from any thread; os_log's whole point is concurrent writers. The handle
// in `handle()` is created once and never released, so it outlives every use.
unsafe impl Send for OsLog {}
unsafe impl Sync for OsLog {}

// libSystem, which rustc links on every Apple target without a `#[link]`.
unsafe extern "C" {
    fn os_log_create(subsystem: *const c_char, category: *const c_char) -> OsLog;
    fn os_log_type_enabled(log: OsLog, kind: u8) -> bool;
}

// oslog_shim.c, compiled by build.rs on Apple targets. `os_log_with_type`
// is a C macro, so it cannot be declared here directly; see that file.
unsafe extern "C" {
    fn fedimint_os_log_public(log: OsLog, kind: u8, message: *const c_char);
    fn fedimint_os_log_private(log: OsLog, kind: u8, message: *const c_char);
}

/// The subsystem every line from Rust is logged under.
const SUBSYSTEM: &CStr = c"org.fedimint.sdk";

/// The category every line from Rust is logged under. One category rather
/// than one per `tracing` target, so a single predicate shows all of it; the
/// target is kept in the message, where it still names the crate and module.
const CATEGORY: &CStr = c"sdk";

/// The environment variable the filter is read from. See the module docs.
const FILTER_VARIABLE: &str = "FEDIMINT_SDK_LOG";

/// The filter used when [`FILTER_VARIABLE`] is unset or does not parse.
///
/// The same as Android's; `android::logcat`'s `DEFAULT_DIRECTIVES` explains
/// why it needs both the `fm` and `fedimint` prefixes.
const DEFAULT_DIRECTIVES: &str = "warn,fm=info,fedimint=info";

/// The largest payload written as one os_log entry.
///
/// Unified logging truncates a formatted entry at about 1024 bytes, marking
/// the cut with `…` and keeping nothing past it. That is a quarter of what
/// logcat allows, and an error chain or a serialized structure easily
/// exceeds it — and the tail of an error chain is usually its root cause. So
/// longer messages are split into consecutive entries, with headroom left
/// under the limit for os_log's own framing.
const MAX_ENTRY_BYTES: usize = 1000;

/// An `os_log_type_t`, as `<os/log.h>` numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum LogType {
    Default = 0x00,
    Debug = 0x02,
    Error = 0x10,
    Fault = 0x11,
}

impl LogType {
    /// The os_log type a `tracing` level is written at.
    ///
    /// **Deliberately one tier up from the names**, and not to be "corrected"
    /// to `INFO → OS_LOG_TYPE_INFO`. os_log keeps its `INFO` type in memory
    /// only, persisting it just when a later fault or error in the same
    /// activity flushes it. `info` is the default filter's own ceiling — the
    /// level fedimint's useful lines are at — so under the one-to-one mapping
    /// none of them reach disk, and `log show` after a failing run comes back
    /// empty: exactly the silence this module exists to end. `DEFAULT` is
    /// persisted, so `info` goes there, and each level above moves with it.
    /// `debug` and `trace` share `DEBUG`, which is memory-only and off unless
    /// streamed, matching how rarely they are enabled.
    fn from_level(level: &Level) -> Self {
        // Compared rather than matched: `Level`'s variants are associated
        // constants on an opaque struct, not an enum.
        if *level == Level::ERROR {
            LogType::Fault
        } else if *level == Level::WARN {
            LogType::Error
        } else if *level == Level::INFO {
            LogType::Default
        } else {
            LogType::Debug
        }
    }

    /// Whether a line at this type is written `%{public}`; see the module
    /// docs. Everything persisted is public, and only the `debug`/`trace`
    /// tier is private.
    fn is_public(self) -> bool {
        self != LogType::Debug
    }
}

/// The one `os_log_t` this module writes through, created on first use.
///
/// Cached rather than created per event: `os_log_create` allocates, and an
/// event can arrive from any runtime worker at any rate.
fn handle() -> OsLog {
    static HANDLE: OnceLock<OsLog> = OnceLock::new();
    // SAFETY: both arguments are NUL-terminated strings with static lifetime.
    // `os_log_create` never returns null; on failure it returns the shared
    // default log, which is still a valid handle.
    *HANDLE.get_or_init(|| unsafe { os_log_create(SUBSYSTEM.as_ptr(), CATEGORY.as_ptr()) })
}

/// Writes `message` to os_log under [`SUBSYSTEM`] and [`CATEGORY`].
///
/// Never panics, and never drops a message it could deliver: interior NULs
/// (which a C string cannot carry) are replaced rather than causing the line
/// to be skipped, and anything over [`MAX_ENTRY_BYTES`] is split across
/// entries on character boundaries. Callable before logging is installed and
/// from a panic hook, since it touches nothing but libSystem.
fn write(kind: LogType, message: &str) {
    let log = handle();
    // SAFETY: `log` is a valid handle (see `handle`).
    if !unsafe { os_log_type_enabled(log, kind as u8) } {
        return;
    }
    for chunk in chunks(message.trim_end_matches('\n'), MAX_ENTRY_BYTES) {
        let text = CString::new(chunk.replace('\0', "\u{FFFD}")).unwrap_or_default();
        // SAFETY: `log` is valid, `text` is a NUL-terminated string that
        // outlives the call, and os_log copies the argument into its own
        // buffer before returning rather than retaining the pointer.
        unsafe {
            if kind.is_public() {
                fedimint_os_log_public(log, kind as u8, text.as_ptr());
            } else {
                fedimint_os_log_private(log, kind as u8, text.as_ptr());
            }
        }
    }
}

/// Installs logging, once per process. Every call after the first is a no-op.
///
/// Installs, in order:
///
/// 1. A panic hook that writes the panic's message, location and thread to
///    os_log before chaining to the previous hook. **This does not prevent
///    the abort**: under `panic = "abort"` the process still dies with
///    `SIGABRT`. What changes is that the crash now comes with a persisted
///    line saying where and why.
/// 2. A `tracing` subscriber writing to os_log, filtered as the module docs
///    describe — unless one is already installed, in which case it is left
///    alone and nothing else here is touched either.
/// 3. A bridge from the `log` crate into that subscriber, so dependencies
///    that log through `log` rather than `tracing` are not lost.
pub(crate) fn init() {
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(install);
}

fn install() {
    install_panic_hook();

    let Filter {
        filter,
        effective,
        rejected,
    } = Filter::from_configured(configured_directives());

    let subscriber = tracing_subscriber::registry().with(filter).with(
        tracing_subscriber::fmt::layer()
            .with_writer(OsLogMakeWriter)
            // os_log renders neither ANSI colour nor a second timestamp, and
            // records the type itself, so all three would only be noise.
            .with_ansi(false)
            .without_time()
            .with_level(false)
            .with_target(true),
    );

    if tracing::subscriber::set_global_default(subscriber).is_err() {
        // Only possible if something in this process installed one first —
        // a Rust host embedding the library, say. It is theirs to own, so
        // stop here rather than also replacing their `log` logger.
        write(
            LogType::Default,
            "a tracing subscriber was already installed; leaving it in place",
        );
        return;
    }

    // After the subscriber, not before: converted `log` records dispatch
    // straight to it. The error means a `log` logger already exists, which,
    // like the subscriber above, is not ours to replace.
    let _ = tracing_log::LogTracer::init();

    if let Some((directives, err)) = rejected {
        tracing::warn!(
            directives = %directives,
            %err,
            "ignoring the log filter in {FILTER_VARIABLE}, which does not parse; \
             using the default ({DEFAULT_DIRECTIVES})"
        );
    }

    // Announced once, at `info` so the default filter lets it through: it
    // distinguishes "installed, and nothing to say at this level" from "not
    // installed", and names the filter in force.
    tracing::info!(filter = %effective, "logging to os_log");
}

/// The filter to install, and how it was arrived at.
struct Filter {
    filter: EnvFilter,
    /// The directive string actually in force, for the startup line.
    effective: String,
    /// A configured value that did not parse, with the parse error.
    rejected: Option<(String, String)>,
}

impl Filter {
    /// The filter for `configured` directives: those directives if they
    /// parse, and [`DEFAULT_DIRECTIVES`] otherwise.
    fn from_configured(configured: Option<String>) -> Self {
        let default = || EnvFilter::new(DEFAULT_DIRECTIVES);
        match configured {
            Some(directives) => match EnvFilter::try_new(&directives) {
                Ok(filter) => Self {
                    filter,
                    effective: directives,
                    rejected: None,
                },
                Err(err) => Self {
                    filter: default(),
                    effective: DEFAULT_DIRECTIVES.to_owned(),
                    rejected: Some((directives, err.to_string())),
                },
            },
            None => Self {
                filter: default(),
                effective: DEFAULT_DIRECTIVES.to_owned(),
                rejected: None,
            },
        }
    }
}

/// Reads [`FILTER_VARIABLE`], or `None` if it is unset, empty or not UTF-8.
fn configured_directives() -> Option<String> {
    let value = std::env::var(FILTER_VARIABLE).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// Writes the panic's message, location and thread to os_log, then runs the
/// previous hook.
///
/// Writes through [`write`] directly rather than through `tracing`: the panic
/// may have been raised inside the subscriber, or while it held a lock, and
/// the one thing this hook must not do is fail to report.
fn install_panic_hook() {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info: &PanicHookInfo<'_>| {
        let thread = std::thread::current();
        let thread = thread.name().unwrap_or("<unnamed>");
        let location = info.location().map_or_else(String::new, |location| {
            format!(
                " at {}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            )
        });
        let message = info
            .payload_as_str()
            .unwrap_or("<panic payload is not a string>");
        write(
            LogType::Fault,
            &format!("panic on thread '{thread}'{location}: {message}"),
        );
        previous(info);
    }));
}

/// Hands the `fmt` layer a fresh [`OsLogWriter`] per event, carrying that
/// event's os_log type.
#[derive(Debug)]
struct OsLogMakeWriter;

impl<'a> MakeWriter<'a> for OsLogMakeWriter {
    type Writer = OsLogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        OsLogWriter::new(LogType::Default)
    }

    fn make_writer_for(&'a self, meta: &Metadata<'_>) -> Self::Writer {
        OsLogWriter::new(LogType::from_level(meta.level()))
    }
}

/// Collects one formatted event and writes it as a single os_log entry when
/// dropped, for the same reason `android::logcat`'s writer does: the `fmt`
/// layer writes a line in several pieces, and one event should be one entry.
#[derive(Debug)]
struct OsLogWriter {
    kind: LogType,
    buffer: Vec<u8>,
}

impl OsLogWriter {
    fn new(kind: LogType) -> Self {
        Self {
            kind,
            buffer: Vec::new(),
        }
    }
}

impl io::Write for OsLogWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for OsLogWriter {
    fn drop(&mut self) {
        if !self.buffer.is_empty() {
            write(self.kind, &String::from_utf8_lossy(&self.buffer));
        }
    }
}

#[cfg(test)]
mod tests {
    use tracing::Level;

    use super::{DEFAULT_DIRECTIVES, Filter, LogType, write};

    #[test]
    fn levels_map_one_tier_up_so_info_persists() {
        assert_eq!(LogType::from_level(&Level::ERROR), LogType::Fault);
        assert_eq!(LogType::from_level(&Level::WARN), LogType::Error);
        // The one that matters: `OS_LOG_TYPE_INFO` is memory-only.
        assert_eq!(LogType::from_level(&Level::INFO), LogType::Default);
        assert_eq!(LogType::from_level(&Level::DEBUG), LogType::Debug);
        assert_eq!(LogType::from_level(&Level::TRACE), LogType::Debug);
    }

    #[test]
    fn only_the_debug_tier_is_private() {
        assert!(LogType::Fault.is_public());
        assert!(LogType::Error.is_public());
        assert!(LogType::Default.is_public());
        assert!(!LogType::Debug.is_public());
    }

    #[test]
    fn an_unset_filter_is_the_default() {
        let filter = Filter::from_configured(None);
        assert_eq!(filter.effective, DEFAULT_DIRECTIVES);
        assert!(filter.rejected.is_none());
    }

    #[test]
    fn a_valid_filter_is_used_as_given() {
        let filter = Filter::from_configured(Some("debug,iroh=warn".to_owned()));
        assert_eq!(filter.effective, "debug,iroh=warn");
        assert!(filter.rejected.is_none());
    }

    #[test]
    fn an_unparseable_filter_falls_back_and_is_reported() {
        let filter = Filter::from_configured(Some("fedimint=loud".to_owned()));
        assert_eq!(filter.effective, DEFAULT_DIRECTIVES);
        let (directives, _err) = filter.rejected.expect("the bad value is reported");
        assert_eq!(directives, "fedimint=loud");
    }

    #[test]
    fn writing_through_the_shim_does_not_crash() {
        // Exercises the real FFI path on the host: an entry over the size
        // limit, an interior NUL, and a `%` that must not be read as a format.
        let long = "é".repeat(1_500);
        write(LogType::Default, &long);
        write(LogType::Debug, "contains a \0 NUL and a %s %n format");
    }
}
