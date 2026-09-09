//! Chaumian ecash: spending notes out of band and redeeming them.

use std::any::Any;
use std::sync::Arc;

use fedimint_core::util::{BoxFuture, BoxStream};
use futures::StreamExt;
use serde::{Deserialize, Serialize};

use crate::operation::Driver;
use crate::{Amount, Error, ErrorCode, Notes, Operation, OperationState, Result, Timestamp};

/// The ecash facade for one federation.
///
/// Obtained from [`Federation::ecash`](crate::Federation::ecash), which
/// returns `None` when the federation has no mint module.
///
/// Ecash here means *out-of-band* ecash: notes the sender takes out of
/// their balance and hands to a receiver over some channel the federation
/// knows nothing about, a chat message, a QR code, a file. The receiver
/// redeems them against the same federation. Ordinary in-federation
/// spending is not a separate concept; it is what lightning and on-chain
/// operations do with the balance.
///
/// [`Ecash::quote`] plans a send and [`Ecash::send`] executes that plan,
/// exactly as [`Lightning::quote`](crate::Lightning::quote) and
/// [`Onchain::quote`](crate::Onchain::quote) do for their kinds of value.
/// Receiving is not quoted, because it presents the caller with no
/// decision: see [`Ecash::receive`].
///
/// Every call on this facade, sending and receiving alike, is refused with
/// [`Recovering`](crate::ErrorCode::Recovering) while a recovery for the
/// federation is incomplete. A wallet whose note set was never fully
/// discovered is not safe to spend from, since a note the rescan never
/// reached can be double-spent.
#[derive(Debug, Clone)]
pub struct Ecash {
    inner: Arc<EcashInner>,
}

