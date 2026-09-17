//! How a detail envelope crosses the boundary.

use crate::{DetailEnvelope, Error, ErrorCode, RawErrorDetails};

// How every structured detail crosses, wherever one appears in the surface: on `Error` through
// `details()` below, and on `Diagnostic`'s own field, which is what lets
// `FederationStatus::Quarantined` carry the real `Diagnostic` instead of a flattened projection
// of it.
//
// The bridge type is `RawErrorDetails` rather than this enum's own shape because
// `Interpreted` carries `ErrorDetails`, which is `#[non_exhaustive]` and grows: the contract on
// `RawErrorDetails` says the boundary shape is always the raw envelope, with each side projecting
// the typed case locally. Lowering therefore re-encodes, and lifting re-projects, so a kind this
// build has no projection for still survives the round trip as bytes.
uniffi::custom_type!(DetailEnvelope, RawErrorDetails, {
    lower: |envelope| envelope.to_raw(),
    try_lift: |raw| Ok(DetailEnvelope::from_raw(raw)),
});

/// FFI accessors for the opaque error interface. Foreign callers cannot read the struct fields off
/// the handle, so these mirror them: `code()` is the stable value to branch on, `reason()` is the
/// human-readable message, and `details()` is the structured detail, lowered to the optional
/// `RawErrorDetails` the contract on that type defines (see the `custom_type!` above) rather than
/// to the growing `ErrorDetails`, which never crosses. `reason` rather than `message` so it does
/// not collide with the message property every foreign exception base class already has.
#[uniffi::export]
impl Error {
    /// The stable [`ErrorCode`] for this failure.
    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// The human-readable message. Never parse it; branch on [`code`](Self::code).
    pub fn reason(&self) -> String {
        self.message.clone()
    }

    /// The structured detail for this failure, where it has any.
    pub fn details(&self) -> Option<RawErrorDetails> {
        self.details.as_ref().map(DetailEnvelope::to_raw)
    }
}
