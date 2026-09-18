//! The SDK handle's FFI adapters, and the UniFFI entry point.

use std::sync::Arc;

use crate::{
    Federation, FederationId, FederationInfo, FederationStatus, FederationStatusUpdates, Mnemonic,
    Result, Sdk, Storage,
};

/// Opens an instance over `data_dir` — the entry point a mobile host calls.
///
/// Pass a `mnemonic` ([`Mnemonic::from_words`] / [`Mnemonic::generate`]) to establish that seed.
/// Pass `None` to use the seed the storage already holds, or, over storage proven empty, to
/// generate and persist a fresh one. The failure modes are exactly those of
/// [`SdkBuilder::build`](crate::SdkBuilder::build).
///
/// The flattened form of [`SdkBuilder`](crate::SdkBuilder), which cannot itself cross the FFI: a
/// builder hands out `Self` by value, and UniFFI objects cross as `Arc`.
#[uniffi::export(async_runtime = "tokio")]
pub async fn create_fedimint_sdk(
    data_dir: String,
    mnemonic: Option<Arc<Mnemonic>>,
) -> Result<Arc<Sdk>> {
    let mut builder = Sdk::builder().storage(Storage::at(&data_dir)?);
    if let Some(mnemonic) = mnemonic {
        // `Mnemonic` crosses as an opaque object, so the binding hands over an `Arc`; the builder
        // wants it by value and `Mnemonic` is a cheap clone.
        builder = builder.mnemonic(Mnemonic::clone(&mnemonic));
    }
    Ok(Arc::new(builder.build().await?))
}

// The UniFFI view of `Sdk::federations`/`Sdk::federation`, under their real names but different
// Rust identifiers so they can wrap each `Federation` in `Arc`: a UniFFI object nested in
// `Vec`/`Option` has to cross as `Arc<T>`, and the real methods stay free of it since nothing about
// the plain-Rust API needs it. `federation` returns `Result` although the real one cannot fail:
// `id` is lifted from a string, and only a `Result`-returning export turns a malformed one into
// this crate's `InvalidInput` error rather than UniFFI's internal error.
//
// `Sdk::stored_federations` needs no adapter and keeps the export attribute on the real method in
// `sdk.rs`: `FederationInfo` is a UniFFI record and the `FederationStatus` it carries crosses as
// itself, and the call takes no argument that could fail to lift.
#[uniffi::export]
impl Sdk {
    /// See [`Sdk::federations`].
    #[uniffi::method(name = "federations")]
    pub fn ffi_federations(&self) -> Vec<Arc<Federation>> {
        self.federations().into_iter().map(Arc::new).collect()
    }

    /// See [`Sdk::federation`].
    #[uniffi::method(name = "federation")]
    pub fn ffi_federation(&self, id: &FederationId) -> Result<Option<Arc<Federation>>> {
        Ok(self.federation(id).map(Arc::new))
    }

    // `federation_status` exists only to return `Result`, for the same reason `federation` does:
    // `id` is lifted from a string, and only a `Result`-returning export turns a malformed one into
    // this crate's `InvalidInput` error. The status itself needs no adaptation —
    // `FederationStatus` and the `Diagnostic` its `Quarantined` variant carries both cross as
    // themselves.
    /// See [`Sdk::federation_status`].
    #[uniffi::method(name = "federation_status")]
    pub fn ffi_federation_status(&self, id: &FederationId) -> Result<Option<FederationStatus>> {
        Ok(self.federation_status(id))
    }
}

// The UniFFI view of `FederationStatusUpdates::next`, under its real name but a different Rust
// identifier: the real signature takes `&mut self`, which a shared `Arc<FederationStatusUpdates>`
// can never provide, so this calls the same `next_shared` body through `&self` instead. The yielded
// `FederationInfo` is the real record, unprojected.
//
// `Sdk::federation_status_updates` itself needs no adapter and keeps the export attribute on the
// real method in `sdk.rs`: a bare object return crosses with nothing to adapt.
#[uniffi::export(async_runtime = "tokio")]
impl FederationStatusUpdates {
    /// See [`FederationStatusUpdates::next`].
    #[uniffi::method(name = "next")]
    pub async fn ffi_next(&self) -> Result<FederationInfo> {
        self.next_shared().await
    }
}
