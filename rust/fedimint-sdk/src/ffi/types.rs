//! How the crate's value types cross the boundary: every `custom_type!` conversion, one per type,
//! in the order `types/` declares them.
//!
//! Each of these is a real type whose FFI shape is a primitive — a string or an integer — so
//! nothing about it needs to sit beside its definition: the type keeps its own parsing and
//! formatting, and this is only the pair of closures that lowers it and lifts it back.

use std::collections::{BTreeMap, HashMap};

use crate::{
    Address, Amount, Bolt11Invoice, Cursor, FederationId, GatewayId, OperationId, Preimage, Sats,
    Timestamp, Txid,
};

uniffi::custom_type!(Address, String, {
    lower: |address| address.to_string(),
    try_lift: |s| s.parse::<Address>().map_err(Into::into),
});

uniffi::custom_type!(Amount, u64, {
    lower: |amount| amount.msats(),
    try_lift: |msats| Ok(Amount::from_msats(msats)),
});

uniffi::custom_type!(Sats, u64, {
    lower: |sats| sats.sats(),
    try_lift: |sats| Ok(Sats::from_sats(sats)),
});

uniffi::custom_type!(FederationId, String, {
    lower: |id| id.to_string(),
    try_lift: |s| s.parse::<FederationId>().map_err(Into::into),
});

uniffi::custom_type!(OperationId, String, {
    lower: |id| id.to_string(),
    try_lift: |s| s.parse::<OperationId>().map_err(Into::into),
});

uniffi::custom_type!(GatewayId, String, {
    lower: |id| id.to_string(),
    try_lift: |s| s.parse::<GatewayId>().map_err(Into::into),
});

uniffi::custom_type!(Txid, String, {
    lower: |id| id.to_string(),
    try_lift: |s| s.parse::<Txid>().map_err(Into::into),
});

uniffi::custom_type!(Cursor, String, {
    lower: |cursor| cursor.to_string(),
    try_lift: |s| s.parse::<Cursor>().map_err(Into::into),
});

uniffi::custom_type!(Bolt11Invoice, String, {
    lower: |invoice| invoice.to_string(),
    try_lift: |s| s.parse::<Bolt11Invoice>().map_err(Into::into),
});

uniffi::custom_type!(Preimage, String, {
    lower: |preimage| preimage.to_string(),
    try_lift: |s| s.parse::<Preimage>().map_err(Into::into),
});

uniffi::custom_type!(Timestamp, u64, {
    lower: |ts| ts.epoch_millis(),
    try_lift: |millis| Ok(Timestamp::from_epoch_millis(millis)),
});

// `BTreeMap` has no UniFFI converter (unlike `HashMap`), and `custom_type!` needs a bare
// identifier, so the shape of `FederationPreview`'s `meta` field is bridged to its `HashMap`
// equivalent through an alias. `remote` because the target type is `std`'s. The map's contents are
// unchanged; a binding's map type is insertion-ordered regardless. Mirrors `fedimint-core`'s own
// `MetaMap`.
type MetaMap = BTreeMap<String, String>;

uniffi::custom_type!(MetaMap, HashMap<String, String>, {
    remote,
    lower: |m| m.into_iter().collect(),
    try_lift: |h| Ok(h.into_iter().collect()),
});