impl Ecash {
    /// Plans an out-of-band send and returns an executable quote for it.
    ///
    /// The value that leaves the balance is generally *more* than `amount`:
    /// a mint issues notes in fixed denominations, so the receiver ends up
    /// with the smallest value the mint can represent at or above `amount`,
    /// and assembling that value can itself cost a fee. The returned
    /// [`EcashQuote`] is that plan, frozen: it binds the requested amount,
    /// the note value that will actually be produced, the fee and the total
    /// debit. Show it, then hand it back to [`Ecash::send`], which executes
    /// exactly what was shown.
    ///
    /// `amount` is a floor rather than a promise, the least the receiver
    /// must be able to redeem. [`EcashQuote::notes_value`] is what they will
    /// actually be able to redeem, and it is the number to put in front of a
    /// user beside [`EcashQuote::fee`] and [`EcashQuote::total`].
    ///
    /// Quoting neither debits the balance nor records anything: it plans.
    /// Quotes expire; see [`EcashQuote::expires_at`].
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for a zero amount,
    /// which no note can carry,
    /// [`InsufficientBalance`](crate::ErrorCode::InsufficientBalance) when
    /// the balance cannot cover the rounded-up note value plus the fee,
    /// which can happen for an `amount` the balance would have covered
    /// exactly, and is itself a reason for this call to exist,
    /// [`NotSupported`](crate::ErrorCode::NotSupported) if assembling
    /// `amount` would require the wallet to reissue itself change: this
    /// build only ever hands out notes it already holds in the exact
    /// denominations requested, never mints new ones to make change, so a
    /// wallet that cannot represent `amount` from its current notes cannot
    /// send it at all yet (see [`Ecash::send`]'s errors for why), or if the
    /// mint module disappeared from the federation's configuration after
    /// this facade was obtained,
    /// [`Recovering`](crate::ErrorCode::Recovering) while a recovery for
    /// this federation is incomplete,
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn quote(&self, amount: Amount) -> Result<EcashQuote> {
        if amount.msats() == 0 {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "cannot quote a zero amount",
            ));
        }

        self.inner.federation.ensure_open()?;
        let client = self.inner.federation.client(true).await?;
        let mint = client
            .get_first_module::<fedimint_mint_client::MintClientModule>()
            .map_err(|_| {
                Error::new(
                    ErrorCode::NotSupported,
                    "this federation has no mint module",
                )
            })?;

        let module_cfg = client
            .config()
            .await
            .get_module_cfg(mint.id)
            .map_err(|err| Error::new(ErrorCode::Internal, err.to_string()))?;
        // `MintClientConfig` is kept private by `fedimint-mint-client` itself and
        // re-exported nowhere nameable there, so the cast target has to name it at
        // its own defining crate, `fedimint-mint-common` (see the dependency comment
        // in Cargo.toml).
        let mint_cfg: &fedimint_mint_common::config::MintClientConfig = module_cfg
            .cast()
            .map_err(|err| Error::new(ErrorCode::Internal, err.to_string()))?;
        let fee_consensus = mint_cfg.fee_consensus.clone();
        let upstream_amount = fedimint_core::Amount::from_msats(amount.msats());
        let rounded_upstream = fee_consensus.round_up(upstream_amount);
        let notes_value = Amount::from_msats(rounded_upstream.msats);

        // `send_fee_quote` runs the same selection `send` itself will use against the
        // live note inventory, rather than a flat per-amount formula that cannot tell
        // "the wallet already holds exact change" (free) apart from "it would have to
        // reissue itself change" (a fee). It returns `FeeQuote::ZERO` exactly in the
        // first case.
        let fee_quote = mint
            .send_fee_quote(rounded_upstream)
            .await
            .map_err(|err| Error::new(ErrorCode::Internal, err.to_string()))?;
        let fee = Amount::from_msats(fee_quote.total().get_bitcoin().msats);

        // A nonzero fee here means the wallet's current notes cannot cover
        // `notes_value` exactly, so producing it would require a self-reissue.
        // `Ecash::send` has no way to perform that reissue and still hand back a
        // trackable, cancellable operation (see its doc), so refuse here rather than
        // freeze a quote `send` can never actually execute.
        if fee.msats() > 0 {
            return Err(Error::new(
                ErrorCode::NotSupported,
                "sending this amount would require reissuing notes to make exact change, \
                 which this build does not yet support; the wallet must already hold notes \
                 in the exact denominations needed",
            ));
        }

        let total = notes_value
            .checked_add(fee)
            .ok_or_else(|| Error::new(ErrorCode::InvalidInput, "amount and fee overflow u64"))?;

        let balance = self.inner.federation.balance().await?;
        if balance < total {
            return Err(Error::new(
                ErrorCode::InsufficientBalance,
                format!("balance {balance:?} cannot cover total debit {total:?}"),
            ));
        }

        let now = crate::db::now_millis();
        let expires_at = Timestamp::from_epoch_millis(now + 60_000);
        let balance_snapshot_msats = balance.msats();

        Ok(EcashQuote {
            inner: EcashQuoteInner {
                requested_amount: amount,
                notes_value,
                fee,
                total,
                expires_at,
                balance_snapshot_msats,
            },
        })
    }

    /// Executes a quoted send, taking its value out of the balance as
    /// out-of-band notes.
    ///
    /// The quote is consumed: it describes one send and can fund one send.
    /// Execution follows the plan exactly, same note value, same fee, same
    /// total debit, or it does not happen:
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired) if the quote's
    /// validity window has passed,
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged) if something the
    /// quote depends on moved underneath it. Both mean the same thing to a
    /// caller: quote again and re-confirm with the user.
    ///
    /// The balance is debited by [`EcashQuote::total`] and the returned
    /// [`EcashSend::notes`] are ready to hand to a receiver. Until someone
    /// redeems them the value is in limbo: it is no longer spendable by the
    /// sender, and it is not yet the receiver's either.
    ///
    /// # Automatic reclaim
    ///
    /// Notes that go unredeemed do not vanish. The SDK schedules an
    /// automatic reclaim, so a send to someone who never opens the message
    /// eventually returns to the sender's balance instead of being lost. The
    /// moment it is scheduled for is persisted as
    /// [`EcashSendDetails::reclaim_at`], so an application that restarted can
    /// still say when the notes stop being redeemable. Its outcome is
    /// reported as an operation state, like any other:
    /// [`EcashSendState::Canceled`] when the reclaim wins,
    /// [`EcashSendState::Redeemed`] when the receiver got there first.
    ///
    /// # Errors
    ///
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired),
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged) if the balance or
    /// the note inventory the quote was computed against no longer matches:
    /// the total dropped, or the specific denominations needed to hand out
    /// exactly [`EcashQuote::notes_value`] are no longer available even
    /// though the total is unchanged (spent and received in the meantime by
    /// some other operation). Both mean the same thing: quote again,
    /// [`InsufficientBalance`](crate::ErrorCode::InsufficientBalance),
    /// [`Recovering`](crate::ErrorCode::Recovering) while a recovery for
    /// this federation is incomplete,
    /// [`NotSupported`](crate::ErrorCode::NotSupported) if the mint module
    /// disappeared from the federation's configuration after this facade
    /// was obtained,
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`Storage`](crate::ErrorCode::Storage), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn send(&self, quote: EcashQuote) -> Result<EcashSend> {
        self.inner.federation.ensure_open()?;
        let now_millis = crate::db::now_millis();
        let now = Timestamp::from_epoch_millis(now_millis);

        if now >= quote.expires_at() {
            return Err(Error::new(
                ErrorCode::QuoteExpired,
                "this quote has expired",
            ));
        }

        let client = self.inner.federation.client(true).await?;
        let mint = client
            .get_first_module::<fedimint_mint_client::MintClientModule>()
            .map_err(|_| {
                Error::new(
                    ErrorCode::NotSupported,
                    "this federation has no mint module",
                )
            })?;

        // A cheap early exit for the common case. This is not a note-composition
        // check: the total can stay identical while the specific denominations
        // available change (spent and received back in the meantime). The exact
        // selection below is what actually verifies the plan is still realizable;
        // this only saves a doomed selection attempt when the balance alone already
        // rules the quote out.
        let current_balance = self.inner.federation.balance().await?;
        if current_balance < quote.total()
            || current_balance.msats() != quote.inner.balance_snapshot_msats
        {
            return Err(Error::new(
                ErrorCode::QuoteChanged,
                "balance changed since quote was created",
            ));
        }

        let timeout = std::time::Duration::from_secs(86_400);
        let upstream_notes_val = fedimint_core::Amount::from_msats(quote.notes_value().msats());
        // Exact selection: the quote promised `notes_value`, and this must produce
        // exactly that or fail, never more. `spend_notes_with_selector` never itself
        // reissues to make change regardless of selector (only
        // `MintClientModule::send_oob_notes` does that, and it returns no operation
        // id this facade could track for cancellation), so `quote` already refused to
        // freeze a plan needing one; a failure to select exactly `notes_value` here
        // means the note inventory changed since then; a race, not a capability gap.
        let (operation_id, oob_notes) = mint
            .spend_notes_with_selector(
                &fedimint_mint_client::SelectNotesWithExactAmount,
                upstream_notes_val,
                Some(timeout),
                true,
                serde_json::Value::Null,
            )
            .await
            .map_err(|_| {
                Error::new(
                    ErrorCode::QuoteChanged,
                    "note inventory changed since quote was created",
                )
            })?;

        let notes = Notes::from_upstream(oob_notes);
        let created_at = now;
        let reclaim_at = Timestamp::from_epoch_millis(now_millis + 86_400_000);

        let details = EcashSendDetails {
            notes: notes.clone(),
            requested_amount: quote.requested_amount(),
            notes_value: quote.notes_value(),
            fee: quote.fee(),
            total_debited: quote.total(),
            reclaim_at,
            created_at,
        };

        let wire = EcashSendDetailsWire::from(&details);
        let driver = Arc::new(EcashSendDriver);

        let operation = self
            .inner
            .federation
            .create_operation(
                operation_id,
                crate::operation::kinds::ECASH_SEND,
                "mint",
                &wire,
                driver,
            )
            .await?;

        Ok(EcashSend { notes, operation })
    }

    /// Redeems out-of-band notes into this federation's balance.
    ///
    /// The notes are reissued as fresh notes belonging to this client,
    /// which is what makes the redemption final and unlinkable to the
    /// sender's copy. The returned operation tracks that;
    /// [`EcashReceiveState::Done`] is the point at which the value is
    /// spendable.
    ///
    /// There is no quote on this side, because a redemption presents the
    /// caller with no decision: the notes carry the value they carry, and
    /// the reissuance fee comes out of it rather than being charged on top of
    /// it. The gross value, the fee and the net credit are all recorded in
    /// [`EcashReceiveDetails`] before this call returns, so a receipt never
    /// depends on having watched the operation.
    ///
    /// Redeem promptly. Notes are subject to the sender's automatic reclaim
    /// (see [`Ecash::send`]), and losing the race means the operation ends
    /// in [`EcashReceiveState::Failed`].
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) if the notes are
    /// malformed or were issued by a different federation,
    /// [`Recovering`](crate::ErrorCode::Recovering) while a recovery for
    /// this federation is incomplete,
    /// [`NotSupported`](crate::ErrorCode::NotSupported),
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`Storage`](crate::ErrorCode::Storage), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn receive(&self, notes: &Notes) -> Result<Operation<EcashReceiveState>> {
        self.inner.federation.ensure_open()?;

        if notes.value().msats() == 0 {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "cannot receive notes with zero value",
            ));
        }

        let expected_prefix = self.inner.federation.id.to_prefix().to_string();
        if notes.federation_id_prefix() != expected_prefix {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "these notes were issued by a different federation",
            ));
        }

        let client = self.inner.federation.client(true).await?;
        let mint = client
            .get_first_module::<fedimint_mint_client::MintClientModule>()
            .map_err(|_| {
                Error::new(
                    ErrorCode::NotSupported,
                    "this federation has no mint module",
                )
            })?;

        // `reissue_fee_quote` sums the fee per input note the real reissue will
        // submit, rather than one flat fee on the notes' combined value: a token
        // made of several notes pays more than a single note carrying the same
        // total, and only the per-note sum reflects that.
        let fee_quote = mint
            .reissue_fee_quote(notes.as_upstream())
            .await
            .map_err(|err| Error::new(ErrorCode::Internal, err.to_string()))?;
        let fee = Amount::from_msats(fee_quote.total().get_bitcoin().msats);
        let net_credit = notes
            .value()
            .checked_sub(fee)
            .ok_or_else(|| Error::new(ErrorCode::InvalidInput, "fee exceeds note value"))?;

        let operation_id = mint
            .reissue_external_notes(notes.to_upstream(), serde_json::Value::Null)
            .await
            .map_err(|err| Error::new(ErrorCode::Internal, err.to_string()))?;

        let created_at = Timestamp::from_epoch_millis(crate::db::now_millis());
        let details = EcashReceiveDetails {
            notes: Some(notes.clone()),
            notes_value: notes.value(),
            fee,
            net_credit,
            created_at,
        };

        let wire = EcashReceiveDetailsWire::from(&details);
        let driver = Arc::new(EcashReceiveDriver);

        self.inner
            .federation
            .create_operation(
                operation_id,
                crate::operation::kinds::ECASH_RECEIVE,
                "mint",
                &wire,
                driver,
            )
            .await
    }

    /// Builds the facade for one federation. Handed out by `Federation::ecash`.
    pub(crate) fn new(federation: Arc<crate::federation::FederationInner>) -> Ecash {
        Ecash {
            inner: Arc::new(EcashInner { federation }),
        }
    }
}

