//! Paying out of a multisig account: proposals, approvals, and what an execution checks before
//! the federation sees it.

use std::collections::BTreeMap;

use fedimint_core::bitcoin::hashes::{Hash, HashEngine, sha256};
use fedimint_core::config;
use fedimint_core::encoding::{Decodable, Encodable};
use fedimint_core::module::registry::ModuleDecoderRegistry;
use fedimint_core::secp256k1::{Message, PublicKey, SECP256K1, schnorr};
use stability_pool_common::{AccountType, BtcTransferRequest};

use crate::{
    Amount, Error, ErrorCode, FederationId, MemberKey, MultisigAccount, MultisigAccountId, Policy,
    Result,
};

/// What a printed proposal starts with.
const PROPOSAL_PREFIX: &str = "msproposal";
/// What a printed approval starts with.
const APPROVAL_PREFIX: &str = "msapproval";
/// What a proposal's digest starts with, so that the signature over it is good for nothing else.
const PROPOSAL_DOMAIN: &[u8] = b"fedimint-sdk multisig proposal";
/// The encoding version this build writes, and the only one it reads.
///
/// The first byte of every encoded proposal and approval. Proposals and approvals travel between
/// members whose applications may run different versions of this crate, so a version this build
/// does not know is refused with a message that says so, rather than misread. Version 1 carried
/// a module kind and opaque payloads in their place, and an amount and a policy that the payout
/// request now says itself.
const VERSION: u8 = 2;
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
/// name, or altered. It names the federation it pays out on, and is refused on any other. Every
/// approval is a signature the federation checks against the account's policy, so a forged
/// approval cannot move money either.
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
    /// The federation it pays out on.
    federation: config::FederationId,
    /// The account the request pays out of, as the facade names it.
    account: MultisigAccount,
    proposer: MemberKey,
    /// The stability pool's request to pay out of the account to the proposer's payout account:
    /// what every approving member signs, and what the federation executes.
    request: BtcTransferRequest,
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
impl Proposal {
    /// Parses a proposal from its printed form.
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for a malformed value, for one that is not
    /// signed by a member of its account, and for one written by another version of this SDK's
    /// encoding than the one this one reads.
    #[cfg_attr(feature = "uniffi", uniffi::constructor)]
    pub fn parse(proposal: String) -> Result<Proposal> {
        proposal.parse()
    }

    /// The federation the payment is made on.
    ///
    /// [`Multisig::approve`](crate::Multisig::approve) and
    /// [`Multisig::execute`](crate::Multisig::execute) refuse a proposal made on another one.
    pub fn federation(&self) -> FederationId {
        FederationId::from_upstream(self.federation)
    }

    /// The id of the account the payment comes out of.
    pub fn account_id(&self) -> MultisigAccountId {
        self.account.id()
    }

    /// The policy of the account the payment comes out of.
    pub fn policy(&self) -> Policy {
        self.account.policy()
    }

    /// How much the payment is for.
    pub fn amount(&self) -> Amount {
        Amount::from_msats(self.request.amount().msats)
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

    /// What `proposer` signs to propose `request` on `federation`.
    pub(crate) fn digest_of(
        federation: config::FederationId,
        proposer: MemberKey,
        request: &BtcTransferRequest,
    ) -> sha256::Hash {
        ProposalBody::new(federation, proposer, request).digest()
    }

    /// The proposal `proposer` signed with `signature`, encoded once here.
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) when `request` does not pay out of a
    /// multisig account, `proposer` is not a member of that account, or `signature` is not
    /// theirs over the proposal.
    pub(crate) fn signed(
        federation: config::FederationId,
        proposer: MemberKey,
        request: BtcTransferRequest,
        signature: schnorr::Signature,
    ) -> Result<Proposal> {
        let body = ProposalBody::new(federation, proposer, &request);
        let mut encoded = vec![VERSION];
        encoded.extend(
            ProposalWire {
                body: body.clone(),
                signature,
            }
            .consensus_encode_to_vec(),
        );
        Proposal::checked(encoded, body, &signature)
    }

    /// The proposal `encoded` is, once `body` passes every check and `signature` is its
    /// proposer's.
    fn checked(
        encoded: Vec<u8>,
        body: ProposalBody,
        signature: &schnorr::Signature,
    ) -> Result<Proposal> {
        let digest = body.digest();
        let account = account_paid_out_of(&body.request)?;
        let proposer = MemberKey::from_upstream(body.proposer);
        check_proposer(&account, proposer, digest, signature)?;
        Ok(Proposal {
            encoded,
            digest,
            federation: body.federation,
            account,
            proposer,
            request: body.request,
        })
    }

