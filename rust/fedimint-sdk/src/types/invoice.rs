//! Bolt11 lightning invoices.

use super::{Amount, Timestamp};

/// A parsed bolt11 lightning invoice.
///
/// `Bolt11Invoice` is opaque: callers obtain one by parsing an invoice
/// string a payee gave them, read it through the accessors below, and pass
/// it to a quote call; they never construct or reassemble one field by
/// field. It round-trips through [`Display`](core::fmt::Display) (recovering
/// the original bolt11 string) and [`FromStr`](core::str::FromStr) with a
/// validating parse.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Bolt11Invoice {
    invoice: String,
}

impl Bolt11Invoice {
    /// Wraps an already-validated bolt11 invoice string.
    ///
    /// Crate-internal: this performs no validation of its own, so it is not
    /// part of the public API. Validation belongs in
    /// [`FromStr`](core::str::FromStr), which is the only way a caller
    /// outside this crate can build one.
    pub(crate) fn from_raw(raw: String) -> Self {
        Self { invoice: raw }
    }

    /// The amount encoded in the invoice, or `None` if the invoice is
    /// amountless (the payer would choose the amount).
    ///
    /// `None` means the invoice cannot be paid through this SDK at all, and
    /// no amount the caller supplies can change that: Fedimint does not
    /// support paying amountless BOLT11 invoices, deliberately, because it
    /// cannot be done safely. Quoting such an invoice fails with
    /// [`ErrorCode::AmountlessInvoice`](crate::ErrorCode::AmountlessInvoice),
    /// so checking this accessor is how an application declines the invoice
    /// with a useful message instead of surfacing a failed quote.
    pub fn amount(&self) -> Option<Amount> {
        unimplemented!()
    }

    /// The invoice's human-readable description, as embedded by the payee.
    /// Empty if the invoice carries no description (some invoices instead
    /// embed a hash of an out-of-band description, which this accessor does
    /// not resolve).
    pub fn description(&self) -> String {
        unimplemented!()
    }

    /// The point in time after which this invoice is no longer payable.
    pub fn expires_at(&self) -> Timestamp {
        unimplemented!()
    }

    /// Whether this invoice's expiry has already passed, as of now.
    ///
    /// This is a convenience over comparing [`Bolt11Invoice::expires_at`] to
    /// the current time; it does not contact the federation or the payee, so
    /// a `false` result is not itself a guarantee that a payment attempt
    /// will succeed.
    pub fn is_expired(&self) -> bool {
        unimplemented!()
    }
}

impl core::fmt::Display for Bolt11Invoice {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let _ = &self.invoice;
        unimplemented!()
    }
}

impl core::str::FromStr for Bolt11Invoice {
    type Err = crate::Error;

    /// Parses a bolt11 invoice from its canonical string form. Returns
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a
    /// malformed value.
    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        unimplemented!()
    }
}
