//! Millisatoshi and whole-satoshi amount types.

use crate::{Error, ErrorCode};

/// A millisatoshi amount.
///
/// This is the unit used for federation balances, ecash notes, and
/// lightning payments: anywhere the protocol itself operates in
/// millisatoshis. All arithmetic is checked: [`Amount::checked_add`] and
/// [`Amount::checked_sub`] return `None` on overflow or underflow instead of
/// panicking or wrapping, and there is no `+`/`-` operator overload.
///
/// On-chain amounts use the distinct [`Sats`] type instead, so that no
/// on-chain code path can silently truncate a sub-satoshi remainder: turning
/// an `Amount` into on-chain `Sats` is always an explicit, fallible or
/// truncating choice ([`Amount::to_sats_exact`] or [`Amount::sats_floor`]).
///
/// Across a foreign-function boundary (for example, into JavaScript via
/// wasm), this type is represented as a 64-bit integer and must cross as a
/// `BigInt`, never a native `number`: a JS `number` cannot represent the
/// full `u64` range without silent precision loss.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Amount(u64);

impl Amount {
    /// Constructs an amount directly from a millisatoshi count.
    pub const fn from_msats(msats: u64) -> Self {
        Self(msats)
    }

    /// Constructs an amount from a whole-satoshi count, converting to
    /// millisatoshis. Returns `None` if `sats * 1000` would overflow `u64`.
    pub const fn from_sats(sats: u64) -> Option<Self> {
        match sats.checked_mul(1_000) {
            Some(msats) => Some(Self(msats)),
            None => None,
        }
    }

    /// Returns the amount as a millisatoshi count.
    pub const fn msats(self) -> u64 {
        self.0
    }

    /// Returns the amount rounded down to the nearest whole satoshi,
    /// discarding any sub-satoshi remainder.
    ///
    /// Use this only where truncation is the intended behavior (for example,
    /// display estimates). On-chain sends should use
    /// [`Amount::to_sats_exact`] so an amount that isn't a whole number of
    /// satoshis is rejected rather than silently rounded.
    pub const fn sats_floor(self) -> Sats {
        Sats(self.0 / 1_000)
    }

    /// Converts to whole satoshis exactly, or returns `None` if `self`
    /// carries a sub-satoshi remainder (is not a multiple of 1000 msat).
    ///
    /// This is the conversion on-chain APIs are expected to use: it never
    /// silently floors, so a caller can surface a clear error instead of
    /// quietly moving a smaller amount than requested.
    pub const fn to_sats_exact(self) -> Option<Sats> {
        if self.0.is_multiple_of(1_000) {
            Some(Sats(self.0 / 1_000))
        } else {
            None
        }
    }

    /// Adds two amounts, returning `None` on overflow.
    pub const fn checked_add(self, rhs: Amount) -> Option<Amount> {
        match self.0.checked_add(rhs.0) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    /// Subtracts `rhs` from `self`, returning `None` if the result would be
    /// negative.
    pub const fn checked_sub(self, rhs: Amount) -> Option<Amount> {
        match self.0.checked_sub(rhs.0) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }
}

impl core::fmt::Display for Amount {
    /// Formats as the millisatoshi count followed by the literal unit
    /// `msat`, e.g. `"1500 msat"`.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} msat", self.0)
    }
}

impl core::str::FromStr for Amount {
    type Err = crate::Error;

    /// Parses exactly what [`Display`](core::fmt::Display) writes: a
    /// millisatoshi count, one space, and the literal unit `msat`, for example
    /// `"1500 msat"`. Whitespace around the whole value is ignored.
    ///
    /// Any other spelling is rejected with
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput), including a
    /// bare number, a different unit, a decimal point, a sign, and a count too
    /// large for a 64-bit integer. A bare number is deliberately not accepted:
    /// it would silently reinterpret a satoshi figure as millisatoshis.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some(digits) = s.trim().strip_suffix(" msat") else {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "invalid amount: expected a millisatoshi count followed by \" msat\"",
            ));
        };
        // `u64::from_str` would also accept a leading `+`, which `Display`
        // never writes, so the digits are checked rather than delegated to.
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "invalid amount: expected a millisatoshi count followed by \" msat\"",
            ));
        }
        let msats = digits.parse::<u64>().map_err(|_| {
            Error::new(
                ErrorCode::InvalidInput,
                "invalid amount: the millisatoshi count does not fit in 64 bits",
            )
        })?;
        Ok(Self(msats))
    }
}