    /// The stability pool's request this proposal pays out with.
    pub(crate) fn request(&self) -> &BtcTransferRequest {
        &self.request
    }

    /// What an approval names this proposal by.
    pub(crate) fn digest(&self) -> sha256::Hash {
        self.digest
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
    /// that is not signed by a member of its account, and for one written in another version of
    /// the encoding.
    fn from_str(s: &str) -> Result<Proposal> {
        let encoded = decode("proposal", PROPOSAL_PREFIX, s)?;
        let ProposalWire { body, signature } = decode_wire("proposal", &encoded)?;
        Proposal::checked(encoded, body, &signature)
    }
}

/// One member's approval of a [`Proposal`].
///
/// Made by [`Multisig::approve`](crate::Multisig::approve) and handed back to whoever collects
/// approvals for [`Multisig::execute`](crate::Multisig::execute). It approves exactly one
/// proposal, and carries the member's signature over its payout request.
///
/// It round-trips through [`Display`](core::fmt::Display) and [`FromStr`](core::str::FromStr), as
/// `msapproval` followed by lowercase base32. The parse checks the structure, not the signature:
/// [`Multisig::execute`](crate::Multisig::execute) checks it against the proposal, and the
/// federation after it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
pub struct Approval {
    /// The encoding, version byte first: what `Display` prints.
    encoded: Vec<u8>,
    /// The digest of the proposal this approves.
    proposal: sha256::Hash,
    member: MemberKey,
    /// The member's BIP-340 signature over the proposal's payout request, as the stability pool
    /// checks it.
    signature: schnorr::Signature,
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
impl Approval {
    /// Parses an approval from its printed form.
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for a malformed value, and for one
    /// written in another version of this SDK's encoding than the one this one reads.
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
    pub(crate) fn new(
        proposal: sha256::Hash,
        member: MemberKey,
        signature: schnorr::Signature,
    ) -> Approval {
        let wire = ApprovalWire {
            proposal,
            member: member.upstream(),
            signature,
        };
        let mut encoded = vec![VERSION];
        encoded.extend(wire.consensus_encode_to_vec());
        Approval {
            encoded,
            proposal,
            member,
            signature,
        }
    }

    /// The member's signature over the proposal's payout request.
    pub(crate) fn signature(&self) -> schnorr::Signature {
        self.signature
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
    /// one written in another version of the encoding.
    fn from_str(s: &str) -> Result<Approval> {
        let encoded = decode("approval", APPROVAL_PREFIX, s)?;
        let wire: ApprovalWire = decode_wire("approval", &encoded)?;
        Ok(Approval {
            encoded,
            proposal: wire.proposal,
            member: MemberKey::from_upstream(wire.member),
            signature: wire.signature,
        })
    }
}

/// The multisig account `request` pays out of, or the refusal that says why it pays out of none.
///
/// Decoding a request checks the accounts' own invariants but not what the module's constructor
/// checks of a payout, so those are checked here: both accounts hold bitcoin, the payment leaves
/// the account, and it is for something.
fn account_paid_out_of(request: &BtcTransferRequest) -> Result<MultisigAccount> {
    let from = request.from();
    if from.acc_type() != AccountType::BtcDepositor
        || request.to().acc_type() != AccountType::BtcDepositor
    {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "the proposal's payout request is not between bitcoin accounts",
        ));
    }
    if *request.to() == from.id() {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "the proposal's payout request pays the account back into itself",
        ));
    }
    if request.amount() == fedimint_core::Amount::ZERO {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "cannot pay out nothing",
        ));
    }
    let threshold = u32::try_from(from.threshold()).map_err(|_| {
        Error::new(
            ErrorCode::InvalidInput,
            "the proposal's account has an impossible threshold",
        )
    })?;
    let members = from
        .pub_keys()
        .copied()
        .map(MemberKey::from_upstream)
        .collect();
    MultisigAccount::derive(&Policy::new(members, threshold))
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
/// Checked here, before anything is signed or sent: each approval is for this proposal and from a member of
/// its account, and there are as many distinct members as the threshold asks for. A member's
/// second approval is dropped rather than refused, since collecting the same one twice is an
/// ordinary thing for a relay to do. The signatures are checked when the payout is signed, and by the
/// federation after that.
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
    federation: config::FederationId,
    proposer: PublicKey,
    request: BtcTransferRequest,
}

