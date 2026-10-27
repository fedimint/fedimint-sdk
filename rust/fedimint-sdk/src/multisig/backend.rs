//! What a module has to provide for the multisig facade to run on it.

use fedimint_client::Client;
use fedimint_core::core::OperationId as UpstreamOperationId;
use fedimint_core::secp256k1::{Message, schnorr};
use fedimint_core::task::{MaybeSend, MaybeSync};
use fedimint_core::util::{BoxFuture, BoxStream};

use crate::{
    Amount, Approval, MemberKey, MultisigAccount, MultisigAmount, MultisigDenomination,
    MultisigDepositState, MultisigSweepState, MultisigTransferState, Proposal, Result,
};

/// A module the [`Multisig`](crate::Multisig) facade runs on.
///
/// An [`Extension`](crate::Extension) hands one to the instance from
/// [`Extension::multisig`](crate::Extension::multisig). The facade then exists on every federation
/// whose configuration has a module of the backend's
/// [`module_kind`](MultisigBackend::module_kind), and everything an application sees is the
/// facade's: the checks on policies, proposals and approvals, the operations and their records,
/// activity history, and the bindings. The backend only does what needs the module.
///
/// # The client
///
/// Every method that needs the federation is handed its client. The facade holds the federation
/// open for as long as the call runs, so a close waits for it to finish. Use that client, and do
/// not ask for another one with [`Federation::client`](crate::Federation::client) from inside a
/// call: a close that starts in between would wait for this call while this call waits for it.
///
/// # Operations
///
/// [`deposit`](MultisigBackend::deposit), [`execute`](MultisigBackend::execute) and
/// [`sweep`](MultisigBackend::sweep) start an operation in the module and return its id. The
/// facade records it, and from then on follows it through the matching `*_updates` method, in this
/// process and after every restart. Each of those returns a stream that:
///
/// - yields the operation's current state first, and then every change of state, ending after the
///   first final one;
/// - waits for nothing before it is first polled, so that asking for it costs one bounded read;
/// - holds no clone of the client, so it never keeps the federation from closing.
///
/// A stream that ends without a final state is asked for again. An operation that has not ended
/// when the application stops is followed again only by a build with a backend for the same
/// module.
pub trait MultisigBackend: MaybeSend + MaybeSync + 'static {
    /// The kind of the module this backend runs on.
    fn module_kind(&self) -> String;

    /// The denominations this backend can hold an account in.
    fn denominations(&self) -> Vec<MultisigDenomination>;

    /// This member's key for `group`.
    ///
    /// The same `group` always gives the same key, and different groups give unrelated keys.
    fn member_key(&self, client: &Client, group: &str) -> Result<MemberKey>;

    /// This member's signature over `message`, as the member of `group`: a BIP-340 Schnorr
    /// signature by the secret key behind [`member_key`](MultisigBackend::member_key) for
    /// `group`.
    ///
    /// The facade signs every proposal it makes this way, so that the other members can tell who
    /// made it. `message` is a digest under a domain of the facade's own, so the signature is good
    /// for nothing the module signs.
    fn sign(&self, client: &Client, group: &str, message: Message) -> Result<schnorr::Signature>;

    /// What `account` holds, in its denomination.
    ///
    /// Zero for an account the federation has never seen.
    fn balance<'a>(
        &'a self,
        client: &'a Client,
        account: &'a MultisigAccount,
    ) -> BoxFuture<'a, Result<MultisigAmount>>;

    /// Starts moving `amount` from the ecash balance into `account`.
    ///
    /// `amount` is not zero, and the account is in a denomination this backend supports.
    fn deposit<'a>(
        &'a self,
        client: &'a Client,
        account: &'a MultisigAccount,
        amount: Amount,
    ) -> BoxFuture<'a, Result<UpstreamOperationId>>;

    /// What the module needs signed to pay `amount` out of `account` to this member.
    ///
    /// Whatever this returns is carried in the [`Proposal`], and read back with
    /// [`Proposal::payload`]. `amount` is not zero and in the account's denomination, and this
    /// member is one of the account's members, who signs the proposal with
    /// [`sign`](MultisigBackend::sign).
    fn propose<'a>(
        &'a self,
        client: &'a Client,
        account: &'a MultisigAccount,
        amount: &'a MultisigAmount,
    ) -> BoxFuture<'a, Result<Vec<u8>>>;

    /// This member's signature over `proposal`, as the member of `group`.
    ///
    /// Checks first that the proposal's payload pays what its account and amount say, since
    /// those are what the member is shown and agrees to. Whatever this returns is carried in the
    /// [`Approval`], and read back with [`Approval::payload`]. The facade has already checked that
    /// this member's key for `group` is one of the account's members, and that the proposal's
    /// [`proposer`](Proposal::proposer), another of them, signed it.
    fn approve(&self, client: &Client, group: &str, proposal: &Proposal) -> Result<Vec<u8>>;

    /// Starts paying out `proposal`, with `approvals`.
    ///
    /// The facade has already checked that every approval is for this proposal, that each comes
    /// from a different member of the account, and that there are as many as its threshold asks
    /// for.
    fn execute<'a>(
        &'a self,
        client: &'a Client,
        proposal: &'a Proposal,
        approvals: &'a [Approval],
    ) -> BoxFuture<'a, Result<UpstreamOperationId>>;

    /// Starts moving everything paid out to this member into the ecash balance, or `None` when
    /// nothing is waiting.
    fn sweep<'a>(&'a self, client: &'a Client) -> BoxFuture<'a, Result<Option<MultisigSweep>>>;

    /// The states of the deposit `id`, as described under [Operations](#operations).
    fn deposit_updates<'a>(
        &'a self,
        client: &'a Client,
        id: UpstreamOperationId,
    ) -> BoxFuture<'a, Result<BoxStream<'static, Result<MultisigDepositState>>>>;

    /// The states of the payout `id`, as described under [Operations](#operations).
    fn transfer_updates<'a>(
        &'a self,
        client: &'a Client,
        id: UpstreamOperationId,
    ) -> BoxFuture<'a, Result<BoxStream<'static, Result<MultisigTransferState>>>>;

    /// The states of the sweep `id`, as described under [Operations](#operations).
    fn sweep_updates<'a>(
        &'a self,
        client: &'a Client,
        id: UpstreamOperationId,
    ) -> BoxFuture<'a, Result<BoxStream<'static, Result<MultisigSweepState>>>>;
}

/// A sweep a [`MultisigBackend`] started: its operation, and how much it moves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultisigSweep {
    pub(crate) operation: UpstreamOperationId,
    pub(crate) amount: Amount,
}

impl MultisigSweep {
    /// The sweep the module started as `operation`, moving `amount` into the ecash balance.
    pub fn new(operation: UpstreamOperationId, amount: Amount) -> MultisigSweep {
        MultisigSweep { operation, amount }
    }
}
