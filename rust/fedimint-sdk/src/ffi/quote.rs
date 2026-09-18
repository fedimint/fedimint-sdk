//! The single-use guard the three quote objects share.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::{Error, ErrorCode, ErrorDetails, Result, Timestamp};

/// The single-use guard a quote object carries across the UniFFI boundary.
///
/// In plain Rust `send` takes its quote by value, so a quote is consumed by the first attempt
/// whatever that attempt's outcome, and a second attempt does not compile. A binding only ever
/// holds a shared `Arc` to the quote, so the quote holds this instead and each facade's UniFFI
/// `send` claims it before doing anything else. The claim is never released, for parity with the
/// by-value API: a failed attempt may already have moved funds, so the only safe retry is a fresh
/// quote.
#[derive(Debug, Default)]
pub(crate) struct QuoteClaim(AtomicBool);

impl QuoteClaim {
    /// Claims the quote for one `send` attempt.
    ///
    /// # Errors
    ///
    /// [`QuoteExpired`](ErrorCode::QuoteExpired) with `already_executed: true` if an earlier
    /// attempt already claimed it, the sub-case [`ErrorDetails::QuoteExpired`] reserves for a
    /// binding reusing a quote object. The message says the quote was already submitted, not that
    /// anything was paid: the earlier attempt may have failed.
    pub(crate) fn claim(&self, expires_at: Timestamp) -> Result<()> {
        if self.0.swap(true, Ordering::SeqCst) {
            return Err(Error::with_details(
                ErrorCode::QuoteExpired,
                "this quote was already submitted to send; quote again",
                ErrorDetails::QuoteExpired {
                    expires_at,
                    already_executed: true,
                },
            ));
        }
        Ok(())
    }
}
