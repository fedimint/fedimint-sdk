//! Plain data types shared across the crate's facades.
//!
//! Every type here is either a small `Copy` value type (amounts, timestamps)
//! or an opaque, string-shaped handle that round-trips through `Display` and
//! `FromStr` (ids, invite codes, ecash notes, invoices, addresses). Keeping
//! them in one module lets the facade modules (`federation`, `ecash`,
//! `lightning`, `onchain`, ...) depend on a single, stable vocabulary.

mod address;
mod amount;
mod ids;
mod invite;
mod invoice;
mod mnemonic;
mod network;
mod notes;
mod timestamp;

pub use address::Address;
pub use amount::{Amount, Sats};
pub use ids::{Cursor, FederationId, GatewayId, OperationId, Txid};
pub use invite::{FederationPreview, InviteCode};
pub use invoice::Bolt11Invoice;
pub use mnemonic::Mnemonic;
pub use network::Network;
pub use notes::Notes;
pub use timestamp::Timestamp;