impl ProposalBody {
    fn new(
        federation: config::FederationId,
        proposer: MemberKey,
        request: &BtcTransferRequest,
    ) -> ProposalBody {
        ProposalBody {
            federation,
            proposer: proposer.upstream(),
            request: request.clone(),
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
    signature: schnorr::Signature,
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
                 version {VERSION}"
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

/// The first test member's proposal to pay `msats` out of the test members' 2-of-3 account to an
/// account of its own, made on `federation`.
#[cfg(test)]
pub(crate) fn test_proposal_on(federation: config::FederationId, msats: u64) -> Proposal {
    let [pair, ..] = super::account::test_keypairs();
    let proposer = MemberKey::from_upstream(pair.public_key());
    let request = test_request(msats);
    let digest = Proposal::digest_of(federation, proposer, &request);
    let signature = pair.sign_schnorr(Message::from_digest(digest.to_byte_array()));
    Proposal::signed(federation, proposer, request, signature).expect("a member's own proposal")
}

/// [`test_proposal_on`] the dummy federation, which the tests' facade runs on.
#[cfg(test)]
pub(crate) fn test_proposal(msats: u64) -> Proposal {
    test_proposal_on(config::FederationId::dummy(), msats)
}

/// A request to pay `msats` out of the test members' 2-of-3 account to the first member's own
/// account.
#[cfg(test)]
fn test_request(msats: u64) -> BtcTransferRequest {
    let [a, b, c] = super::account::test_keys();
    let account = MultisigAccount::derive(&Policy::new(vec![a, b, c], 2)).expect("a valid policy");
    let from = super::spv2::module_account(&account).expect("a valid account");
    let to = stability_pool_common::Account::single(a.upstream(), AccountType::BtcDepositor);
    BtcTransferRequest::new(
        7,
        from,
        fedimint_core::Amount::from_msats(msats),
        to.id(),
        vec![],
        100,
    )
    .expect("a valid request")
}

#[cfg(test)]
mod tests {
    use fedimint_core::secp256k1::Keypair;
    use stability_pool_common::Account;

    use super::*;
    use crate::multisig::account::{test_keypairs, test_keys};

    fn federation() -> config::FederationId {
        config::FederationId::dummy()
    }

    fn account() -> MultisigAccount {
        let [a, b, c] = test_keys();
        MultisigAccount::derive(&Policy::new(vec![a, b, c], 2)).expect("a valid policy")
    }

    /// `signer`'s signature over the proposal of `request` by `proposer`, on `federation`.
    fn signature_on(
        federation: config::FederationId,
        signer: &Keypair,
        proposer: MemberKey,
        request: &BtcTransferRequest,
    ) -> schnorr::Signature {
        let digest = Proposal::digest_of(federation, proposer, request);
        signer.sign_schnorr(Message::from_digest(digest.to_byte_array()))
    }

    /// The printed form of a proposal with `body`, signed with `signature`, whatever they are.
    fn printed(body: ProposalBody, signature: schnorr::Signature) -> String {
        let mut encoded = vec![VERSION];
        encoded.extend(ProposalWire { body, signature }.consensus_encode_to_vec());
        fedimint_core::base32::encode_prefixed_bytes(PROPOSAL_PREFIX, &encoded)
    }

    #[test]
    fn a_proposal_round_trips_through_its_printed_form() {
        let proposal = test_proposal(5_000);
        let printed = proposal.to_string();
        assert!(printed.starts_with("msproposal"), "{printed}");

        let parsed: Proposal = printed.parse().expect("parses");
        assert_eq!(parsed, proposal);
        assert_eq!(parsed.account_id(), account().id());
        assert_eq!(parsed.policy(), account().policy());
        assert_eq!(parsed.amount(), Amount::from_msats(5_000));
        assert_eq!(parsed.federation().inner(), federation());
        assert_eq!(parsed.proposer(), test_keys()[0]);
        assert_eq!(parsed.request(), &test_request(5_000));
        // Case does not matter, which is what lets a QR code carry it in alphanumeric mode.
        let shouted: Proposal = printed.to_uppercase().parse().expect("parses");
        assert_eq!(shouted, proposal);
    }

    #[test]
    fn the_account_is_the_one_the_request_pays_out_of() {
        // The same members in another order, and the threshold the module's account has: the
        // account read off the request is the one the policy describes.
        let [a, b, c] = test_keys();
        let listed = MultisigAccount::derive(&Policy::new(vec![c, a, b], 2)).expect("valid");
        assert_eq!(test_proposal(5_000).account(), listed);
        let other = MultisigAccount::derive(&Policy::new(vec![a, b, c], 3)).expect("valid");
        assert_ne!(test_proposal(5_000).account_id(), other.id());
    }

    #[test]
    fn a_proposal_is_only_made_by_the_member_who_signs_it() {
        let [a, b, _] = test_keypairs();
        let [key_a, ..] = test_keys();
        let request = test_request(5_000);

        // Signed by b in a's name.
        let err = Proposal::signed(
            federation(),
            key_a,
            request.clone(),
            signature_on(federation(), &b, key_a, &request),
        )
        .expect_err("not a's signature");
        assert_eq!(err.code, ErrorCode::InvalidInput);

        // Signed by a, then altered.
        let err = Proposal::signed(
            federation(),
            key_a,
            test_request(6_000),
            signature_on(federation(), &a, key_a, &request),
        )
        .expect_err("a signed another amount");
        assert_eq!(err.code, ErrorCode::InvalidInput);

        // Signed by a, then moved to another federation.
        let elsewhere = config::FederationId(sha256::Hash::hash(b"another federation"));
        let err = Proposal::signed(
            elsewhere,
            key_a,
            request.clone(),
            signature_on(federation(), &a, key_a, &request),
        )
        .expect_err("a signed for another federation");
        assert_eq!(err.code, ErrorCode::InvalidInput);

        // The parse checks the same, on whatever arrives over the channel.
        let forged = printed(
            ProposalBody::new(federation(), key_a, &request),
            signature_on(federation(), &b, key_a, &request),
        );
        let err = forged.parse::<Proposal>().expect_err("not a's signature");
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn a_proposal_is_only_made_by_a_member_of_the_account() {
        let outsider = Keypair::from_seckey_slice(SECP256K1, &[7; 32]).expect("a valid key");
        let key = MemberKey::from_upstream(outsider.public_key());
        let request = test_request(5_000);
        let err = Proposal::signed(
            federation(),
            key,
            request.clone(),
            signature_on(federation(), &outsider, key, &request),
        )
        .expect_err("not a member");
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn a_request_that_is_no_payout_out_of_a_multisig_account_is_refused() {
        let [a, ..] = test_keypairs();
        let [key_a, ..] = test_keys();
        let from = crate::multisig::spv2::module_account(&account()).expect("valid");
        let own = Account::single(key_a.upstream(), AccountType::BtcDepositor);
        let seeker = Account::single(key_a.upstream(), AccountType::Seeker);
        // Built as decoding builds them, without the constructor's checks.
        let raw = |from: &Account, to: &Account, msats: u64| {
            let mut bytes = Vec::new();
            7u64.consensus_encode(&mut bytes).expect("encodes");
            from.consensus_encode(&mut bytes).expect("encodes");
            fedimint_core::Amount::from_msats(msats)
                .consensus_encode(&mut bytes)
                .expect("encodes");
            to.id().consensus_encode(&mut bytes).expect("encodes");
            Vec::<u8>::new()
                .consensus_encode(&mut bytes)
                .expect("encodes");
            100u64.consensus_encode(&mut bytes).expect("encodes");
            BtcTransferRequest::consensus_decode_whole(&bytes, &ModuleDecoderRegistry::default())
                .expect("decodes")
        };
        // The encoding above is the request's own.
        assert_eq!(raw(&from, &own, 5_000), test_request(5_000));

        for request in [
            raw(&from, &from, 5_000),
            raw(&from, &own, 0),
            raw(&from, &seeker, 5_000),
        ] {
            let body = ProposalBody::new(federation(), key_a, &request);
            let signature = signature_on(federation(), &a, key_a, &request);
            let err = printed(body, signature)
                .parse::<Proposal>()
                .expect_err("refused");
            assert_eq!(err.code, ErrorCode::InvalidInput, "{request:?}");
        }
    }

    #[test]
    fn approvals_carry_over_to_the_same_proposal_signed_again() {
        let [a, b, _] = test_keypairs();
        let [key_a, key_b, _] = test_keys();
        let request = test_request(5_000);
        let digest = Proposal::digest_of(federation(), key_a, &request);
        let message = Message::from_digest(digest.to_byte_array());
        let signed_with = |aux| {
            Proposal::signed(
                federation(),
                key_a,
                request.clone(),
                SECP256K1.sign_schnorr_with_aux_rand(&message, &a, &aux),
            )
            .expect("a's own proposal")
        };
        let first = signed_with([1; 32]);
        let again = signed_with([2; 32]);
        assert_ne!(first.to_string(), again.to_string());

        let approval = Approval::new(
            first.digest(),
            key_b,
            crate::multisig::spv2::sign_request(&b, first.request()),
        );
        assert!(approval.approves(&again));
    }

    #[test]
    fn an_approval_round_trips_and_knows_its_proposal() {
        let [a, ..] = test_keypairs();
        let [key_a, ..] = test_keys();
        let proposal = test_proposal(5_000);
        let approval = Approval::new(
            proposal.digest(),
            key_a,
            crate::multisig::spv2::sign_request(&a, proposal.request()),
        );
        let parsed: Approval = approval.to_string().parse().expect("parses");
        assert_eq!(parsed, approval);
        assert_eq!(parsed.member(), key_a);
        assert_eq!(parsed.signature(), approval.signature());
        assert!(parsed.approves(&proposal));
        assert!(!parsed.approves(&test_proposal(6_000)));
    }

    #[test]
    fn what_is_not_a_proposal_is_refused() {
        let printed = test_proposal(5_000).to_string();
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
    fn another_version_of_the_encoding_is_refused_by_name() {
        for version in [1, VERSION + 1] {
            let mut encoded = test_proposal(5_000).encoded;
            encoded[0] = version;
            let printed = fedimint_core::base32::encode_prefixed_bytes(PROPOSAL_PREFIX, &encoded);
            let err = printed.parse::<Proposal>().expect_err("refused");
            assert_eq!(err.code, ErrorCode::InvalidInput);
            assert!(
                err.message.contains(&format!("version {version}")),
                "{}",
                err.message
            );
        }
    }

    #[test]
    fn execution_needs_threshold_many_distinct_members_of_the_account() {
        let [pair_a, pair_b, _] = test_keypairs();
        let [a, b, _] = test_keys();
        let proposal = test_proposal(5_000);
        let approval = |pair: &Keypair| {
            Approval::new(
                proposal.digest(),
                MemberKey::from_upstream(pair.public_key()),
                crate::multisig::spv2::sign_request(pair, proposal.request()),
            )
        };

        let err = approvals_for(&proposal, &[approval(&pair_a), approval(&pair_a)])
            .expect_err("one member twice is one approval");
        assert_eq!(err.code, ErrorCode::InvalidInput);

        let kept = approvals_for(
            &proposal,
            &[approval(&pair_b), approval(&pair_a), approval(&pair_a)],
        )
        .expect("two members meet a threshold of two");
        assert_eq!(
            kept.iter().map(Approval::member).collect::<Vec<_>>(),
            vec![a, b]
        );
    }

    #[test]
    fn execution_refuses_approvals_that_are_not_for_it() {
        let [pair_a, pair_b, _] = test_keypairs();
        let [a, b, _] = test_keys();
        let proposal = test_proposal(5_000);
        let signature =
            |pair: &Keypair| crate::multisig::spv2::sign_request(pair, proposal.request());

        let elsewhere = Approval::new(
            sha256::Hash::hash(b"another proposal"),
            a,
            signature(&pair_a),
        );
        let err = approvals_for(
            &proposal,
            &[
                elsewhere,
                Approval::new(proposal.digest(), b, signature(&pair_b)),
            ],
        )
        .expect_err("refused");
        assert_eq!(err.code, ErrorCode::InvalidInput);

        let outsider = Keypair::from_seckey_slice(SECP256K1, &[7; 32]).expect("a valid key");
        let err = approvals_for(
            &proposal,
            &[
                Approval::new(
                    proposal.digest(),
                    MemberKey::from_upstream(outsider.public_key()),
                    signature(&outsider),
                ),
                Approval::new(proposal.digest(), a, signature(&pair_a)),
            ],
        )
        .expect_err("refused");
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn approvals_are_filed_under_the_modules_key_order() {
        // The members as a policy lists them need not be the module's order; each signature has
        // to land under its key's index there, or the federation would refuse the payout.
        let [pair_a, pair_b, pair_c] = test_keypairs();
        let proposal = test_proposal(5_000);
        let approval = |pair: &Keypair| {
            Approval::new(
                proposal.digest(),
                MemberKey::from_upstream(pair.public_key()),
                crate::multisig::spv2::sign_request(pair, proposal.request()),
            )
        };
        for approvals in [
            vec![approval(&pair_c), approval(&pair_b)],
            vec![approval(&pair_a), approval(&pair_c)],
        ] {
            let signed = crate::multisig::spv2::signed_request(&proposal, &approvals)
                .expect("the approvals verify");
            signed
                .validate_signatures()
                .expect("as the module checks them");
            assert_eq!(signed.signatures().len(), 2);
        }
    }
}
