//! Routes this crate's logging — and every dependency's — into Android's
//! logcat.
//!
//! Nothing in this crate installs a `tracing` subscriber, and on most targets
//! that is correct: which subscriber a process runs, with what filter and
//! writing where, is the host application's decision, and a library that
//! installs a global one takes it away from them. Android is the exception
//! because the host is Kotlin, and a Kotlin app has no way to install a Rust
//! subscriber at all. Without one, every event is discarded — this crate's,
//! and the far larger volume from fedimint-client, the modules and the
//! transports underneath — which in practice meant that a failing run left
//! nothing in `adb logcat` from Rust but a tombstone. So on Android, and only
//! there, the SDK installs one itself, and steps aside if something beat it
//! to it.
//!
//! Everything lands under the single tag [`TAG`], so one filter shows the
//! whole of it:
//!
//! ```text
//! adb logcat -s fedimint-sdk
//! ```
//!
//! # Turning up the verbosity on a device
//!
//! The filter is read once, when logging is installed, from the system
//! property `debug.fedimint_sdk.log`, in the same syntax as `RUST_LOG`:
//!
//! ```text
//! adb shell setprop debug.fedimint_sdk.log 'debug,iroh=warn'
//! # then restart the app
//! ```
//!
//! `debug.*` properties are writable from `adb shell` without root, so this
//! needs no rebuild and no cooperation from the app. Unset, or set to
//! something that does not parse, the filter is [`DEFAULT_DIRECTIVES`] — and
//! a value that does not parse is reported rather than silently dropped.
//! Either way the filter in force is logged once at startup, as
//! `logging to logcat filter=…`, which is how to confirm a `setprop` took.
//!
//! # What must never reach this
//!
//! Logcat is readable by anything with `adb` access to the device. For a
//! wallet that is not an abstract concern: an ecash note in a log line is
//! spendable by whoever reads it, and the same goes for mnemonic words,
//! preimages and API secrets. The default filter keeps everything below
//! `info` out, which is where detailed state tends to live, but the
//! guarantee that matters is that nothing at `info` or above formats one of
//! those — in this crate, and in what it depends on.

use std::ffi::{CStr, CString, c_char, c_int};
use std::io;
use std::panic::{self, PanicHookInfo};
use std::sync::Once;

use tracing::{Level, Metadata};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;

// Android's own logging. Part of the NDK's stable surface, present on every
// device, and thread-safe, which the writer below relies on: events arrive
// from whichever runtime worker produced them.
#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(prio: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

// In libc on every API level. `__system_property_read_callback` (API 26+) is
// the newer interface and lifts the value-length limit, but a filter fits in
// `PROP_VALUE_MAX` comfortably and this one needs no callback plumbing.
unsafe extern "C" {
    fn __system_property_get(name: *const c_char, value: *mut c_char) -> c_int;
}

/// The logcat tag every line from Rust is written under.
///
/// One tag rather than one per `tracing` target, so that `adb logcat -s
/// fedimint-sdk` shows all of it. The target is kept in the message instead,
/// where it still identifies the crate and module each line came from.
const TAG: &CStr = c"fedimint-sdk";

/// The system property the filter is read from. See the module docs.
const FILTER_PROPERTY: &CStr = c"debug.fedimint_sdk.log";

/// The longest value a system property can hold, NUL included (`PROP_VALUE_MAX`
/// in `<sys/system_properties.h>`).
const PROP_VALUE_MAX: usize = 92;

