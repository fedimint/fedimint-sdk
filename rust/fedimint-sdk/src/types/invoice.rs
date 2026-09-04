//! Bolt11 lightning invoices.

use fedimint_ln_common::lightning_invoice::{self, Bolt11InvoiceDescriptionRef, Currency};

use super::{Amount, Network, Timestamp};
use crate::{Error, ErrorCode};

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
    invoice: lightning_invoice::Bolt11Invoice,
}

impl Bolt11Invoice {
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
        // Already millisatoshis upstream
        // (lightning-invoice-0.33.3/src/lib.rs:1650).
        self.invoice.amount_milli_satoshis().map(Amount::from_msats)
    }

    /// The invoice's human-readable description, as embedded by the payee.
    /// Empty if the invoice carries no description (some invoices instead
    /// embed a hash of an out-of-band description, which this accessor does
    /// not resolve).
    pub fn description(&self) -> String {
        // `description()` returns one of two variants
        // (lightning-invoice-0.33.3/src/lib.rs:1483). `Description`'s `Display`
        // replaces control characters, which matters because this text is
        // attacker-controlled and ends up in a UI.
        match self.invoice.description() {
            Bolt11InvoiceDescriptionRef::Direct(description) => description.to_string(),
            Bolt11InvoiceDescriptionRef::Hash(_) => String::new(),
        }
    }

    /// The point in time after which this invoice is no longer payable.
    pub fn expires_at(&self) -> Timestamp {
        // Upstream returns `None` when the timestamp plus the expiry overflows
        // a `Duration` (lightning-invoice-0.33.3/src/lib.rs:1533). Saturating
        // is the honest reading of "no longer payable after this", and the
        // accessor must not panic.
        let millis = self.invoice.expires_at().map_or(u64::MAX, |since_epoch| {
            u64::try_from(since_epoch.as_millis()).unwrap_or(u64::MAX)
        });
        Timestamp::from_epoch_millis(millis)
    }

    /// Whether this invoice's expiry has already passed, as of now.
    ///
    /// This is a convenience over comparing [`Bolt11Invoice::expires_at`] to
    /// the current time; it does not contact the federation or the payee, so
    /// a `false` result is not itself a guarantee that a payment attempt
    /// will succeed.
    pub fn is_expired(&self) -> bool {
        // Upstream's own `is_expired` calls `SystemTime::now`, which traps on
        // bare wasm32-unknown-unknown. `fedimint_core::time::now`
        // (fedimint-core/src/time.rs) reads the JS clock there, and
        // `would_expire` is the variant that takes "now" as an argument
        // (lightning-invoice-0.33.3/src/lib.rs:1577).
        let now = fedimint_core::time::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        self.invoice.would_expire(now)
    }

    /// Wraps an already-parsed bolt11 invoice.
    ///
    /// Crate-internal: this performs no validation of its own, so it is not
    /// part of the public API. Validation belongs in
    /// [`FromStr`](core::str::FromStr), which is the only way a caller
    /// outside this crate can build one.
    pub(crate) fn from_upstream(invoice: lightning_invoice::Bolt11Invoice) -> Self {
        Self { invoice }
    }

    /// The network this invoice's BOLT11 currency names, or `None` for a
    /// currency this crate's [`Network`] cannot represent.
    ///
    /// Crate-internal: this is what an on-chain-network comparison at quote
    /// time reads. `None` is a real answer, not a failure: a simnet invoice
    /// names a network the SDK has no variant for, and that still proves a
    /// mismatch against any federation.
    pub(crate) fn network(&self) -> Option<Network> {
        // Matched here rather than through upstream's `From<Currency> for
        // Network` (lightning-invoice-0.33.3/src/lib.rs:465-476), which
        // collapses `Simnet` into `Regtest` and would report a network the
        // invoice never named.
        //
        // `BitcoinTestnet` is BOLT11's `tb` prefix, which testnet3 and testnet4 both use,
        // so this single answer of `Network::Testnet` cannot distinguish them. The
        // lightning facade must expand it into `{Testnet, Testnet4}` when it fills
        // `ErrorDetails::NetworkMismatch::compatible`, rather than reporting testnet3 alone.
        match self.invoice.currency() {
            Currency::Bitcoin => Some(Network::Bitcoin),
            Currency::BitcoinTestnet => Some(Network::Testnet),
            Currency::Regtest => Some(Network::Regtest),
            Currency::Signet => Some(Network::Signet),
            Currency::Simnet => None,
        }
    }

    /// The BOLT11 currency prefix this invoice was spelled with, lowercased.
    ///
    /// Crate-internal: this is what fills
    /// [`ErrorDetails::NetworkMismatch::observed_prefix`](crate::ErrorDetails::NetworkMismatch),
    /// and it is the only field that can describe a currency this SDK has no
    /// name for.
    pub(crate) fn observed_prefix(&self) -> String {
        // The same table upstream's `Display for Currency` uses
        // (lightning-invoice-0.33.3/src/ser.rs:194-200).
        match self.invoice.currency() {
            Currency::Bitcoin => "bc",
            Currency::BitcoinTestnet => "tb",
            Currency::Regtest => "bcrt",
            Currency::Signet => "tbs",
            Currency::Simnet => "sb",
        }
        .to_owned()
    }
}

