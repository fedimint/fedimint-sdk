//! High-level Fedimint client SDK. API skeleton per fedimint-sdk#344.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(missing_debug_implementations)]
// Skeleton-phase allowances — remove both when implementation starts. Parameters
// are deliberately named (they are rustdoc-visible API contract) but unused, and
// the private placeholder `inner` fields are never constructed or read while
// every body is unimplemented!(). CI runs with RUSTFLAGS="-D warnings"
// (section 3), so these must be in-source allows, not tolerated warnings:
#![allow(unused_variables)]
#![allow(dead_code)]

mod activity;
mod ecash;
mod error;
mod federation;
mod lightning;
mod meta;
mod onchain;
mod operation;
#[cfg(feature = "experimental")]
mod recovery;
mod sdk;
mod storage;
mod types;

pub use activity::{ActivityItem, ActivityPage, ActivityStatus, Direction};
pub use ecash::{Ecash, EcashReceiveState, EcashSend, EcashSendState};
pub use error::{Error, ErrorCode, Result};
pub use federation::{BalanceUpdates, Capabilities, Federation};
pub use lightning::{Lightning, LightningRoute, LnQuote, LnReceive, LnReceiveState, LnSendState};
pub use meta::{ConsensusMetadata, Meta};
pub use onchain::{Onchain, OnchainQuote, OnchainReceive, OnchainReceiveState, OnchainSendState};
pub use operation::{AnyOperation, Operation, OperationKind, OperationState, OperationUpdates};
// Behind the off-by-default `experimental` feature, and excluded from the
// crate's stability contract; see the module's own documentation.
#[cfg(feature = "experimental")]
pub use recovery::{Recovery, RecoveryState};
pub use sdk::{Sdk, SdkBuilder};
pub use storage::Storage;
pub use types::{
    Address, Amount, Bolt11Invoice, Cursor, FederationId, FederationPreview, GatewayId, InviteCode,
    Mnemonic, Network, Notes, OperationId, Sats, Timestamp, Txid,
};