/// The filter used when [`FILTER_PROPERTY`] is unset or does not parse.
///
/// `warn` for everything, `info` for fedimint's own code, which logs under
/// two families of target and needs a directive for each. `EnvFilter`
/// matches targets by prefix, so:
///
/// - `fm` covers fedimint-client and the module clients, which log under the
///   short targets fedimint-logging defines (`fm::client`, `fm::net::iroh`,
///   `fm::db`, …) — every one of them starts with `fm`, and no crate in this
///   build does, so the prefix catches nothing else.
/// - `fedimint` covers crates that log under their module path instead,
///   this one included (`fedimint_sdk::…`, `fedimint_rocksdb`).
///
/// Without `fm` the second alone looks sufficient and is not: it silently
/// holds fedimint-client to `warn`. The transports underneath (iroh,
/// hickory, quinn, rustls) are chatty at `info` and would scroll the lines
/// that matter out of logcat's ring buffer before anyone captured it; at
/// `warn` they still report what went wrong.
const DEFAULT_DIRECTIVES: &str = "warn,fm=info,fedimint=info";

/// The largest payload written as one logcat entry.
///
/// The kernel logger caps an entry at `LOGGER_ENTRY_MAX_PAYLOAD` (4068 bytes),
/// tag and headers included, and silently truncates past it. An error chain
/// or a serialized structure can exceed that, so longer messages are split
/// into consecutive entries rather than losing their tail — which, for an
/// error, is usually the root cause.
const MAX_ENTRY_BYTES: usize = 4000;

/// A logcat priority, as `<android/log.h>` numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Priority {
    Verbose = 2,
    Debug = 3,
    Info = 4,
    Warn = 5,
    Error = 6,
}

impl From<&Level> for Priority {
    fn from(level: &Level) -> Self {
        // Compared rather than matched: `Level`'s variants are associated
        // constants on an opaque struct, not an enum.
        if *level == Level::ERROR {
            Priority::Error
        } else if *level == Level::WARN {
            Priority::Warn
        } else if *level == Level::INFO {
            Priority::Info
        } else if *level == Level::DEBUG {
            Priority::Debug
        } else {
            Priority::Verbose
        }
    }
}

/// Writes `message` to logcat under [`TAG`].
///
/// Never panics, and never drops a message it could deliver: interior NULs
/// (which a C string cannot carry) are replaced rather than causing the line
/// to be skipped, and anything over [`MAX_ENTRY_BYTES`] is split across
/// entries on character boundaries. Callable before logging is installed and
/// from a panic hook, since it touches nothing but `liblog`.
pub(crate) fn write(priority: Priority, message: &str) {
    for chunk in chunks(message.trim_end_matches('\n'), MAX_ENTRY_BYTES) {
        let text = CString::new(chunk.replace('\0', "\u{FFFD}")).unwrap_or_default();
        // SAFETY: both pointers are to NUL-terminated strings that outlive
        // the call, and liblog copies out of them before returning rather
        // than retaining them.
        unsafe {
            __android_log_write(priority as c_int, TAG.as_ptr(), text.as_ptr());
        }
    }
}

/// Splits `text` into pieces of at most `max` bytes, each ending on a
/// character boundary.
fn chunks(text: &str, max: usize) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let mut end = rest.len().min(max);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        // Only reachable if `max` is smaller than the first character, which
        // the constant above rules out; taking that one character regardless
        // keeps a future caller from spinning forever on an empty chunk.
        if end == 0 {
            end = rest.chars().next().map_or(rest.len(), char::len_utf8);
        }
        let (head, tail) = rest.split_at(end);
        rest = tail;
        Some(head)
    })
}

