//! Paying out of a multisig account: proposals, approvals, and what an execution checks before
//! the federation sees it.

use std::collections::BTreeMap;

use fedimint_core::bitcoin::hashes::{Hash, HashEngine, sha256};
use fedimint_core::encoding::{Decodable, Encodable};
use fedimint_core::module::registry::ModuleDecoderRegistry;
use fedimint_core::secp256k1::{Message, PublicKey, SECP256K1, schnorr};

use super::account::{AmountWire, PolicyWire};
use crate::{
    Error, ErrorCode, MemberKey, MultisigAccount, MultisigAccountId, MultisigAmount, Policy, Result,
};

/// What a printed proposal starts with.
const PROPOSAL_PREFIX: &str = "msproposal";
/// What a printed approval starts with.
const APPROVAL_PREFIX: &str = "msapproval";
/// What a proposal's digest starts with, so that the signature over it is good for nothing else.
const PROPOSAL_DOMAIN: &[u8] = b"fedimint-sdk multisig proposal";
/// The encoding version this build writes, and the newest it reads.
///
/// The first byte of every encoded proposal and approval. Proposals and approvals travel between
/// members whose applications may run different versions of this crate, so a version this build
/// does not know is refused with a message that says so, rather than misread.
const VERSION: u8 = 1;

/// A payment out of a multisig account, waiting for its members' approvals.
///
/// Made by [`Multisig::propose`](crate::Multisig::propose), which pays the proposing member, and
/// handed to the other members over whatever channel the application has: its own server, a chat,
/// a QR code. Each member who agrees answers with an [`Approval`] from
/// [`Multisig::approve`](crate::Multisig::approve), and once the account's threshold is met anyone
/// holding the proposal and the approvals executes it with
/// [`Multisig::execute`](crate::Multisig::execute).
///
/// Nothing here is trusted on its word: the channel can be anyone's. The member who made a
/// proposal signs it, and the payment goes to that member, its [`proposer`](Proposal::proposer).
/// The parse checks that signature, so a proposal cannot be passed off under another member's
/// name, or altered. Every approval is a signature the federation checks against the account's
/// policy, so a forged approval cannot move money either.
///
/// It round-trips through [`Display`](core::fmt::Display) and [`FromStr`](core::str::FromStr), as
/// `msproposal` followed by lowercase base32. The parse checks the structure, that the policy
/// describes an account, and that one of its members signed the proposal, not the approvals it
/// will collect.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
pub struct Proposal {
    /// The encoding, version byte first: what `Display` prints.
    encoded: Vec<u8>,
    /// What the proposer signed, and what an approval names the proposal it approves by. It
    /// covers everything but the signature, so signing the same proposal again changes nothing an
    /// approval relies on.
    digest: sha256::Hash,
    /// The module kind of the backend that made it, which is the only one that can execute it.
    module: String,
    account: MultisigAccount,
    amount: MultisigAmount,
    proposer: MemberKey,
    /// What the backend needs signed, opaque to this crate.
    payload: Vec<u8>,
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
impl Proposal {
    /// Parses a proposal from its printed form.
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for a malformed value, for one that is not
    /// signed by a member of its account, and for one written by a newer version of this SDK than
    /// this one reads.
    #[cfg_attr(feature = "uniffi", uniffi::constructor)]
    pub fn parse(proposal: String) -> Result<Proposal> {
        proposal.parse()
    }

    /// The id of the account the payment comes out of.
    pub fn account_id(&self) -> MultisigAccountId {
        self.account.id()
    }

    /// The policy of the account the payment comes out of.
    pub fn policy(&self) -> Policy {
        self.account.policy()
    }

    /// How much the payment is for, in the account's denomination.
    pub fn amount(&self) -> MultisigAmount {
        self.amount.clone()
    }

    /// The member who made the proposal, and who the payment goes to.
    ///
    /// Signed by that member, so it is who made the proposal. A member of the account can still
    /// hand out a proposal of their own in place of the one the others expect, and this is what
    /// tells the two apart: show it before asking a member to approve.
    pub fn proposer(&self) -> MemberKey {
        self.proposer
    }

