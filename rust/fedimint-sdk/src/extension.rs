//! Plugging client modules this crate does not ship into an SDK instance.
//!
//! Behind the `extension-api` feature, off by default.

use fedimint_client::module_init::DynClientModuleInit;
use fedimint_core::task::{MaybeSend, MaybeSync};

use crate::Federation;

/// What another crate plugs into an [`Sdk`](crate::Sdk) instance: client modules this crate does
/// not ship, and code that runs each time a federation comes up.
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

    /// Called each time one of the instance's federations comes up: when it is joined or
    /// recovered, when it is reopened, and when [`SdkBuilder::build`](crate::SdkBuilder::build)
    /// brings it back.
    ///
    /// The federation is open when this is called, either
    /// [`Running`](crate::FederationStatus::Running) or still
    /// [`Recovering`](crate::FederationStatus::Recovering). This is the place to pick up the
    /// extension's own unfinished work.
    ///
    /// It is called while the instance holds the lock that orders its lifecycle calls, so it must
    /// return without waiting for anything: start long work as a task of its own instead
    /// (`fedimint_core::task::spawn`). Calling [`Sdk::join`](crate::Sdk::join), for instance, or
    /// any other lifecycle call from here and waiting for it would wait for ever.
    fn federation_opened(&self, federation: &Federation) {}
}