/// A whole-satoshi amount, used for on-chain (peg-in/peg-out) operations.
///
/// Bitcoin's on-chain protocol has no sub-satoshi unit, so on-chain-facing
/// APIs in this crate take and return `Sats` rather than [`Amount`]: there is
/// no representable value that would require flooring, so no code path can
/// silently lose value the way converting an arbitrary millisatoshi amount
/// down to satoshis would. Converting an [`Amount`] to `Sats` is always an
/// explicit choice made by the caller ([`Amount::to_sats_exact`] or
/// [`Amount::sats_floor`]); converting a `Sats` up to an [`Amount`] is
/// [`Sats::to_amount`].
///
/// As with [`Amount`], all arithmetic is checked: [`Sats::checked_add`] and
/// [`Sats::checked_sub`] return `None` on overflow or underflow instead of
/// panicking or wrapping, and there is no `+`/`-` operator overload.
///
/// Like [`Amount`], this crosses a wasm foreign-function boundary as a
/// `BigInt`, never a native JS `number`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sats(u64);

impl Sats {
    /// Constructs an amount directly from a whole-satoshi count.
    pub const fn from_sats(sats: u64) -> Self {
        Self(sats)
    }

    /// Returns the amount as a whole-satoshi count.
    pub const fn sats(self) -> u64 {
        self.0
    }

    /// Converts to a millisatoshi [`Amount`]. Returns `None` if `self * 1000`
    /// would overflow `u64` (possible because `u64::MAX` satoshis is larger
    /// than `u64::MAX` millisatoshis can represent).
    pub const fn to_amount(self) -> Option<Amount> {
        match self.0.checked_mul(1_000) {
            Some(msats) => Some(Amount(msats)),
            None => None,
        }
    }

    /// Adds two satoshi amounts, returning `None` on overflow.
    ///
    /// Like [`Amount::checked_add`], and for the same reason: arithmetic on
    /// a money type is checked, so a caller adding a withdrawal to its fee
    /// never has to drop to raw `u64` and never silently wraps.
    pub const fn checked_add(self, rhs: Sats) -> Option<Sats> {
        match self.0.checked_add(rhs.0) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    /// Subtracts `rhs` from `self`, returning `None` if the result would be
    /// negative.
    ///
    /// Like [`Amount::checked_sub`]. `None` rather than a saturating zero,
    /// so that "the fee exceeds the deposit" is a case the caller has to
    /// handle rather than one that quietly reports nothing owed.
    pub const fn checked_sub(self, rhs: Sats) -> Option<Sats> {
        match self.0.checked_sub(rhs.0) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }
}

impl core::fmt::Display for Sats {
    /// Formats as the satoshi count followed by the literal unit `sat`,
    /// e.g. `"25000 sat"`.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} sat", self.0)
    }
}

impl core::str::FromStr for Sats {
    type Err = crate::Error;