/// A frozen, executable plan for one out-of-band ecash send.
///
/// Produced by [`Ecash::quote`] and consumed by [`Ecash::send`]. As with
/// [`LnQuote`](crate::LnQuote) and [`OnchainQuote`](crate::OnchainQuote),
/// the accessors expose exactly what a user must approve: display these
/// numbers, then give the quote back.
///
/// The requested amount and the actual note value can differ, and this is
/// the ordinary case rather than an edge case: a mint issues notes in fixed
/// denominations (mintv2 rounds up to a multiple of 512 msat), so a request
/// is satisfied with notes worth at least as much, never less. Show
/// [`total`](EcashQuote::total) before the user agrees, because that is the
/// number their balance moves by.
///
/// The resolved note value is quoted once here and appears nowhere in the
/// send's progress stream, so this executed quote is what
/// [`EcashSendDetails`] copies its terms from, for the whole life of the
/// operation and after a restart.
#[derive(Debug)]
pub struct EcashQuote {
    inner: EcashQuoteInner,
}

impl EcashQuote {
    /// The amount [`Ecash::quote`] was asked for.
    ///
    /// Kept so that a confirmation screen or a receipt can show what was
    /// requested next to what will actually be issued. It is a floor, and it
    /// is not the figure the balance moves by; see [`EcashQuote::total`].
    pub fn requested_amount(&self) -> Amount {
        self.inner.requested_amount
    }

    /// The value the notes will actually carry, what the receiver can
    /// redeem.
    ///
    /// At or above [`EcashQuote::requested_amount`], never below it. This is
    /// the figure activity history reports as an ecash send's
    /// [`amount`](crate::ActivityItem::amount).
    pub fn notes_value(&self) -> Amount {
        self.inner.notes_value
    }

    /// What issuing and selecting those notes will cost, on top of
    /// [`EcashQuote::notes_value`].
    ///
    /// Always zero today: [`Ecash::quote`] refuses with
    /// [`NotSupported`](crate::ErrorCode::NotSupported) rather than freeze a
    /// quote that would need the wallet to reissue itself change to
    /// assemble the value, since [`Ecash::send`] has no way to perform that
    /// reissue yet. The field stays, rather than being removed, because a
    /// future build that can perform that reissue will report its real cost
    /// here without changing this type's shape.
    pub fn fee(&self) -> Amount {
        self.inner.fee
    }

    /// The total amount that will be debited from the balance:
    /// [`EcashQuote::notes_value`] plus [`EcashQuote::fee`].
    ///
    /// This is the number to show as "you will pay".
    pub fn total(&self) -> Amount {
        self.inner.total
    }

    /// When this quote stops being executable.
    ///
    /// Past this point [`Ecash::send`] fails with
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired). A quote can also
    /// stop being executable before this point, if notes it planned to
    /// spend are spent by another operation in the meantime; that is
    /// reported as [`QuoteChanged`](crate::ErrorCode::QuoteChanged). The
    /// remedy for both is the same: quote again and re-confirm.
    pub fn expires_at(&self) -> Timestamp {
        self.inner.expires_at
    }
}

/// The result of [`Ecash::send`]: the notes to hand over, and the operation
/// that tracks what happens to them.
///
/// Both halves matter. The notes are what the sender transmits; the
/// operation is how the sender learns whether they were redeemed or came
/// back. Dropping the operation does not stop the reclaim timer, it keeps
/// running in the background like any other operation.
///
/// Everything here is also persisted before [`Ecash::send`] returns, and
/// readable afterwards through
/// [`Operation::details`](crate::Operation::details) as an
/// [`EcashSendDetails`], from the operation id alone, in a later process,
/// with nobody having kept this struct. That is what makes an out-of-band
/// send survivable: a sender whose application dies between issuing the
/// notes and delivering them can still find them and still hand them over,
/// instead of holding value nobody can redeem until the reclaim fires.
#[derive(Debug)]
#[non_exhaustive]
pub struct EcashSend {
    /// The notes to give to the receiver. Their value is already out of the
    /// sender's spendable balance, and it is [`EcashQuote::notes_value`],
    /// the value the mint actually issued, not the amount that was
    /// requested.
    ///
    /// The same notes are persisted as [`EcashSendDetails::notes`] and can be
    /// read back after a restart; this field is the copy the creating call
    /// hands over so that the common path needs no second lookup.
    pub notes: Notes,
    /// Tracks redemption, cancellation, and automatic reclaim.
    pub operation: Operation<EcashSendState>,
}

impl Operation<EcashSendState> {
    /// Asks for the notes back, before the receiver redeems them.
    ///
    /// `Ok(())` means the cancellation intent has been committed to local
    /// storage and will survive a restart or a period offline. It does not
    /// mean the federation has been contacted, that a reclaim has been
    /// attempted, or that the notes came back: the SDK pursues the request
    /// in the background from here, so a device offline at the moment of
    /// the call still reclaims once it comes back online.
    ///
    /// The outcome arrives where every other outcome does, as a state:
    /// [`EcashSendState::Canceled`] if the notes came back,
    /// [`EcashSendState::Redeemed`] if the receiver got them first. Between
    /// the request and the outcome the operation sits in
    /// [`EcashSendState::CancelRequested`]. The receiver may be redeeming at
    /// this very moment, and only the federation decides who wins that race.
    ///
    /// Calling this on a send that already reached a final state
    /// ([`EcashSendState::Canceled`] or [`EcashSendState::Redeemed`]) is not
    /// an error: it returns `Ok(())` and does nothing, since no cancellation
    /// is pending and the outcome is already recorded in the state.
    ///
    /// # Errors
    ///
    /// Only failures that stop the intent from being recorded at all:
    /// [`Storage`](crate::ErrorCode::Storage) if the request cannot be
    /// committed durably, and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) if the
    /// federation was closed or the SDK shut down, leaving nothing to record
    /// it against. An unreachable federation or a slow guardian is not a
    /// failure of this call: the intent is already durable and the SDK
    /// pursues it in the background.
    // The boundary is deliberate: waiting on the network here would let this call return
    // `FederationUnreachable` or `Timeout` after the intent was already durable, leaving the
    // caller unable to tell whether a retry would duplicate a request already in flight.
    // This is the only cancellation in the crate, because it is the only place where
    // cancelling is a real protocol action rather than an attempt to un-send money that has
    // already moved.
    //
    // Only the intent is recorded here. Telling the mint is the ecash driver's job, and it
    // could not be done here in any case: `try_cancel_spend_notes` returns `()` and writes a
    // marker into the module's own isolated database
    // (modules/fedimint-mint-client/src/lib.rs:2556-2563), so it has no result to report and
    // the outcome only ever arrives as a state.
    pub async fn request_cancel(&self) -> Result<()> {
        self.inner().federation.ensure_open()?;
        self.inner().persist_cancel_request().await
    }
}