impl core::fmt::Display for Bolt11Invoice {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Re-encodes the signed invoice, reproducing the original bolt11
        // string (lightning-invoice-0.33.3/src/ser.rs:152-156).
        core::fmt::Display::fmt(&self.invoice, f)
    }
}

impl core::str::FromStr for Bolt11Invoice {
    type Err = crate::Error;

    /// Parses a bolt11 invoice from its canonical string form. Returns
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a
    /// malformed value.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // An expired invoice still parses: expiry is refused at quote time,
        // where the caller can be told something useful about it. Upstream's
        // `ParseOrSemanticError` names a structural problem, not the invoice,
        // so its text may be passed on.
        let invoice = s
            .trim()
            .parse::<lightning_invoice::Bolt11Invoice>()
            .map_err(|err| {
                Error::new(
                    ErrorCode::InvalidInput,
                    format!("invalid bolt11 invoice: {err}"),
                )
            })?;
        Ok(Self { invoice })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The BOLT11 specification's "coffee beans" vector, 0.025 BTC on mainnet,
    /// taken verbatim from `lightning-invoice`'s own test suite.
    const MAINNET_25M: &str = "lnbc25m1pvjluezpp5qqqsyqcyq5rqwzqfqqqsyqcyq5rqwzqfqqqsyqcyq5rqwzqfqypqdq5vdhkven9v5sxyetpdeessp5zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zygs9q5sqqqqqqqqqqqqqqqpqsq67gye39hfg3zd8rgc80k32tvy9xk2xunwm5lzexnvpx6fd77en8qaq424dxgt56cag2dpt359k3ssyhetktkpqh24jqnjyw6uqd08sgptq44qu";

    /// A mainnet invoice with no amount. The specification's own amountless
    /// vector predates mandatory payment secrets and no longer parses as a
    /// complete invoice, so this one was built once with `InvoiceBuilder` from
    /// fixed inputs: secret key `[0x11; 32]`, payment hash `[0x22; 32]`,
    /// payment secret `[0x33; 32]`, timestamp 1_700_000_000 s, CLTV delta 144.
    const MAINNET_AMOUNTLESS: &str = "lnbc1pj48ugqdq0dehjqctdda6kuaqpp5yg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3qsp5xvenxvenxvenxvenxvenxvenxvenxvenxvenxvenxvenxvenxves9qrsgqcqzyswm4efuu52zkzgrcc35fra9fmvj7s9ppxmej85s83hjkh7crcy9vqlradwalsmq40knf3552panjvlhjlrfazmvs86krxuaygut8v30sq0y0422";

    /// A regtest invoice for 100_000 msat, built the same way with payment
    /// hash `[0x44; 32]` and payment secret `[0x55; 32]`.
    const REGTEST_100_000: &str = "lnbcrt1u1pj48ugqdq2vdhkven9v5pp5g3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zqsp5242424242424242424242424242424242424242424242424242s9qrsgqcqzys2reg4wsryjt5w8z33ugydecgfmgyvtttwa7e0yzlm803z203j9hqspa4lr6m09cd808xkw9uh4sxc8wf3w6k0gaf5zrqm7zhcxug0vqqpdkpja";

    #[test]
    fn an_invoice_round_trips_through_display_and_from_str() {
        let invoice = MAINNET_25M
            .parse::<Bolt11Invoice>()
            .expect("a valid invoice");
        assert_eq!(invoice.to_string(), MAINNET_25M);
        let padded = format!("  {MAINNET_25M}\n");
        assert_eq!(padded.parse::<Bolt11Invoice>().expect("trimmed"), invoice);
    }

