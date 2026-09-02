//! Millisatoshi and whole-satoshi amount types.

/// A millisatoshi amount.
///
/// This is the unit used for federation balances, ecash notes, and
/// lightning payments — anywhere the protocol itself operates in
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
/// `BigInt`, never a native `number` — a JS `number` cannot represent the
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

/// A signed change to a balance: what an operation finally did to it.
///
/// The realized "what landed" figure on every receive-side details record
/// ([`LnReceiveDetails`](crate::LnReceiveDetails),
/// [`OnchainReceiveDetails`](crate::OnchainReceiveDetails),
/// [`EcashReceiveDetails`](crate::EcashReceiveDetails)) is one of these
/// rather than an [`Amount`], because a receive can *reduce* the balance. The
/// primary module balances every transaction it finalizes by sweeping some of
/// the wallet's existing notes in as inputs and reissuing them as outputs
/// alongside the incoming value, and it charges a fee per output, so a
/// receive too small to cover the outputs it needs is completed from the
/// existing balance; and a finalization that fails part-way can lose the
/// reissued pre-existing value along with the incoming value. An unsigned
/// credit cannot say either of those things, and reading a zero where the
/// balance actually fell would be the exact lie a receipt exists to prevent.
///
/// The movement is `Credit(amount)` when the balance rose by `amount` and
/// `Debit(amount)` when it fell. Zero is always written as
/// [`NetMovement::ZERO`], a credit of nothing; a debit of nothing is never
/// written, so the two variants never describe the same movement.
///
/// Crosses a foreign-function boundary as a tagged value with one
/// [`Amount`] payload; see that type for how the payload itself crosses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NetMovement {
    /// The balance rose by this much.
    Credit(Amount),
    /// The balance fell by this much.
    Debit(Amount),
}

impl NetMovement {
    /// No movement: the canonical zero, a credit of nothing.
    pub const ZERO: Self = Self::Credit(Amount::from_msats(0));

    /// `gross` less `fee` as a movement: a credit of the difference when
    /// the fee fits inside the gross, and a debit of the excess when it does
    /// not.
    ///
    /// This is the shape of every success-side identity on a receive record:
    /// what came in, less what the accepted transaction charged, is what the
    /// balance did — in either direction.
    pub const fn gross_less_fee(gross: Amount, fee: Amount) -> Self {
        match gross.checked_sub(fee) {
            Some(credit) => Self::Credit(credit),
            None => match fee.checked_sub(gross) {
                Some(debit) => Self::Debit(debit),
                None => Self::ZERO,
            },
        }
    }

    /// The amount credited, or zero for a debit.
    pub const fn credited(self) -> Amount {
        match self {
            Self::Credit(amount) => amount,
            Self::Debit(_) => Amount::from_msats(0),
        }
    }

    /// The amount debited, or zero for a credit.
    pub const fn debited(self) -> Amount {
        match self {
            Self::Credit(_) => Amount::from_msats(0),
            Self::Debit(amount) => amount,
        }
    }

    /// Whether this movement is at or below `other` — the comparison a
    /// failure-side inequality makes, in signed terms: a debit is below every
    /// credit, and two movements of the same sign compare by magnitude in
    /// that sign's direction.
    pub const fn is_at_most(self, other: Self) -> bool {
        match (self, other) {
            (Self::Credit(a), Self::Credit(b)) => a.msats() <= b.msats(),
            (Self::Debit(a), Self::Debit(b)) => a.msats() >= b.msats(),
            (Self::Debit(_), Self::Credit(_)) => true,
            (Self::Credit(_), Self::Debit(_)) => false,
        }
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
    fn net_movement_gross_less_fee_signs_the_difference() {
        let gross = Amount::from_msats(1_000);
        assert_eq!(
            NetMovement::gross_less_fee(gross, Amount::from_msats(300)),
            NetMovement::Credit(Amount::from_msats(700))
        );
        assert_eq!(
            NetMovement::gross_less_fee(gross, Amount::from_msats(1_300)),
            NetMovement::Debit(Amount::from_msats(300))
        );
        // Zero is always the credit, never a debit of nothing.
        assert_eq!(NetMovement::gross_less_fee(gross, gross), NetMovement::ZERO);
        assert_eq!(NetMovement::ZERO.credited(), Amount::from_msats(0));
        assert_eq!(NetMovement::ZERO.debited(), Amount::from_msats(0));
    }

    #[test]
    fn net_movement_orders_debits_below_credits() {
        let credit = NetMovement::Credit(Amount::from_msats(500));
        let smaller_credit = NetMovement::Credit(Amount::from_msats(100));
        let debit = NetMovement::Debit(Amount::from_msats(100));
        let bigger_debit = NetMovement::Debit(Amount::from_msats(900));
        assert!(smaller_credit.is_at_most(credit));
        assert!(!credit.is_at_most(smaller_credit));
        assert!(debit.is_at_most(credit));
        assert!(!credit.is_at_most(debit));
        assert!(bigger_debit.is_at_most(debit));
        assert!(!debit.is_at_most(bigger_debit));
        assert!(credit.is_at_most(credit));
    }

    #[test]
    fn amount_display_format() {
        assert_eq!(Amount::from_msats(1500).to_string(), "1500 msat");
    }

    #[test]
    fn sats_display_format() {
        assert_eq!(Sats::from_sats(25_000).to_string(), "25000 sat");
    }
}