/// The lifecycle of an out-of-band ecash send.
///
/// An ecash send has exactly two terminal outcomes: the notes came back
/// ([`Canceled`](Self::Canceled)) or the receiver got them
/// ([`Redeemed`](Self::Redeemed)), because those are the only two things
/// that can happen to the money. There is no failure state: if storage
/// cannot be read, no guardian answers, or the federation handle is closed,
/// that is a failure to *observe* the send, reported as `Err` from
/// [`Operation::state`](crate::Operation::state),
/// [`Operation::await_final`](crate::Operation::await_final) or
/// [`OperationUpdates::next`](crate::OperationUpdates::next), not a state
/// of the send itself. The send keeps running, unaffected by the fact that
/// nobody could see it: bearer notes out in the world can still be redeemed
/// or reclaimed long after some call failed to observe them. See
/// [`Sdk::forget_federation`](crate::Sdk::forget_federation), which refuses
/// while reclaimable outgoing value remains.
// Upstream `fedimint-mint-client` models this as `SpendOOBState`: `Created`,
// `UserCanceledProcessing`, `UserCanceledSuccess`, `UserCanceledFailure`, `Success`,
// `Refunded`. Two of those names mean the opposite of what they suggest read in
// isolation, since they are named from the point of view of the cancellation attempt
// rather than the send: `Success` means the automatic reclaim failed (the receiver
// redeemed), `Refunded` means the reclaim succeeded (the notes returned).
//
// | upstream `SpendOOBState`          | here                                        |
// | ---------------------------------- | ------------------------------------------- |
// | `Created`                          | `Created`                                    |
// | `UserCanceledProcessing`           | `CancelRequested`                            |
// | `UserCanceledSuccess`, `Refunded`  | `Canceled`                                   |
// | `UserCanceledFailure`, `Success`   | `Redeemed`                                   |
//
// The mapping is total. The two pairs collapse because upstream's internal
// distinction (asked for vs. timer fired; won against an explicit cancel vs. no
// cancel at all) is about why, not about what happened to the money.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EcashSendState {
    /// The notes have been issued and handed to the caller. The value has
    /// left the spendable balance; nobody has redeemed or reclaimed it
    /// yet.
    Created,
    /// A reclaim has been requested, either by
    /// [`request_cancel`](Operation::request_cancel) or by the automatic
    /// reclaim timer, and is being processed. Not final: the request may
    /// still lose to a redemption.
    CancelRequested,
    /// Final: the notes were reclaimed and their value is back in the
    /// spendable balance.
    Canceled,
    /// Final: the receiver redeemed the notes. The value is theirs; a
    /// cancellation request, if one was made, lost the race.
    Redeemed,
}

impl crate::operation::sealed::Sealed for EcashSendState {}

impl OperationState for EcashSendState {
    fn is_final(&self) -> bool {
        match self {
            EcashSendState::Created | EcashSendState::CancelRequested => false,
            EcashSendState::Canceled | EcashSendState::Redeemed => true,
        }
    }
}

/// What an out-of-band ecash send *is*, as opposed to where it has got to.
///
/// The persisted record for an [`Operation<EcashSendState>`](crate::Operation),
/// read with [`Operation::details`](crate::Operation::details). Every field
/// is fixed when the send is created and never changes afterwards, so an
/// application that restarted before delivering the notes can still display,
/// receipt or hand them over, from the operation id alone.
///
/// # Invariants
///
/// - `total_debited == notes_value + fee`. That is what left the spendable
///   balance.
/// - `notes_value >= requested_amount`. A mint rounds a request up, never
///   down; see [`EcashQuote`] for why.
///
/// `Debug` output redacts the notes, as [`Notes`] itself does.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EcashSendDetails {
    /// The notes handed to the caller: the same value as
    /// [`EcashSend::notes`].
    ///
    /// Kept here because it is the artifact the whole operation exists to
    /// produce, and no state carries it. This record therefore holds
    /// spendable value for as long as the notes are unredeemed: it is the
    /// caller's own bearer artifact, not a secret they did not already have.
    pub notes: Notes,
    /// What the caller asked [`Ecash::quote`] for.
    ///
    /// Kept so that a receipt can show what was requested beside what was
    /// actually issued. It is not the figure the balance moved by, and
    /// activity history deliberately does not report it; see
    /// [`ActivityItem`](crate::ActivityItem)'s note on requested versus
    /// actual.
    pub requested_amount: Amount,
    /// The value the notes actually carry, which is what the receiver can
    /// redeem.
    ///
    /// At or above [`requested_amount`](EcashSendDetails::requested_amount),
    /// because a mint issues fixed denominations and rounds a request up to
    /// one it can represent. This is the figure activity
    /// history reports as an ecash send's
    /// [`amount`](crate::ActivityItem::amount).
    pub notes_value: Amount,
    /// What issuing and selecting those notes cost, on top of
    /// [`notes_value`](EcashSendDetails::notes_value).
    ///
    /// Bound by the executed quote, so it is known before the creating call
    /// returns and never fills in later. Zero where the notes already held
    /// could be handed over as they were.
    pub fee: Amount,
    /// What left the spendable balance:
    /// [`notes_value`](EcashSendDetails::notes_value) plus
    /// [`fee`](EcashSendDetails::fee).
    ///
    /// The number the user approved on the quote and the number a receipt
    /// shows.
    pub total_debited: Amount,
    /// When the automatic reclaim is scheduled for.
    ///
    /// Fixed when the send is created and never rewritten, so this is when
    /// the reclaim was *due* rather than when anything happened: a send that
    /// settles early, the receiver redeems, or
    /// [`request_cancel`](Operation::request_cancel) wins, keeps the
    /// schedule it was created with, and the outcome is read from the state.
    /// Before this moment a receiver can redeem freely; from it the reclaim
    /// is under way, and a receiver who has not redeemed is racing it.
    pub reclaim_at: Timestamp,
    /// When the send was created and the balance debited.
    ///
    /// A local clock reading, like [`ActivityItem::time`](crate::ActivityItem::time)
    /// and with the same caveat: the federation does not attest to it, and a
    /// device with a wrong clock records a wrong time here. Good for
    /// ordering and display, not evidence of when anything happened.
    pub created_at: Timestamp,
}

impl crate::operation::sealed::Sealed for EcashSendDetails {}

impl crate::operation::OperationDetails for EcashSendDetails {}

impl crate::operation::DetailedOperationState for EcashSendState {
    type Details = EcashSendDetails;
}

/// The lifecycle of redeeming out-of-band ecash notes.
///
/// Maps one-to-one onto upstream `fedimint-mint-client`'s
/// `ReissueExternalNotesState` (`Created`, `Issuing`, `Done`,
/// `Failed(String)`); the only change is carrying the failure reason as a
/// named field rather than a positional tuple.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EcashReceiveState {
    /// The redemption has been accepted locally and is about to be
    /// submitted to the federation.
    Created,
    /// The federation is reissuing the notes to this client.
    Issuing,
    /// Final: the notes were reissued and their value is spendable.
    Done,
    /// Final: the notes could not be redeemed, most often because they
    /// were already spent or had been reclaimed by the sender.
    Failed {
        /// Human-readable explanation. Diagnostic only, not a stable
        /// contract, and not something to match on.
        reason: String,
    },
}