    /// The proposal in its printed form, to hand to the other members.
    pub fn display(&self) -> String {
        self.to_string()
    }
}

impl Proposal {
    /// The account the payment comes out of.
    pub fn account(&self) -> MultisigAccount {
        self.account.clone()
    }

    /// What `proposer` signs to propose paying `amount` out of `account` through `module`, with
    /// `payload`.
    pub(crate) fn digest_of(
        module: &str,
        account: &MultisigAccount,
        amount: &MultisigAmount,
        proposer: MemberKey,
        payload: &[u8],
    ) -> sha256::Hash {
        ProposalBody::new(module, account, amount, proposer, payload).digest()
    }

    /// The proposal `proposer` signed with `signature`, encoded once here.
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) when `proposer` is not a member of
    /// `account`, or `signature` is not theirs over the proposal.
    pub(crate) fn signed(
        module: String,
        account: MultisigAccount,
        amount: MultisigAmount,
        proposer: MemberKey,
        payload: Vec<u8>,
        signature: schnorr::Signature,
    ) -> Result<Proposal> {
        let body = ProposalBody::new(&module, &account, &amount, proposer, &payload);
        let digest = body.digest();
        check_proposer(&account, proposer, digest, &signature)?;
        let mut encoded = vec![VERSION];
        encoded.extend(ProposalWire { body, signature }.consensus_encode_to_vec());
        Ok(Proposal {
            encoded,
            digest,
            module,
            account,
            amount,
            proposer,
            payload,
        })
    }

    /// The module kind of the backend that made this proposal.
    pub(crate) fn module(&self) -> &str {
        &self.module
    }

    /// What an approval names this proposal by.
    pub(crate) fn digest(&self) -> sha256::Hash {
        self.digest
    }
}

#[cfg(feature = "extension-api")]
impl Proposal {
    /// What the [`MultisigBackend`](crate::MultisigBackend) that made this proposal put in it,
    /// from [`MultisigBackend::propose`](crate::MultisigBackend::propose).
    pub fn payload(&self) -> Vec<u8> {
        self.payload.clone()
    }
}

impl core::fmt::Display for Proposal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&fedimint_core::base32::encode_prefixed_bytes(
            PROPOSAL_PREFIX,
            &self.encoded,
        ))
    }
}

impl core::str::FromStr for Proposal {
    type Err = Error;

    /// Parses a proposal from its printed form. Returns
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a malformed value, for one
    /// that is not signed by a member of its account, and for one written by a newer version of
    /// this SDK.
    fn from_str(s: &str) -> Result<Proposal> {
        let encoded = decode("proposal", PROPOSAL_PREFIX, s)?;
        let ProposalWire { body, signature } = decode_wire("proposal", &encoded)?;
        let digest = body.digest();
        let account = MultisigAccount::derive(&Policy::try_from(body.policy)?)?;
        let amount = MultisigAmount::try_from(body.amount)?;
        check_amount(&account, &amount)?;
        let proposer = MemberKey::from_upstream(body.proposer);
        check_proposer(&account, proposer, digest, &signature)?;
        Ok(Proposal {
            encoded,
            digest,
            module: body.module,
            account,
            amount,
            proposer,
            payload: body.payload,
        })
    }
}

/// One member's approval of a [`Proposal`].
///
/// Made by [`Multisig::approve`](crate::Multisig::approve) and handed back to whoever collects
/// approvals for [`Multisig::execute`](crate::Multisig::execute). It approves exactly one
/// proposal, and carries the member's signature over it.
///
/// It round-trips through [`Display`](core::fmt::Display) and [`FromStr`](core::str::FromStr), as
/// `msapproval` followed by lowercase base32. The parse checks the structure, not the signature:
/// that is the federation's job.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
pub struct Approval {
    /// The encoding, version byte first: what `Display` prints.
    encoded: Vec<u8>,
    /// The digest of the proposal this approves.
    proposal: sha256::Hash,
    member: MemberKey,
    /// The backend's signature, opaque to this crate.
    payload: Vec<u8>,
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
impl Approval {
    /// Parses an approval from its printed form.
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for a malformed value, and for one
    /// written by a newer version of this SDK than this one reads.
    #[cfg_attr(feature = "uniffi", uniffi::constructor)]
    pub fn parse(approval: String) -> Result<Approval> {
        approval.parse()
    }

