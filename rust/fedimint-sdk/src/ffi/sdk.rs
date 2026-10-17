//! The SDK handle's FFI adapters, and the UniFFI entry point.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::{
    DiscoveredFederation, Federation, FederationId, FederationInfo, FederationStatus,
    FederationStatusUpdates, InviteCode, Mnemonic, Network, Result, Sdk, Storage,
};

/// Opens an instance over `data_dir`: on native targets a directory path, in a browser the name
/// of an origin-scoped store (1 to 64 characters of letters, digits, `-`, `_` or `.`).
///
/// Pass a `mnemonic` ([`Mnemonic::from_words`] / [`Mnemonic::generate`]) to establish that seed.
/// Pass `None` to use the seed the storage already holds, or, over storage proven empty, to
/// generate and persist a fresh one. The failure modes are exactly those of
/// [`SdkBuilder::build`](crate::SdkBuilder::build).
///
/// The flattened form of [`SdkBuilder`](crate::SdkBuilder), which cannot itself cross the FFI: a
/// builder hands out `Self` by value, and UniFFI objects cross as `Arc`.
#[cfg_attr(not(target_family = "wasm"), uniffi::export(async_runtime = "tokio"))]
#[cfg_attr(target_family = "wasm", uniffi::export)]
pub async fn create_fedimint_sdk(
    data_dir: String,
    mnemonic: Option<Arc<Mnemonic>>,
) -> Result<Arc<Sdk>> {
    // A Kotlin host has no way to install a Rust log subscriber, or to hand the SDK the `Context`
    // its DNS resolver needs, so on Android the SDK does both itself, here: the first call a host
    // is certain to make, and one made once its `Application` exists. Idempotent. See
    // `crate::android`.
    #[cfg(target_os = "android")]
    {
        crate::android::init_logging();
        crate::android::publish_context();
    }
    // A Swift host cannot install a Rust log subscriber either, so on Apple targets the SDK
    // installs one, with a panic hook, at the same point. Nothing needs publishing there. See
    // `crate::apple`.
    #[cfg(target_vendor = "apple")]
    crate::apple::init_logging();

    #[cfg(not(target_family = "wasm"))]
    let storage = Storage::at(&data_dir)?;
    #[cfg(target_family = "wasm")]
    let storage = Storage::in_browser(&data_dir)?;
    let mut builder = Sdk::builder().storage(storage);
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
#[cfg_attr(not(target_family = "wasm"), uniffi::export(async_runtime = "tokio"))]
#[cfg_attr(target_family = "wasm", uniffi::export)]
impl FederationStatusUpdates {
    /// See [`FederationStatusUpdates::next`].
    #[uniffi::method(name = "next")]
    pub async fn ffi_next(&self) -> Result<FederationInfo> {
        self.next_shared().await
    }
}

/// The UniFFI view of [`crate::DiscoverySource`].
#[derive(Debug, Clone, uniffi::Enum)]
pub enum FfiDiscoverySource {
    /// Dev mode: returns a hardcoded local/test federation without network queries.
    Dev,
    /// Production mode: queries the specified Nostr relays for NIP-87 announcements.
    Nostr {
        /// Nostr relay websocket URLs (e.g. `["wss://relay.damus.io"]`).
        relays: Vec<String>,
    },
}

impl From<FfiDiscoverySource> for crate::DiscoverySource {
    fn from(source: FfiDiscoverySource) -> Self {
        match source {
            FfiDiscoverySource::Dev => crate::DiscoverySource::Dev,
            FfiDiscoverySource::Nostr { relays } => crate::DiscoverySource::Nostr { relays },
        }
    }
}

// The UniFFI view of `Sdk::discover_federations`, under its real name but a different Rust
// identifier: `DiscoveredFederation` carries a bare `InviteCode`, which is a UniFFI object and
// must cross nested in a record behind `Arc<InviteCode>`.
#[cfg_attr(not(target_family = "wasm"), uniffi::export(async_runtime = "tokio"))]
#[cfg_attr(target_family = "wasm", uniffi::export)]
impl Sdk {
    /// See [`Sdk::discover_federations`].
    #[uniffi::method(name = "discover_federations")]
    pub async fn ffi_discover_federations(
        &self,
        source: FfiDiscoverySource,
    ) -> Result<Vec<FfiDiscoveredFederation>> {
        let federations = self.discover_federations(source.into()).await?;
        Ok(federations.into_iter().map(Into::into).collect())
    }
}

/// The UniFFI view of [`DiscoveredFederation`], with `invite` crossing as `Arc<InviteCode>`:
/// [`InviteCode`] is a UniFFI object, and an object can sit in a record only behind an `Arc`.
/// Exported as `DiscoveredFederation`; the real type never crosses.
#[derive(Debug, uniffi::Record)]
#[uniffi(name = "DiscoveredFederation")]
pub struct FfiDiscoveredFederation {
    /// See [`DiscoveredFederation::id`].
    pub id: FederationId,
    /// See [`DiscoveredFederation::invite`].
    pub invite: Arc<InviteCode>,
    /// See [`DiscoveredFederation::name`].
    pub name: Option<String>,
    /// See [`DiscoveredFederation::network`].
    pub network: Option<Network>,
    /// See [`DiscoveredFederation::modules`].
    pub modules: Vec<String>,
    /// See [`DiscoveredFederation::metadata`].
    pub metadata: BTreeMap<String, String>,
}

impl From<DiscoveredFederation> for FfiDiscoveredFederation {
    fn from(fed: DiscoveredFederation) -> Self {
        // Destructured with no `..`, so a field added to `DiscoveredFederation` fails to compile
        // here until this projection carries it too.
        let DiscoveredFederation {
            id,
            invite,
            name,
            network,
            modules,
            metadata,
        } = fed;
        Self {
            id,
            invite: Arc::new(invite),
            name,
            network,
            modules,
            metadata,
        }
    }
}