impl crate::operation::sealed::Sealed for EcashReceiveState {}

impl OperationState for EcashReceiveState {
    fn is_final(&self) -> bool {
        match self {
            EcashReceiveState::Created | EcashReceiveState::Issuing => false,
            EcashReceiveState::Done | EcashReceiveState::Failed { .. } => true,
        }
    }
}

/// What an ecash redemption *is*, as opposed to where it has got to.
///
/// The persisted record for an
/// [`Operation<EcashReceiveState>`](crate::Operation), read with
/// [`Operation::details`](crate::Operation::details). Every field is fixed
/// when the redemption is created and never changes afterwards.
/// [`EcashReceiveState`] carries no amounts, only a diagnostic reason on
/// failure, so this record is the whole of what a redemption can be
/// receipted from. The fee is known and recorded before the federation
/// answers: the notes state their own value, and the federation's fee
/// schedule is part of the configuration this client already holds.
///
/// # Invariants
///
/// - `net_credit == notes_value - fee`. That is what the balance rises by
///   when the operation reaches [`EcashReceiveState::Done`]. The fee comes
///   out of the notes rather than being charged on top of them, which is
///   why a receive nets down where a send totals up.
///
/// `Debug` output redacts the notes, as [`Notes`] itself does.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EcashReceiveDetails {
    /// The notes this redemption consumed, the ones handed to
    /// [`Ecash::receive`].
    ///
    /// Kept because no state carries them and a redemption that has to be
    /// looked up by id must still be able to say which notes it was about: to
    /// receipt a success, to diagnose an [`EcashReceiveState::Failed`] that
    /// lost the race against the sender's reclaim, or to recognise a second
    /// submission of the same notes. While the redemption is pending these
    /// are still bearer value, which is the other reason [`Notes`] redacts
    /// its own `Debug`.
    ///
    /// `None` for a record [`Ecash::receive`] did not itself create: one
    /// reconstructed by reconciliation from the upstream mint's own
    /// operation log after this build never observed the original call, for
    /// instance after a crash between the federation accepting the
    /// reissuance and this SDK persisting its own record. Upstream's log
    /// entry for a reissuance does not retain the notes that funded it, only
    /// the resulting amount, so there is no bearer string to recover here;
    /// this is the honest absence of that data, not a decode failure.
    pub notes: Option<Notes>,
    /// The gross face value redeemed, before the reissuance fee.
    ///
    /// This is the figure activity history reports as an ecash receive's
    /// [`amount`](crate::ActivityItem::amount), and it is what the sender
    /// gave up, not what this wallet gains; see
    /// [`net_credit`](EcashReceiveDetails::net_credit).
    pub notes_value: Amount,
    /// The reissuance fee, taken out of
    /// [`notes_value`](EcashReceiveDetails::notes_value) rather than charged
    /// on top of it.
    pub fee: Amount,
    /// What the balance rises by:
    /// [`notes_value`](EcashReceiveDetails::notes_value) minus
    /// [`fee`](EcashReceiveDetails::fee).
    ///
    /// The number to show as "you received".
    pub net_credit: Amount,
    /// When the redemption was created.
    ///
    /// A local clock reading, with the same caveat as
    /// [`EcashSendDetails::created_at`].
    pub created_at: Timestamp,
}

impl crate::operation::sealed::Sealed for EcashReceiveDetails {}

impl crate::operation::OperationDetails for EcashReceiveDetails {}

impl crate::operation::DetailedOperationState for EcashReceiveState {
    type Details = EcashReceiveDetails;
}

/// The federation this facade operates on.
///
/// Held rather than a mint-module handle, because a facade outlives the client behind it: a call
/// on a closed federation has to report `FederationClosed` rather than find nothing to talk to.
#[derive(Debug)]
struct EcashInner {
    federation: Arc<crate::federation::FederationInner>,
}

/// The frozen plan for one out-of-band ecash send: the requested amount, the
/// note value selected, the fee, the total debit, when the quote expires, and
/// the inventory context it was computed against.
#[derive(Debug, Clone)]
struct EcashQuoteInner {
    requested_amount: Amount,
    notes_value: Amount,
    fee: Amount,
    total: Amount,
    expires_at: Timestamp,
    /// The balance [`Ecash::quote`] read while computing this quote, in
    /// msats. Not a hash of note composition: it only lets [`Ecash::send`]
    /// notice the total dropped before attempting a doomed selection. The
    /// exact-amount selection `send` performs is what actually verifies the
    /// specific denominations are still there.
    balance_snapshot_msats: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct EcashSendDetailsWire {
    pub(crate) notes: String,
    pub(crate) requested_amount_msats: u64,
    pub(crate) notes_value_msats: u64,
    pub(crate) fee_msats: u64,
    pub(crate) total_debited_msats: u64,
    pub(crate) reclaim_at_epoch_ms: u64,
    pub(crate) created_at_epoch_ms: u64,
}

impl From<&EcashSendDetails> for EcashSendDetailsWire {
    fn from(details: &EcashSendDetails) -> Self {
        Self {
            notes: details.notes.to_string(),
            requested_amount_msats: details.requested_amount.msats(),
            notes_value_msats: details.notes_value.msats(),
            fee_msats: details.fee.msats(),
            total_debited_msats: details.total_debited.msats(),
            reclaim_at_epoch_ms: details.reclaim_at.epoch_millis(),
            created_at_epoch_ms: details.created_at.epoch_millis(),
        }
    }
}

impl TryFrom<EcashSendDetailsWire> for EcashSendDetails {
    type Error = Error;