    /// The key of the member who approved.
    pub fn member(&self) -> MemberKey {
        self.member
    }

    /// Whether this approves `proposal`, and not some other one.
    pub fn approves(&self, proposal: &Proposal) -> bool {
        self.proposal == proposal.digest
    }

    /// The approval in its printed form, to hand to whoever executes the proposal.
    pub fn display(&self) -> String {
        self.to_string()
    }
}

impl Approval {
    /// `member`'s approval of the proposal `proposal` is the digest of, encoded once here.
    pub(crate) fn new(proposal: sha256::Hash, member: MemberKey, payload: Vec<u8>) -> Approval {
        let wire = ApprovalWire {
            proposal,
            member: member.upstream(),
            payload: payload.clone(),
        };
        let mut encoded = vec![VERSION];
        encoded.extend(wire.consensus_encode_to_vec());
        Approval {
            encoded,
            proposal,
            member,
            payload,
        }
    }
}

#[cfg(feature = "extension-api")]
impl Approval {
    /// `member`'s approval of `proposal`, carrying `payload`.
    ///
    /// For a backend that receives approvals some other way than through this crate, and has to
    /// hand them to [`Multisig::execute`](crate::Multisig::execute). What
    /// [`Multisig::approve`](crate::Multisig::approve) makes is built the same way, with what
    /// [`MultisigBackend::approve`](crate::MultisigBackend::approve) returns as the payload.
    pub fn from_parts(proposal: &Proposal, member: MemberKey, payload: Vec<u8>) -> Approval {
        Approval::new(proposal.digest(), member, payload)
    }

    /// What the [`MultisigBackend`](crate::MultisigBackend) put in this approval, from
    /// [`MultisigBackend::approve`](crate::MultisigBackend::approve).
    pub fn payload(&self) -> Vec<u8> {
        self.payload.clone()
    }
}

impl core::fmt::Display for Approval {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&fedimint_core::base32::encode_prefixed_bytes(
            APPROVAL_PREFIX,
            &self.encoded,
        ))
    }
}

impl core::str::FromStr for Approval {
    type Err = Error;

    /// Parses an approval from its printed form. Returns
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a malformed value, and for
    /// one written by a newer version of this SDK.
    fn from_str(s: &str) -> Result<Approval> {
        let encoded = decode("approval", APPROVAL_PREFIX, s)?;
        let wire: ApprovalWire = decode_wire("approval", &encoded)?;
        Ok(Approval {
            encoded,
            proposal: wire.proposal,
            member: MemberKey::from_upstream(wire.member),
            payload: wire.payload,
        })
    }
}

/// Refuses paying out nothing, or an amount in a different denomination from the account's.
pub(crate) fn check_amount(account: &MultisigAccount, amount: &MultisigAmount) -> Result<()> {
    if amount.denomination() != account.denomination() {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "the amount is not in the account's denomination",
        ));
    }
    if amount.is_zero() {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "cannot pay out nothing",
        ));
    }
    Ok(())
}

/// Refuses a proposal whose proposer is not a member of `account`, or did not sign `digest` with
/// `signature`.
fn check_proposer(
    account: &MultisigAccount,
    proposer: MemberKey,
    digest: sha256::Hash,
    signature: &schnorr::Signature,
) -> Result<()> {
    if !account.members().contains(&proposer) {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            format!("the proposal is made by {proposer}, who is not a member of the account"),
        ));
    }
    let (key, _) = proposer.upstream().x_only_public_key();
    SECP256K1
        .verify_schnorr(
            signature,
            &Message::from_digest(digest.to_byte_array()),
            &key,
        )
        .map_err(|_| {
            Error::new(
                ErrorCode::InvalidInput,
                "the proposal is not signed by the member it names as its proposer",
            )
        })
}

