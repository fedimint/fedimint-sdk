//! A portable point-in-time type for the public API.

/// A point in time, expressed as milliseconds since the Unix epoch
/// (1970-01-01T00:00:00Z), ignoring leap seconds.
///
/// This is the only time type in the crate's public API. `std::time::SystemTime`
/// is not used because it does not cross a foreign-function boundary
/// portably: its internal representation is platform-specific and it has no
/// stable wire format. An integer count of epoch milliseconds, by contrast,
/// maps directly onto every target's native time type (a JS `Date`, a Swift
/// `Date`, a Kotlin `Instant`) with a single, unambiguous conversion.
///
/// `Timestamp` carries no timezone: it is always UTC, as epoch time
/// inherently is. Converting to a local time for display is the caller's
/// responsibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(u64);

impl Timestamp {
    /// Constructs a timestamp from a count of milliseconds since the Unix
    /// epoch.
    pub const fn from_epoch_millis(millis: u64) -> Self {
        Self(millis)
    }

    /// Returns the timestamp as a count of milliseconds since the Unix
    /// epoch.
    pub const fn epoch_millis(self) -> u64 {
        self.0
    }
}

impl core::fmt::Display for Timestamp {
    /// Formats as the bare integer epoch-millisecond value, e.g.
    /// `"1700000000000"`. This is a machine-readable representation, not a
    /// human calendar rendering — callers that need the latter should
    /// convert `epoch_millis()` with their platform's date/time formatting.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_millis_round_trips_through_from_epoch_millis() {
        let ts = Timestamp::from_epoch_millis(1_700_000_000_000);
        assert_eq!(ts.epoch_millis(), 1_700_000_000_000);
    }

    #[test]
    fn display_format_is_bare_integer() {
        assert_eq!(
            Timestamp::from_epoch_millis(1_700_000_000_000).to_string(),
            "1700000000000"
        );
        assert_eq!(Timestamp::from_epoch_millis(0).to_string(), "0");
    }
}