    #[test]
    fn an_invoice_reports_its_amount_in_millisatoshis() {
        assert_eq!(
            MAINNET_25M
                .parse::<Bolt11Invoice>()
                .expect("valid")
                .amount(),
            Some(Amount::from_msats(2_500_000_000))
        );
        assert_eq!(
            REGTEST_100_000
                .parse::<Bolt11Invoice>()
                .expect("valid")
                .amount(),
            Some(Amount::from_msats(100_000))
        );
    }

    #[test]
    fn an_amountless_invoice_reports_no_amount() {
        // This is the accessor an application checks to decline the invoice
        // with a useful message rather than surfacing a failed quote.
        let invoice = MAINNET_AMOUNTLESS
            .parse::<Bolt11Invoice>()
            .expect("a valid amountless invoice");
        assert_eq!(invoice.amount(), None);
        assert_eq!(invoice.to_string(), MAINNET_AMOUNTLESS);
    }

    #[test]
    fn description_is_the_embedded_text() {
        assert_eq!(
            MAINNET_25M
                .parse::<Bolt11Invoice>()
                .expect("valid")
                .description(),
            "coffee beans"
        );
        assert_eq!(
            MAINNET_AMOUNTLESS
                .parse::<Bolt11Invoice>()
                .expect("valid")
                .description(),
            "no amount"
        );
    }

    #[test]
    fn description_is_empty_for_a_hash_description() {
        // No known fixed vector uses a hash description, so this one is built with
        // `InvoiceBuilder` the same way `MAINNET_AMOUNTLESS` above was.
        use fedimint_core::bitcoin::hashes::{Hash, sha256};
        use fedimint_core::secp256k1::{Secp256k1, SecretKey};
        use lightning_invoice::{InvoiceBuilder, PaymentSecret};

        let secp = Secp256k1::new();
        let private_key = SecretKey::from_slice(&[0x11; 32]).expect("a valid secret key");
        let raw = InvoiceBuilder::new(Currency::Regtest)
            .amount_milli_satoshis(1_000)
            .payment_hash(sha256::Hash::hash(&[0x22; 32]))
            .payment_secret(PaymentSecret([0x33; 32]))
            .description_hash(sha256::Hash::hash(b"an out-of-band description"))
            .duration_since_epoch(core::time::Duration::from_secs(1_700_000_000))
            .min_final_cltv_expiry_delta(144)
            .build_signed(|hash| secp.sign_ecdsa_recoverable(hash, &private_key))
            .expect("a valid invoice")
            .to_string();

        let invoice = raw.parse::<Bolt11Invoice>().expect("a valid invoice");
        assert_eq!(invoice.description(), "");
    }

    #[test]
    fn expiry_is_reported_in_epoch_milliseconds() {
        // The vector was created at 1_496_314_658 s with a 3_600 s expiry, so
        // it expired long ago and stays a stable fixture forever.
        let invoice = MAINNET_25M.parse::<Bolt11Invoice>().expect("valid");
        assert_eq!(
            invoice.expires_at(),
            Timestamp::from_epoch_millis(1_496_318_258_000)
        );
        assert!(invoice.is_expired());
        // An expired invoice still parses: expiry is a quote-time refusal, not
        // a parse-time one.
        assert_eq!(
            REGTEST_100_000
                .parse::<Bolt11Invoice>()
                .expect("valid")
                .expires_at(),
            Timestamp::from_epoch_millis(1_700_003_600_000)
        );
    }

    #[test]
    fn network_and_prefix_come_from_the_bolt11_currency() {
        let mainnet = MAINNET_25M.parse::<Bolt11Invoice>().expect("valid");
        assert_eq!(mainnet.network(), Some(Network::Bitcoin));
        assert_eq!(mainnet.observed_prefix(), "bc");

        let regtest = REGTEST_100_000.parse::<Bolt11Invoice>().expect("valid");
        assert_eq!(regtest.network(), Some(Network::Regtest));
        assert_eq!(regtest.observed_prefix(), "bcrt");
    }

    #[test]
    fn a_malformed_invoice_is_invalid_input() {
        for rejected in ["", "lnbcrt1000n1pexample", "not an invoice"] {
            let error = rejected
                .parse::<Bolt11Invoice>()
                .expect_err("a malformed invoice is rejected");
            assert_eq!(error.code, crate::ErrorCode::InvalidInput);
        }
    }
}