/// The approvals that let `proposal` be executed, one per member, or the refusal that says why
/// they do not.
///
/// Checked here, once, for every backend: each approval is for this proposal and from a member of
/// its account, and there are as many distinct members as the threshold asks for. A member's
/// second approval is dropped rather than refused, since collecting the same one twice is an
/// ordinary thing for a relay to do. The signatures are the backend's to check, and the
/// federation's after it.
pub(crate) fn approvals_for(proposal: &Proposal, approvals: &[Approval]) -> Result<Vec<Approval>> {
    let mut by_member = BTreeMap::new();
    for approval in approvals {
        if !approval.approves(proposal) {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "an approval is for a different proposal",
            ));
        }
        if !proposal.account.members().contains(&approval.member) {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                format!(
                    "an approval is from {}, who is not a member of the account",
                    approval.member
                ),
            ));
        }
        by_member
            .entry(approval.member)
            .or_insert_with(|| approval.clone());
    }
    let threshold = proposal.account.threshold();
    let have = u32::try_from(by_member.len()).unwrap_or(u32::MAX);
    if have < threshold {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            format!("{have} members approved, and the account needs {threshold}"),
        ));
    }
    Ok(by_member.into_values().collect())
}

/// A proposal as it is encoded, after the version byte.
#[derive(Debug, Clone, PartialEq, Eq, Encodable, Decodable)]
struct ProposalWire {
    body: ProposalBody,
    /// The proposer's signature over the body's digest.
    signature: schnorr::Signature,
}

/// What a proposal says, which its proposer signs.
#[derive(Debug, Clone, PartialEq, Eq, Encodable, Decodable)]
struct ProposalBody {
    module: String,
    policy: PolicyWire,
    amount: AmountWire,
    proposer: PublicKey,
    payload: Vec<u8>,
}

impl ProposalBody {
    fn new(
        module: &str,
        account: &MultisigAccount,
        amount: &MultisigAmount,
        proposer: MemberKey,
        payload: &[u8],
    ) -> ProposalBody {
        ProposalBody {
            module: module.to_owned(),
            policy: PolicyWire::from(&account.policy()),
            amount: AmountWire::from(amount),
            proposer: proposer.upstream(),
            payload: payload.to_vec(),
        }
    }

    /// What the proposer signs: the body under this encoding's version, after a domain of its
    /// own.
    fn digest(&self) -> sha256::Hash {
        let mut engine = sha256::Hash::engine();
        engine.input(PROPOSAL_DOMAIN);
        engine.input(&[VERSION]);
        engine.input(&self.consensus_encode_to_vec());
        sha256::Hash::from_engine(engine)
    }
}

/// An approval as it is encoded, after the version byte.
#[derive(Debug, Clone, PartialEq, Eq, Encodable, Decodable)]
struct ApprovalWire {
    proposal: sha256::Hash,
    member: PublicKey,
    payload: Vec<u8>,
}

/// The bytes a printed `what` carries, version byte first, once the prefix is checked.
fn decode(what: &str, prefix: &str, s: &str) -> Result<Vec<u8>> {
    let encoded = fedimint_core::base32::decode_prefixed_bytes(prefix, s.trim()).map_err(|_| {
        Error::new(
            ErrorCode::InvalidInput,
            format!("not a multisig {what}: it has to start with {prefix} and be base32"),
        )
    })?;
    match encoded.first() {
        Some(&VERSION) => Ok(encoded),
        Some(version) => Err(Error::new(
            ErrorCode::InvalidInput,
            format!(
                "this multisig {what} is in version {version} of its encoding, and this SDK reads \
                 up to {VERSION}"
            ),
        )),
        None => Err(Error::new(
            ErrorCode::InvalidInput,
            format!("this multisig {what} is empty"),
        )),
    }
}

