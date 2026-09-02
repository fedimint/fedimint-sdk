//! Plain data types shared across the crate's facades.
//!
//! Most types here are either a small `Copy` value type (amounts,
//! timestamps) or an opaque, string-shaped handle that round-trips through
//! `Display` and `FromStr` (ids, invite codes, ecash notes, invoices,
//! addresses, preimages). Three are deliberately neither:
//!
//! - [`Mnemonic`] parses with `FromStr` but has **no `Display`**, and no
//!   `Debug` either. That is a central safety contract of this crate rather
//!   than an omission — a seed phrase must not be formattable into a log
//!   line — and the words come out only through the explicit
//!   [`Mnemonic::words`]. See that type for the full rationale.
//! - [`Network`] is a small `Copy` enum, matched on rather than printed or
//!   parsed.
//! - [`FederationPreview`] is a plain data record with public fields, not a
//!   handle: it exists to be read field by field to render a screen.
//!
//! Keeping them in one module lets the facade modules (`federation`,
//! `ecash`, `lightning`, `onchain`, ...) depend on a single, stable
//! vocabulary.

mod address;
mod amount;
mod ids;
mod invite;
mod invoice;
mod mnemonic;
mod network;
mod notes;
mod preimage;
mod timestamp;

pub use address::Address;
pub use amount::{Amount, NetMovement, Sats};
pub use ids::{Cursor, FederationId, GatewayId, OperationId, Txid};
pub use invite::{FederationPreview, InviteCode};
pub use invoice::Bolt11Invoice;
pub use mnemonic::Mnemonic;
pub use network::Network;
pub use notes::Notes;
pub use preimage::Preimage;
pub use timestamp::Timestamp;