    /// Parses exactly what [`Display`](core::fmt::Display) writes: a satoshi
    /// count, one space, and the literal unit `sat`, for example
    /// `"25000 sat"`. Whitespace around the whole value is ignored.
    ///
    /// Any other spelling is rejected with
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput), including a
    /// bare number, a different unit, a decimal point, a sign, and a count too
    /// large for a 64-bit integer.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some(digits) = s.trim().strip_suffix(" sat") else {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "invalid amount: expected a satoshi count followed by \" sat\"",
            ));
        };
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "invalid amount: expected a satoshi count followed by \" sat\"",
            ));
        }
        let sats = digits.parse::<u64>().map_err(|_| {
            Error::new(
                ErrorCode::InvalidInput,
                "invalid amount: the satoshi count does not fit in 64 bits",
            )
        })?;
        Ok(Self(sats))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_sats_overflow_returns_none() {
        assert_eq!(Amount::from_sats(u64::MAX), None);
        assert!(Amount::from_sats(1_000_000).is_some());
    }

    #[test]
    fn msats_round_trips_through_from_msats() {
        let a = Amount::from_msats(1_234_567);
        assert_eq!(a.msats(), 1_234_567);
    }

    #[test]
    fn sats_floor_truncates_sub_satoshi_remainder() {
        let a = Amount::from_msats(1_999);
        assert_eq!(a.sats_floor(), Sats::from_sats(1));
        let a = Amount::from_msats(2_000);
        assert_eq!(a.sats_floor(), Sats::from_sats(2));
    }

    #[test]
    fn to_sats_exact_is_none_on_remainder_and_some_on_exact() {
        assert_eq!(Amount::from_msats(1_999).to_sats_exact(), None);
        assert_eq!(
            Amount::from_msats(2_000).to_sats_exact(),
            Some(Sats::from_sats(2))
        );
    }

    #[test]
    fn checked_add_overflows_to_none() {
        let a = Amount::from_msats(u64::MAX);
        let b = Amount::from_msats(1);
        assert_eq!(a.checked_add(b), None);
        assert_eq!(
            Amount::from_msats(1).checked_add(Amount::from_msats(2)),
            Some(Amount::from_msats(3))
        );
    }

    #[test]
    fn checked_sub_underflows_to_none() {
        let a = Amount::from_msats(1);
        let b = Amount::from_msats(2);
        assert_eq!(a.checked_sub(b), None);
        assert_eq!(
            Amount::from_msats(5).checked_sub(Amount::from_msats(2)),
            Some(Amount::from_msats(3))
        );
    }

    #[test]
    fn sats_to_amount_overflows_to_none() {
        assert_eq!(Sats::from_sats(u64::MAX).to_amount(), None);
        assert_eq!(
            Sats::from_sats(3).to_amount(),
            Some(Amount::from_msats(3_000))
        );
    }

    #[test]
    fn sats_checked_add_overflows_to_none() {
        let a = Sats::from_sats(u64::MAX);
        let b = Sats::from_sats(1);
        assert_eq!(a.checked_add(b), None);
        assert_eq!(
            Sats::from_sats(1).checked_add(Sats::from_sats(2)),
            Some(Sats::from_sats(3))
        );
    }

    #[test]
    fn sats_checked_sub_underflows_to_none() {
        let a = Sats::from_sats(1);
        let b = Sats::from_sats(2);
        assert_eq!(a.checked_sub(b), None);
        assert_eq!(
            Sats::from_sats(5).checked_sub(Sats::from_sats(2)),
            Some(Sats::from_sats(3))
        );
    }

    #[test]
    fn amount_display_format() {
        assert_eq!(Amount::from_msats(1500).to_string(), "1500 msat");
    }

    #[test]
    fn sats_display_format() {
        assert_eq!(Sats::from_sats(25_000).to_string(), "25000 sat");
    }

    #[test]
    fn amount_parses_its_own_display_form() {
        assert_eq!(
            "1500 msat".parse::<Amount>().expect("the display form"),
            Amount::from_msats(1_500)
        );
        // Round-tripping the documented form is the whole contract.
        let amount = Amount::from_msats(1_234_567);
        assert_eq!(
            amount.to_string().parse::<Amount>().expect("round trip"),
            amount
        );
        // Surrounding whitespace is ignored, because a value pasted out of a
        // log or a text field routinely carries some.
        assert_eq!(
            "  0 msat\n".parse::<Amount>().expect("trimmed"),
            Amount::from_msats(0)
        );
    }

    #[test]
    fn amount_rejects_anything_but_its_display_form() {
        // A bare number is the dangerous one: it looks like it should work and
        // there is no unit to disagree about, so accepting it would invite a
        // caller to hand over satoshis and get millisatoshis.
        for rejected in [
            "1500",
            "1500 sat",
            "1500msat",
            "1500  msat",
            "1.5 msat",
            "+1500 msat",
            "-1 msat",
            " msat",
            "",
            "18446744073709551616 msat",
        ] {
            let error = rejected
                .parse::<Amount>()
                .expect_err("only the display form parses");
            assert_eq!(error.code, crate::ErrorCode::InvalidInput);
        }
    }

    #[test]
    fn sats_parses_its_own_display_form() {
        assert_eq!(
            "25000 sat".parse::<Sats>().expect("the display form"),
            Sats::from_sats(25_000)
        );
        let sats = Sats::from_sats(21_000_000);
        assert_eq!(sats.to_string().parse::<Sats>().expect("round trip"), sats);
    }

    #[test]
    fn sats_rejects_anything_but_its_display_form() {
        for rejected in [
            "25000",
            "25000 msat",
            "25000sat",
            "0.5 sat",
            "sat",
            "",
            "+5 sat",
            "-5 sat",
            "18446744073709551616 sat",
        ] {
            let error = rejected
                .parse::<Sats>()
                .expect_err("only the display form parses");
            assert_eq!(error.code, crate::ErrorCode::InvalidInput);
        }
    }
}