/// The wire form after `encoded`'s version byte, all of it.
fn decode_wire<W>(what: &str, encoded: &[u8]) -> Result<W>
where
    W: Decodable,
{
    W::consensus_decode_whole(
        encoded.get(1..).unwrap_or_default(),
        &ModuleDecoderRegistry::default(),
    )
    .map_err(|err| {
        Error::new(
            ErrorCode::InvalidInput,
            format!("this multisig {what} is malformed: {err}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use fedimint_core::secp256k1::Keypair;

    use super::*;
    use crate::multisig::account::{test_keypairs, test_keys};
    use crate::{Amount, MultisigDenomination};

    const MODULE: &str = "multi_sig_stability_pool";

    fn account() -> MultisigAccount {
        let [a, b, c] = test_keys();
        MultisigAccount::derive(&Policy::new(vec![a, b, c], 2, MultisigDenomination::Btc))
            .expect("a valid policy")
    }

    fn btc(msats: u64) -> MultisigAmount {
        MultisigAmount::Btc {
            amount: Amount::from_msats(msats),
        }
    }

    /// `signer`'s signature over the proposal by `proposer` of `msats` out of [`account`].
    fn signature(signer: &Keypair, proposer: MemberKey, msats: u64) -> schnorr::Signature {
        let digest = Proposal::digest_of(MODULE, &account(), &btc(msats), proposer, &[1, 2, 3]);
        signer.sign_schnorr(Message::from_digest(digest.to_byte_array()))
    }

    /// The proposal `pair`'s member makes of `msats` out of [`account`].
    fn proposal_by(pair: &Keypair, msats: u64) -> Proposal {
        let proposer = MemberKey::from_upstream(pair.public_key());
        Proposal::signed(
            MODULE.to_owned(),
            account(),
            btc(msats),
            proposer,
            vec![1, 2, 3],
            signature(pair, proposer, msats),
        )
        .expect("a member's own proposal")
    }

    fn proposal() -> Proposal {
        let [a, ..] = test_keypairs();
        proposal_by(&a, 5_000)
    }

    #[test]
    fn a_proposal_round_trips_through_its_printed_form() {
        let proposal = proposal();
        let printed = proposal.to_string();
        assert!(printed.starts_with("msproposal"), "{printed}");

        let parsed: Proposal = printed.parse().expect("parses");
        assert_eq!(parsed, proposal);
        assert_eq!(parsed.account_id(), account().id());
        assert_eq!(parsed.module(), MODULE);
        assert_eq!(parsed.proposer(), test_keys()[0]);
        // Case does not matter, which is what lets a QR code carry it in alphanumeric mode.
        let shouted: Proposal = printed.to_uppercase().parse().expect("parses");
        assert_eq!(shouted, proposal);
    }

    #[test]
    fn a_proposal_is_only_made_by_the_member_who_signs_it() {
        let [a, b, _] = test_keypairs();
        let [key_a, ..] = test_keys();

        // Signed by b in a's name.
        let err = Proposal::signed(
            MODULE.to_owned(),
            account(),
            btc(5_000),
            key_a,
            vec![1, 2, 3],
            signature(&b, key_a, 5_000),
        )
        .expect_err("not a's signature");
        assert_eq!(err.code, ErrorCode::InvalidInput);

        // Signed by a, then altered.
        let altered = Proposal::signed(
            MODULE.to_owned(),
            account(),
            btc(6_000),
            key_a,
            vec![1, 2, 3],
            signature(&a, key_a, 5_000),
        )
        .expect_err("a signed another amount");
        assert_eq!(altered.code, ErrorCode::InvalidInput);

        // The parse checks the same, on whatever arrives over the channel.
        let mut encoded = proposal().encoded;
        let ProposalWire { body, .. } =
            decode_wire::<ProposalWire>("proposal", &encoded).expect("its own encoding decodes");
        encoded.truncate(1);
        encoded.extend(
            ProposalWire {
                body,
                signature: signature(&b, key_a, 5_000),
            }
            .consensus_encode_to_vec(),
        );
        let printed = fedimint_core::base32::encode_prefixed_bytes(PROPOSAL_PREFIX, &encoded);
        let err = printed.parse::<Proposal>().expect_err("not a's signature");
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn a_proposal_is_only_made_by_a_member_of_the_account() {
        let outsider = Keypair::from_seckey_slice(SECP256K1, &[7; 32]).expect("a valid key");
        let key = MemberKey::from_upstream(outsider.public_key());
        let err = Proposal::signed(
            MODULE.to_owned(),
            account(),
            btc(5_000),
            key,
            vec![1, 2, 3],
            signature(&outsider, key, 5_000),
        )
        .expect_err("not a member");
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn approvals_carry_over_to_the_same_proposal_signed_again() {
        let [a, ..] = test_keypairs();
        let [key_a, key_b, _] = test_keys();
        let digest = Proposal::digest_of(MODULE, &account(), &btc(5_000), key_a, &[1, 2, 3]);
        let message = Message::from_digest(digest.to_byte_array());
        let signed_with = |aux| {
            Proposal::signed(
                MODULE.to_owned(),
                account(),
                btc(5_000),
                key_a,
                vec![1, 2, 3],
                SECP256K1.sign_schnorr_with_aux_rand(&message, &a, &aux),
            )
            .expect("a's own proposal")
        };
        let first = signed_with([1; 32]);
        let again = signed_with([2; 32]);
        assert_ne!(first.to_string(), again.to_string());

        let approval = Approval::new(first.digest(), key_b, vec![9]);
        assert!(approval.approves(&again));
    }

    #[test]
    fn an_approval_round_trips_and_knows_its_proposal() {
        let [a, ..] = test_keypairs();
        let [key_a, ..] = test_keys();
        let proposal = proposal();
        let approval = Approval::new(proposal.digest(), key_a, vec![9; 64]);
        let parsed: Approval = approval.to_string().parse().expect("parses");
        assert_eq!(parsed, approval);
        assert_eq!(parsed.member(), key_a);
        assert!(parsed.approves(&proposal));
        assert!(!parsed.approves(&proposal_by(&a, 6_000)));
    }

    #[test]
    fn what_is_not_a_proposal_is_refused() {
        let printed = proposal().to_string();
        let padded = format!("{printed}00");
        for bad in [
            "",
            "msproposal",
            "msapproval0000",
            &printed[..printed.len() - 4],
            padded.as_str(),
        ] {
            let err = bad.parse::<Proposal>().expect_err("refused");
            assert_eq!(err.code, ErrorCode::InvalidInput, "{bad}");
        }
    }

    #[test]
    fn a_newer_encoding_is_refused_by_name() {
        let mut encoded = proposal().encoded;
        encoded[0] = VERSION + 1;
        let printed = fedimint_core::base32::encode_prefixed_bytes(PROPOSAL_PREFIX, &encoded);
        let err = printed.parse::<Proposal>().expect_err("refused");
        assert_eq!(err.code, ErrorCode::InvalidInput);
        assert!(err.message.contains("version 2"), "{}", err.message);
    }

    #[cfg(feature = "extension-api")]
    #[test]
    fn an_extension_builds_the_same_approvals_the_facade_does() {
        let [a, ..] = test_keys();
        let proposal = proposal();
        assert_eq!(
            Approval::from_parts(&proposal, a, vec![9]),
            Approval::new(proposal.digest(), a, vec![9])
        );
    }

    #[test]
    fn execution_needs_threshold_many_distinct_members_of_the_account() {
        let [a, b, _] = test_keys();
        let proposal = proposal();
        let approval = |member| Approval::new(proposal.digest(), member, vec![]);

        let err = approvals_for(&proposal, &[approval(a), approval(a)])
            .expect_err("one member twice is one approval");
        assert_eq!(err.code, ErrorCode::InvalidInput);

        let kept = approvals_for(&proposal, &[approval(b), approval(a), approval(a)])
            .expect("two members meet a threshold of two");
        assert_eq!(
            kept.iter().map(Approval::member).collect::<Vec<_>>(),
            vec![a, b]
        );
    }

    #[test]
    fn execution_refuses_approvals_that_are_not_for_it() {
        let [a, b, _] = test_keys();
        let proposal = proposal();

        let elsewhere = Approval::new(sha256::Hash::hash(b"another proposal"), a, vec![]);
        let err = approvals_for(
            &proposal,
            &[elsewhere, Approval::new(proposal.digest(), b, vec![])],
        )
        .expect_err("refused");
        assert_eq!(err.code, ErrorCode::InvalidInput);

        let outsider: MemberKey =
            "02e493dbf1c10d80f3581e4904930b1404cc6c13900ee0758474fa94abe8c4cd13"
                .parse()
                .expect("a valid key");
        let err = approvals_for(
            &proposal,
            &[
                Approval::new(proposal.digest(), outsider, vec![]),
                Approval::new(proposal.digest(), a, vec![]),
            ],
        )
        .expect_err("refused");
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }
}
