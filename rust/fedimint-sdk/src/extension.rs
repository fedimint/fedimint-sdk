//! Plugging client modules this crate does not ship into an SDK instance.
//!
//! Behind the `extension-api` feature, off by default.

use std::sync::Arc;

use fedimint_client::ClientHandleArc;
use fedimint_client::module_init::DynClientModuleInit;
use fedimint_core::task::{MaybeSend, MaybeSync};
use fedimint_core::util::BoxFuture;

use crate::{Federation, MultisigBackend};

/// What another crate plugs into an [`Sdk`](crate::Sdk) instance: client modules this crate does
/// not ship, and work that runs on each client a federation starts.
///
/// Register one with [`SdkBuilder::extension`](crate::SdkBuilder::extension). Its modules then
/// run in every federation whose configuration has a module of their kind, alongside the ones
/// this crate ships, and the extension reaches them through [`Federation::client`].
///
/// # Versioning
///
/// An extension works with fedimint's own types, so it must be built against exactly the fedimint
/// release this crate is built against: require this crate with `=`, and every fedimint crate at
/// the version this crate's manifest requires. This part of the API follows that release rather
/// than this crate's semver, and any release of this crate may change it.
pub trait Extension: MaybeSend + MaybeSync + 'static {
    /// Client modules to add to every federation's client, beside the ones this crate ships.
    ///
    /// Asked once, by [`SdkBuilder::build`](crate::SdkBuilder::build). A module kind this crate
    /// already ships, or one that another extension of the same instance also adds, fails that
    /// build with [`InvalidInput`](crate::ErrorCode::InvalidInput).
    ///
    /// A module only runs in a federation whose configuration has a module of its kind; for every
    /// other federation it is never initialised.
    fn module_inits(&self) -> Vec<DynClientModuleInit> {
        Vec::new()
    }

    /// Called with each `client` one of the instance's federations starts: when the federation
    /// is joined or recovered, when it is reopened, when
    /// [`SdkBuilder::build`](crate::SdkBuilder::build) brings it back, and when the SDK replaces
    /// its client, as it does when a recovery ends or is retried. It is called once for each
    /// client.
    ///
    /// The federation is open when this is called, either
    /// [`Running`](crate::FederationStatus::Running) or still
    /// [`Recovering`](crate::FederationStatus::Recovering). While it recovers, a module that
    /// cannot be used before its recovery ends is missing from the client, and is there in the
    /// client that replaces it.
    ///
    /// What it returns is the extension's work on `client`, picking up its own unfinished work
    /// included, and the SDK runs it. The work may hold `client` for as long as it runs. The SDK
    /// drops it, finished or not, when it stops the client and when the instance is shut down
    /// or dropped, and the replacement client, if there is one, comes through here again.
    ///
    /// Keep no clone of `client` anywhere else. One that outlives the client keeps it from
    /// shutting down cleanly, as [`Federation::client`] explains, and one held by work the SDK
    /// does not run also keeps the instance's storage claimed once the application has dropped
    /// the instance, until that work ends.
    ///
    /// It is called while the instance holds the lock that orders its lifecycle calls, so it must
    /// return without waiting for anything. Calling [`Sdk::join`](crate::Sdk::join), for
    /// instance, or any other lifecycle call from here and waiting for it would wait for ever.
    /// The work it returns runs apart from that lock, and may wait.
    fn client_started(
        &self,
        federation: &Federation,
        client: &ClientHandleArc,
    ) -> BoxFuture<'static, ()> {
        Box::pin(std::future::ready(()))
    }

    /// The module the [`Multisig`](crate::Multisig) facade runs on, if this extension provides
    /// one.
    ///
    /// Asked once, by [`SdkBuilder::build`](crate::SdkBuilder::build); an instance has one
    /// multisig backend at most, and a second one fails that build with
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput). The facade is then available on every
    /// federation whose configuration has a module of the backend's
    /// [`module_kind`](MultisigBackend::module_kind).
    fn multisig(&self) -> Option<Arc<dyn MultisigBackend>> {
        None
    }
}