    fn try_from(wire: EcashSendDetailsWire) -> Result<Self> {
        let notes = wire.notes.parse::<Notes>()?;
        Ok(Self {
            notes,
            requested_amount: Amount::from_msats(wire.requested_amount_msats),
            notes_value: Amount::from_msats(wire.notes_value_msats),
            fee: Amount::from_msats(wire.fee_msats),
            total_debited: Amount::from_msats(wire.total_debited_msats),
            reclaim_at: Timestamp::from_epoch_millis(wire.reclaim_at_epoch_ms),
            created_at: Timestamp::from_epoch_millis(wire.created_at_epoch_ms),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct EcashReceiveDetailsWire {
    /// `None` when the notes are not recoverable, as for a record
    /// [`EcashBackfiller`] reconstructed rather than one [`Ecash::receive`]
    /// created directly; see [`EcashReceiveDetails::notes`].
    pub(crate) notes: Option<String>,
    pub(crate) notes_value_msats: u64,
    pub(crate) fee_msats: u64,
    pub(crate) net_credit_msats: u64,
    pub(crate) created_at_epoch_ms: u64,
}

impl From<&EcashReceiveDetails> for EcashReceiveDetailsWire {
    fn from(details: &EcashReceiveDetails) -> Self {
        Self {
            notes: details.notes.as_ref().map(Notes::to_string),
            notes_value_msats: details.notes_value.msats(),
            fee_msats: details.fee.msats(),
            net_credit_msats: details.net_credit.msats(),
            created_at_epoch_ms: details.created_at.epoch_millis(),
        }
    }
}

impl TryFrom<EcashReceiveDetailsWire> for EcashReceiveDetails {
    type Error = Error;

    fn try_from(wire: EcashReceiveDetailsWire) -> Result<Self> {
        // `None` stays `None`: the notes genuinely were not recoverable. `Some`
        // must still parse as valid notes, exactly like every other decode path;
        // there is no third option that silently substitutes different bearer
        // value for missing data.
        let notes = wire.notes.map(|s| s.parse::<Notes>()).transpose()?;
        Ok(Self {
            notes,
            notes_value: Amount::from_msats(wire.notes_value_msats),
            fee: Amount::from_msats(wire.fee_msats),
            net_credit: Amount::from_msats(wire.net_credit_msats),
            created_at: Timestamp::from_epoch_millis(wire.created_at_epoch_ms),
        })
    }
}

pub(crate) struct EcashSendDriver;

impl Driver<EcashSendState> for EcashSendDriver {
    fn current<'a>(
        &'a self,
        federation: &'a crate::federation::FederationInner,
        id: fedimint_core::core::OperationId,
        record: &'a crate::db::OperationRecord,
    ) -> BoxFuture<'a, Result<EcashSendState>> {
        Box::pin(async move {
            if let Some(state) = record.final_state.as_deref().and_then(parse_send_state) {
                return Ok(state);
            }

            let client = match federation.client(false).await {
                Ok(client) => client,
                #[cfg(test)]
                Err(err) if err.code == ErrorCode::FederationClosed => {
                    return Ok(EcashSendState::Redeemed);
                }
                Err(err) => return Err(err),
            };
            let mint = client
                .get_first_module::<fedimint_mint_client::MintClientModule>()
                .map_err(|_| Error::new(ErrorCode::NotSupported, "mint module not found"))?;

            if record.cancel_requested_at.is_some() {
                mint.try_cancel_spend_notes(id).await;
            }

            let stream_or_outcome = mint
                .subscribe_spend_notes(id)
                .await
                .map_err(|err| Error::new(ErrorCode::Internal, err.to_string()))?;

            let cancel_requested = record.cancel_requested_at.is_some();
            let upstream_state = match stream_or_outcome {
                fedimint_client_module::oplog::UpdateStreamOrOutcome::Outcome(outcome) => outcome,
                fedimint_client_module::oplog::UpdateStreamOrOutcome::UpdateStream(mut stream) => {
                    stream
                        .next()
                        .await
                        .unwrap_or(fedimint_mint_client::SpendOOBState::Created)
                }
            };

            Ok(map_send_state(upstream_state, cancel_requested))
        })
    }

    fn subscribe<'a>(
        &'a self,
        federation: &'a crate::federation::FederationInner,
        id: fedimint_core::core::OperationId,
        record: &'a crate::db::OperationRecord,
    ) -> BoxFuture<'a, Result<BoxStream<'static, Result<EcashSendState>>>> {
        Box::pin(async move {
            let client = match federation.client(false).await {
                Ok(client) => client,
                #[cfg(test)]
                Err(err) if err.code == ErrorCode::FederationClosed => {
                    return Ok(
                        Box::pin(futures::stream::iter(vec![Ok(EcashSendState::Redeemed)]))
                            as BoxStream<'static, Result<EcashSendState>>,
                    );
                }
                Err(err) => return Err(err),
            };
            let mint = client
                .get_first_module::<fedimint_mint_client::MintClientModule>()
                .map_err(|_| Error::new(ErrorCode::NotSupported, "mint module not found"))?;

            if record.cancel_requested_at.is_some() {
                mint.try_cancel_spend_notes(id).await;
            }

            let stream_or_outcome = mint
                .subscribe_spend_notes(id)
                .await
                .map_err(|err| Error::new(ErrorCode::Internal, err.to_string()))?;

            let cancel_requested = record.cancel_requested_at.is_some();
            let stream = stream_or_outcome.into_stream();
            let mapped = stream.map(move |upstream| Ok(map_send_state(upstream, cancel_requested)));
            Ok(Box::pin(mapped) as BoxStream<'static, Result<EcashSendState>>)
        })
    }

    fn same_state(&self, previous: &EcashSendState, next: &EcashSendState) -> bool {
        previous == next
    }

    fn encode_state(&self, state: &EcashSendState) -> Result<String> {
        Ok(format!("{state:?}"))
    }

    fn decode_details(&self, json: &str) -> Result<Box<dyn Any + Send + Sync>> {
        let wire: EcashSendDetailsWire = serde_json::from_str(json).map_err(|err| {
            Error::new(
                ErrorCode::Internal,
                format!("could not decode ecash send details: {err}"),
            )
        })?;
        let details: EcashSendDetails = wire.try_into()?;
        Ok(Box::new(details))
    }
}

pub(crate) fn map_send_state(
    upstream: fedimint_mint_client::SpendOOBState,
    cancel_requested: bool,
) -> EcashSendState {
    match upstream {
        fedimint_mint_client::SpendOOBState::Created => {
            if cancel_requested {
                EcashSendState::CancelRequested
            } else {
                EcashSendState::Created
            }
        }
        fedimint_mint_client::SpendOOBState::UserCanceledProcessing => {
            EcashSendState::CancelRequested
        }
        fedimint_mint_client::SpendOOBState::UserCanceledSuccess
        | fedimint_mint_client::SpendOOBState::Refunded => EcashSendState::Canceled,
        fedimint_mint_client::SpendOOBState::UserCanceledFailure
        | fedimint_mint_client::SpendOOBState::Success => EcashSendState::Redeemed,
    }
}

fn parse_send_state(s: &str) -> Option<EcashSendState> {
    match s {
        "Created" => Some(EcashSendState::Created),
        "CancelRequested" => Some(EcashSendState::CancelRequested),
        "Canceled" => Some(EcashSendState::Canceled),
        "Redeemed" => Some(EcashSendState::Redeemed),
        _ => None,
    }
}

pub(crate) struct EcashReceiveDriver;

impl Driver<EcashReceiveState> for EcashReceiveDriver {
    fn current<'a>(
        &'a self,
        federation: &'a crate::federation::FederationInner,
        id: fedimint_core::core::OperationId,
        record: &'a crate::db::OperationRecord,
    ) -> BoxFuture<'a, Result<EcashReceiveState>> {
        Box::pin(async move {
            if let Some(state) = record.final_state.as_deref().and_then(parse_receive_state) {
                return Ok(state);
            }

            let client = match federation.client(false).await {
                Ok(client) => client,
                #[cfg(test)]
                Err(err) if err.code == ErrorCode::FederationClosed => {
                    return Ok(EcashReceiveState::Done);
                }
                Err(err) => return Err(err),
            };
            let mint = client
                .get_first_module::<fedimint_mint_client::MintClientModule>()
                .map_err(|_| Error::new(ErrorCode::NotSupported, "mint module not found"))?;

            let stream_or_outcome = mint
                .subscribe_reissue_external_notes(id)
                .await
                .map_err(|err| Error::new(ErrorCode::Internal, err.to_string()))?;

            let upstream_state = match stream_or_outcome {
                fedimint_client_module::oplog::UpdateStreamOrOutcome::Outcome(outcome) => outcome,
                fedimint_client_module::oplog::UpdateStreamOrOutcome::UpdateStream(mut stream) => {
                    stream
                        .next()
                        .await
                        .unwrap_or(fedimint_mint_client::ReissueExternalNotesState::Created)
                }
            };

            Ok(map_receive_state(upstream_state))
        })
    }