/// Installs logging, once per process. Every call after the first is a no-op.
///
/// Installs, in order:
///
/// 1. A panic hook that writes the panic's message, location and thread to
///    logcat before chaining to the previous hook. With `panic = "abort"`
///    the only other trace of a panic is the tombstone's one-line abort
///    message, which carries no source location.
/// 2. A `tracing` subscriber writing to logcat, filtered as the module docs
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

    // `effective` is the directive string actually in force, kept alongside
    // the filter built from it so the startup line below can name it.
    let (filter, effective, rejected) = match configured_directives() {
        Some(directives) => match EnvFilter::try_new(&directives) {
            Ok(filter) => (filter, directives, None),
            Err(err) => (
                EnvFilter::new(DEFAULT_DIRECTIVES),
                DEFAULT_DIRECTIVES.to_owned(),
                Some((directives, err.to_string())),
            ),
        },
        None => (
            EnvFilter::new(DEFAULT_DIRECTIVES),
            DEFAULT_DIRECTIVES.to_owned(),
            None,
        ),
    };

    let subscriber = tracing_subscriber::registry().with(filter).with(
        tracing_subscriber::fmt::layer()
            .with_writer(Logcat)
            // Logcat renders neither ANSI colour nor a second timestamp, and
            // shows the priority itself, so all three would only be noise.
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
            Priority::Info,
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
            "ignoring the log filter in debug.fedimint_sdk.log, which does not parse; \
             using the default ({DEFAULT_DIRECTIVES})"
        );
    }

    // Announced once, at `info` so the default filter lets it through. Without
    // it, "installed, and fedimint had nothing to say at this level" and "not
    // installed" look identical in logcat — and a `setprop` that did not take
    // effect is indistinguishable from one that did. Naming the filter in
    // force answers both.
    tracing::info!(filter = %effective, "logging to logcat");
}

/// Reads [`FILTER_PROPERTY`], or `None` if it is unset or empty.
fn configured_directives() -> Option<String> {
    let mut value = [0u8; PROP_VALUE_MAX];
    // SAFETY: `value` is `PROP_VALUE_MAX` bytes, the most the call writes
    // (NUL included), and the name is a NUL-terminated string.
    let len = unsafe { __system_property_get(FILTER_PROPERTY.as_ptr(), value.as_mut_ptr().cast()) };
    if len <= 0 {
        return None;
    }
    let value = CStr::from_bytes_until_nul(&value).ok()?;
    let value = value.to_string_lossy().trim().to_owned();
    (!value.is_empty()).then_some(value)
}

/// Writes the panic's message, location and thread to logcat, then runs the
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
            Priority::Error,
            &format!("panic on thread '{thread}'{location}: {message}"),
        );
        previous(info);
    }));
}

/// Hands the `fmt` layer a fresh [`LogcatWriter`] per event, carrying that
/// event's priority.
#[derive(Debug)]
struct Logcat;

impl<'a> MakeWriter<'a> for Logcat {
    type Writer = LogcatWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LogcatWriter::new(Priority::Info)
    }

    fn make_writer_for(&'a self, meta: &Metadata<'_>) -> Self::Writer {
        LogcatWriter::new(Priority::from(meta.level()))
    }
}

/// Collects one formatted event and writes it as a single logcat entry when
/// dropped.
///
/// The `fmt` layer makes one writer per event, writes the whole line into it,
/// and drops it, so buffering until the drop is what turns one event into one
/// entry rather than into however many `write` calls the formatter happened
/// to make.
#[derive(Debug)]
struct LogcatWriter {
    priority: Priority,
    buffer: Vec<u8>,
}

impl LogcatWriter {
    fn new(priority: Priority) -> Self {
        Self {
            priority,
            buffer: Vec::new(),
        }
    }
}

impl io::Write for LogcatWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for LogcatWriter {
    fn drop(&mut self) {
        if !self.buffer.is_empty() {
            write(self.priority, &String::from_utf8_lossy(&self.buffer));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::chunks;

    #[test]
    fn chunks_respect_the_limit_and_character_boundaries() {
        // 'é' is two bytes, so a 3-byte limit has to stop before splitting one.
        let pieces: Vec<&str> = chunks("aéé", 3).collect();
        assert_eq!(pieces, ["aé", "é"]);
        assert!(pieces.iter().all(|piece| piece.len() <= 3));
    }

    #[test]
    fn chunks_of_nothing_is_nothing() {
        assert_eq!(chunks("", 10).count(), 0);
    }

    #[test]
    fn a_limit_smaller_than_a_character_still_makes_progress() {
        // '€' is three bytes; with a limit of one it must still be emitted
        // whole rather than looping on an empty chunk.
        let pieces: Vec<&str> = chunks("€€", 1).collect();
        assert_eq!(pieces, ["€", "€"]);
    }
}