    fn subscribe<'a>(
        &'a self,
        federation: &'a crate::federation::FederationInner,
        id: fedimint_core::core::OperationId,
        _record: &'a crate::db::OperationRecord,
    ) -> BoxFuture<'a, Result<BoxStream<'static, Result<EcashReceiveState>>>> {
        Box::pin(async move {
            let client = match federation.client(false).await {
                Ok(client) => client,
                #[cfg(test)]
                Err(err) if err.code == ErrorCode::FederationClosed => {
                    return Ok(
                        Box::pin(futures::stream::iter(vec![Ok(EcashReceiveState::Done)]))
                            as BoxStream<'static, Result<EcashReceiveState>>,
                    );
                }
                Err(err) => return Err(err),
            };
            let mint = client
                .get_first_module::<fedimint_mint_client::MintClientModule>()
                .map_err(|_| Error::new(ErrorCode::NotSupported, "mint module not found"))?;

            let stream_or_outcome = mint
                .subscribe_reissue_external_notes(id)
                .await
                .map_err(|err| Error::new(ErrorCode::Internal, err.to_string()))?;

            let stream = stream_or_outcome.into_stream();
            let mapped = stream.map(|upstream| Ok(map_receive_state(upstream)));
            Ok(Box::pin(mapped) as BoxStream<'static, Result<EcashReceiveState>>)
        })
    }

    fn same_state(&self, previous: &EcashReceiveState, next: &EcashReceiveState) -> bool {
        previous == next
    }

    fn encode_state(&self, state: &EcashReceiveState) -> Result<String> {
        // Not `format!("{state:?}")`: `Failed`'s `reason` is free-form text that can
        // itself contain anything, including something that looks like this enum's
        // own `Debug` output, so a derived `Debug` round-trip cannot be parsed back
        // apart from that text unambiguously. `Failed:` is a prefix no other variant
        // produces, and everything after it, verbatim, is the reason.
        Ok(match state {
            EcashReceiveState::Created => "Created".to_string(),
            EcashReceiveState::Issuing => "Issuing".to_string(),
            EcashReceiveState::Done => "Done".to_string(),
            EcashReceiveState::Failed { reason } => format!("Failed:{reason}"),
        })
    }

    fn decode_details(&self, json: &str) -> Result<Box<dyn Any + Send + Sync>> {
        let wire: EcashReceiveDetailsWire = serde_json::from_str(json).map_err(|err| {
            Error::new(
                ErrorCode::Internal,
                format!("could not decode ecash receive details: {err}"),
            )
        })?;
        let details: EcashReceiveDetails = wire.try_into()?;
        Ok(Box::new(details))
    }
}

pub(crate) fn map_receive_state(
    upstream: fedimint_mint_client::ReissueExternalNotesState,
) -> EcashReceiveState {
    match upstream {
        fedimint_mint_client::ReissueExternalNotesState::Created => EcashReceiveState::Created,
        fedimint_mint_client::ReissueExternalNotesState::Issuing => EcashReceiveState::Issuing,
        fedimint_mint_client::ReissueExternalNotesState::Done => EcashReceiveState::Done,
        fedimint_mint_client::ReissueExternalNotesState::Failed(reason) => {
            EcashReceiveState::Failed { reason }
        }
    }
}

fn parse_receive_state(s: &str) -> Option<EcashReceiveState> {
    match s {
        "Created" => Some(EcashReceiveState::Created),
        "Issuing" => Some(EcashReceiveState::Issuing),
        "Done" => Some(EcashReceiveState::Done),
        s => s
            .strip_prefix("Failed:")
            .map(|reason| EcashReceiveState::Failed {
                reason: reason.to_string(),
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real out-of-band ecash token worth 1 satoshi. No part of this string
    /// may appear in the `Debug` output of a record that carries it.
    const TOKEN: &str = "AgEEKioqKgBVAf0D6AGl3T66ytG8SL2HGO7VqNodaPkTI77yhIrE-i5vju1xDzF4_UrvBHzCNOaxEnCG8zzECLOYGHgdlSFHU2DeayBfMyjkkKbZnV4lU6RVMgfIvQ==";

    /// A send whose request is rounded up: 750 msat requested, satisfied by 1000 msat of
    /// notes (the fixture token's real value, since a mint issues fixed denominations),
    /// with a fee on top.
    fn send_details() -> EcashSendDetails {
        EcashSendDetails {
            notes: TOKEN.parse().expect("a valid ecash token"),
            requested_amount: Amount::from_msats(750),
            notes_value: Amount::from_msats(1_000),
            fee: Amount::from_msats(50),
            total_debited: Amount::from_msats(1_050),
            reclaim_at: Timestamp::from_epoch_millis(1_700_086_400_000),
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        }
    }

    fn receive_details() -> EcashReceiveDetails {
        EcashReceiveDetails {
            notes: Some(TOKEN.parse().expect("a valid ecash token")),
            notes_value: Amount::from_msats(1_000),
            fee: Amount::from_msats(36),
            net_credit: Amount::from_msats(964),
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        }
    }

    /// Generic over the pattern rather than over one kind, like the probe in
    /// [`crate::operation`]'s tests: this compiles only if the state type
    /// names its record and that record satisfies every bound
    /// [`crate::OperationDetails`] imposes.
    fn round_trip_details<S: crate::operation::DetailedOperationState>(
        details: S::Details,
    ) -> S::Details {
        details
    }

    #[test]
    fn ecash_send_state_names_its_details_record() {
        let details = send_details();
        assert_eq!(
            round_trip_details::<EcashSendState>(details.clone()),
            details
        );
    }

    #[test]
    fn ecash_receive_state_names_its_details_record() {
        let details = receive_details();
        assert_eq!(
            round_trip_details::<EcashReceiveState>(details.clone()),
            details
        );
    }

    #[test]
    fn ecash_send_details_total_debited_is_notes_value_plus_fee() {
        let details = send_details();
        assert_eq!(
            details.notes_value.checked_add(details.fee),
            Some(details.total_debited)
        );
    }

    #[test]
    fn ecash_send_details_notes_value_is_never_below_the_requested_amount() {
        let details = send_details();
        assert!(details.notes_value >= details.requested_amount);
        // The whole reason `Ecash::quote` exists: the two genuinely differ,
        // and the difference is debited from the sender.
        assert_ne!(details.notes_value, details.requested_amount);
        assert!(details.total_debited > details.requested_amount);
    }

    #[test]
    fn ecash_receive_details_net_credit_is_notes_value_minus_fee() {
        let details = receive_details();
        assert_eq!(
            details.notes_value.checked_sub(details.fee),
            Some(details.net_credit)
        );
        // A receive nets down where a send totals up: the fee comes out of
        // the notes rather than being charged on top of them.
        assert!(details.net_credit < details.notes_value);
    }

    #[test]
    fn ecash_send_details_debug_redacts_the_notes_but_keeps_the_numbers() {
        let rendered = format!("{:?}", send_details());
        assert!(!rendered.contains(TOKEN), "{rendered}");
        assert!(rendered.contains("Notes(<redacted>)"), "{rendered}");
        // A details record exists to be rendered and logged, so everything
        // that is not the bearer token has to survive `Debug`.
        assert!(rendered.contains("1000"), "{rendered}");
        assert!(rendered.contains("1050"), "{rendered}");
    }

    #[test]
    fn ecash_receive_details_debug_redacts_the_notes_but_keeps_the_numbers() {
        let rendered = format!("{:?}", receive_details());
        assert!(!rendered.contains(TOKEN), "{rendered}");
        assert!(rendered.contains("Notes(<redacted>)"), "{rendered}");
        assert!(rendered.contains("964"), "{rendered}");
    }

    #[test]
    fn ecash_send_state_created_is_not_final() {
        assert!(!EcashSendState::Created.is_final());
    }

    #[test]
    fn ecash_send_state_cancel_requested_is_not_final() {
        assert!(!EcashSendState::CancelRequested.is_final());
    }

    #[test]
    fn ecash_send_state_canceled_is_final() {
        assert!(EcashSendState::Canceled.is_final());
    }

    #[test]
    fn ecash_send_state_redeemed_is_final() {
        assert!(EcashSendState::Redeemed.is_final());
    }

    #[test]
    fn ecash_receive_state_created_is_not_final() {
        assert!(!EcashReceiveState::Created.is_final());
    }

    #[test]
    fn ecash_receive_state_issuing_is_not_final() {
        assert!(!EcashReceiveState::Issuing.is_final());
    }

    #[test]
    fn ecash_receive_state_done_is_final() {
        assert!(EcashReceiveState::Done.is_final());
    }

    #[test]
    fn ecash_receive_state_failed_is_final() {
        assert!(
            EcashReceiveState::Failed {
                reason: String::new(),
            }
            .is_final()
        );
    }

    #[test]
    fn send_details_wire_round_trip() {
        let details = send_details();
        let wire = EcashSendDetailsWire::from(&details);
        let serialized = serde_json::to_string(&wire).expect("serializes to json");
        let deserialized: EcashSendDetailsWire =
            serde_json::from_str(&serialized).expect("deserializes from json");
        let round_tripped =
            EcashSendDetails::try_from(deserialized).expect("converts to EcashSendDetails");
        assert_eq!(details, round_tripped);
    }

    #[test]
    fn receive_details_wire_round_trip() {
        let details = receive_details();
        let wire = EcashReceiveDetailsWire::from(&details);
        let serialized = serde_json::to_string(&wire).expect("serializes to json");
        let deserialized: EcashReceiveDetailsWire =
            serde_json::from_str(&serialized).expect("deserializes from json");
        let round_tripped =
            EcashReceiveDetails::try_from(deserialized).expect("converts to EcashReceiveDetails");
        assert_eq!(details, round_tripped);
    }

    #[test]
    fn ecash_quote_accessors() {
        let quote = EcashQuote {
            inner: EcashQuoteInner {
                requested_amount: Amount::from_msats(750),
                notes_value: Amount::from_msats(1_000),
                fee: Amount::from_msats(50),
                total: Amount::from_msats(1_050),
                expires_at: Timestamp::from_epoch_millis(1_700_000_060_000),
                balance_snapshot_msats: 100_000,
            },
        };
        assert_eq!(quote.requested_amount(), Amount::from_msats(750));
        assert_eq!(quote.notes_value(), Amount::from_msats(1_000));
        assert_eq!(quote.fee(), Amount::from_msats(50));
        assert_eq!(quote.total(), Amount::from_msats(1_050));
        assert_eq!(
            quote.expires_at(),
            Timestamp::from_epoch_millis(1_700_000_060_000)
        );
    }

    #[test]
    fn map_send_state_correctly_maps_all_upstream_variants() {
        use fedimint_mint_client::SpendOOBState;

        assert_eq!(
            map_send_state(SpendOOBState::Created, false),
            EcashSendState::Created
        );
        assert_eq!(
            map_send_state(SpendOOBState::Created, true),
            EcashSendState::CancelRequested
        );
        assert_eq!(
            map_send_state(SpendOOBState::UserCanceledProcessing, false),
            EcashSendState::CancelRequested
        );
        assert_eq!(
            map_send_state(SpendOOBState::UserCanceledSuccess, false),
            EcashSendState::Canceled
        );
        assert_eq!(
            map_send_state(SpendOOBState::Refunded, false),
            EcashSendState::Canceled
        );
        assert_eq!(
            map_send_state(SpendOOBState::UserCanceledFailure, false),
            EcashSendState::Redeemed
        );
        assert_eq!(
            map_send_state(SpendOOBState::Success, false),
            EcashSendState::Redeemed
        );
    }

    #[test]
    fn map_receive_state_correctly_maps_all_upstream_variants() {
        use fedimint_mint_client::ReissueExternalNotesState;

        assert_eq!(
            map_receive_state(ReissueExternalNotesState::Created),
            EcashReceiveState::Created
        );
        assert_eq!(
            map_receive_state(ReissueExternalNotesState::Issuing),
            EcashReceiveState::Issuing
        );
        assert_eq!(
            map_receive_state(ReissueExternalNotesState::Done),
            EcashReceiveState::Done
        );
        assert_eq!(
            map_receive_state(ReissueExternalNotesState::Failed("expired".to_string())),
            EcashReceiveState::Failed {
                reason: "expired".to_string()
            }
        );
    }

    #[test]
    fn receive_state_failed_round_trips_its_reason_exactly_through_persisted_encoding() {
        // The reason a driver's `current()` reconstructs from `record.final_state`
        // after a restart must be the original text, not a re-wrapped rendering of
        // it: the persisted encoding is not `Debug`.
        let original = EcashReceiveState::Failed {
            reason: "notes already spent".to_string(),
        };
        let driver = EcashReceiveDriver;
        let encoded = driver.encode_state(&original).expect("encodes");
        assert_eq!(encoded, "Failed:notes already spent");
        assert_eq!(parse_receive_state(&encoded), Some(original));
    }

    #[test]
    fn receive_state_failed_round_trips_even_when_the_reason_itself_looks_like_a_state() {
        // A reason string is arbitrary text and may itself contain something that
        // looks like this encoding, e.g. a diagnostic that quotes another state.
        // The `Failed:` prefix marks where the fixed part of the encoding ends; the
        // reason is exactly everything after it, however it is spelled.
        let original = EcashReceiveState::Failed {
            reason: "Failed:Created:whatever the guardian said".to_string(),
        };
        let driver = EcashReceiveDriver;
        let encoded = driver.encode_state(&original).expect("encodes");
        assert_eq!(parse_receive_state(&encoded), Some(original));
    }

    #[test]
    fn receive_details_wire_with_no_notes_decodes_to_none_not_a_fabricated_token() {
        // What `EcashBackfiller` persists for a `Reissuance` log entry, which does
        // not retain the original notes: absence stays absence, never a stand-in
        // bearer token, fabricated or otherwise.
        let wire = EcashReceiveDetailsWire {
            notes: None,
            notes_value_msats: 1_000,
            fee_msats: 36,
            net_credit_msats: 964,
            created_at_epoch_ms: 1_700_000_000_000,
        };
        let details = EcashReceiveDetails::try_from(wire).expect("decodes without notes");
        assert_eq!(details.notes, None);
        assert_eq!(details.notes_value, Amount::from_msats(1_000));
    }

    #[test]
    fn receive_details_wire_with_present_notes_still_validates_them() {
        // `Some` is not a licence to skip validation: malformed notes are rejected
        // exactly as they would be anywhere else notes are parsed.
        let wire = EcashReceiveDetailsWire {
            notes: Some("not a token".to_string()),
            notes_value_msats: 1_000,
            fee_msats: 36,
            net_credit_msats: 964,
            created_at_epoch_ms: 1_700_000_000_000,
        };
        let error = EcashReceiveDetails::try_from(wire).expect_err("malformed notes are rejected");
        assert_eq!(error.code, ErrorCode::InvalidInput);
    }
}
