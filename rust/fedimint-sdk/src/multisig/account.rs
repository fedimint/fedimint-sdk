//! Who a group is and what it holds: member keys, policies, accounts and amounts.

use fedimint_core::bitcoin::hashes::{Hash, sha256};
use fedimint_core::encoding::{Decodable, Encodable};
use fedimint_core::secp256k1::PublicKey;

use crate::{Amount, Error, ErrorCode, Result};

/// One member's public key in a multisig group.
///
/// A member has a key of its own in every group, from
/// [`Multisig::member_key`](crate::Multisig::member_key), so that two groups cannot be linked
/// through a shared key. The members exchange their keys to agree on a [`Policy`].
///
/// Not a secret. It round-trips through [`Display`](core::fmt::Display) and
/// [`FromStr`](core::str::FromStr) as 66 lowercase hex characters, and the parse checks that the
/// value really is a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MemberKey {
    key: PublicKey,
}

impl MemberKey {
    /// Wraps a key that is already known to be valid.
    pub(crate) fn from_upstream(key: PublicKey) -> MemberKey {
        MemberKey { key }
    }

    /// The key itself.
    pub(crate) fn upstream(&self) -> PublicKey {
        self.key
    }
}

#[cfg(feature = "extension-api")]
impl MemberKey {
    /// The member key for `key`, for a [`MultisigBackend`](crate::MultisigBackend) handing one
    /// out.
    pub fn from_public_key(key: PublicKey) -> MemberKey {
        MemberKey::from_upstream(key)
    }

    /// The key itself, for a [`MultisigBackend`](crate::MultisigBackend) building the module's
    /// own account from a policy.
    pub fn public_key(&self) -> PublicKey {
        self.key
    }
}

impl core::fmt::Display for MemberKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Always the compressed spelling, 66 lowercase hex characters
        // (secp256k1-0.29.1/src/key.rs:152-164).
        core::fmt::Display::fmt(&self.key, f)
    }
}

impl core::str::FromStr for MemberKey {
    type Err = Error;

    /// Parses a member key. Returns [`ErrorCode::InvalidInput`] for a malformed value.
    fn from_str(s: &str) -> Result<MemberKey> {
        let key = s.trim().parse::<PublicKey>().map_err(|err| {
            Error::new(
                ErrorCode::InvalidInput,
                format!("invalid member key: {err}"),
            )
        })?;
        Ok(MemberKey { key })
    }
}

/// What a multisig account holds its value in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[non_exhaustive]
pub enum MultisigDenomination {
    /// Bitcoin, counted in millisatoshi.
    Btc,
    /// A fiat currency, held at a stable value: the account's bitcoin is converted at the
    /// federation's exchange rate whenever it moves.
    Fiat,
}

impl MultisigDenomination {
    /// The tag this denomination is encoded and hashed as.
    pub(crate) fn tag(self) -> u8 {
        match self {
            MultisigDenomination::Btc => 0,
            MultisigDenomination::Fiat => 1,
        }
    }

    /// The denomination a tag names, if any.
    pub(crate) fn from_tag(tag: u8) -> Option<MultisigDenomination> {
        match tag {
            0 => Some(MultisigDenomination::Btc),
            1 => Some(MultisigDenomination::Fiat),
            _ => None,
        }
    }
}

/// An amount in a multisig account's own denomination.
///
/// What [`Multisig::balance`](crate::Multisig::balance) reports and what
/// [`Multisig::propose`](crate::Multisig::propose) pays out. Money going *into* an account is
/// always taken from the ecash balance, so a deposit is a plain [`Amount`] whatever the account
/// holds its value in.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[non_exhaustive]
pub enum MultisigAmount {
    /// An amount of bitcoin.
    Btc {
        /// The amount.
        amount: Amount,
    },
    /// An amount of a fiat currency.
    Fiat {
        /// The amount in the currency's smallest unit: cents for the US dollar, whole yen for
        /// the Japanese yen.
        minor_units: u64,
        /// The currency's ISO 4217 code, `"USD"` for instance.
        currency: String,
    },
}

impl MultisigAmount {
    /// The denomination this amount is in.
    pub fn denomination(&self) -> MultisigDenomination {
        match self {
            MultisigAmount::Btc { .. } => MultisigDenomination::Btc,
            MultisigAmount::Fiat { .. } => MultisigDenomination::Fiat,
        }
    }

    /// Whether this amount is zero.
    pub(crate) fn is_zero(&self) -> bool {
        match self {
            MultisigAmount::Btc { amount } => amount.msats() == 0,
            MultisigAmount::Fiat { minor_units, .. } => *minor_units == 0,
        }
    }
}

/// Who a multisig group's members are, and how many of them it takes to move its money.
///
/// The members agree on a policy among themselves, out of band: each adds its own key from
/// [`Multisig::member_key`](crate::Multisig::member_key), and the application keeps the result.
/// [`Multisig::account`](crate::Multisig::account) turns a policy into the account it describes,
/// locally: the federation learns about the account only when value first moves into it, and
/// every member who holds the same policy derives the same account.
///
/// The order of [`members`](Policy::members) does not matter: two policies with the same members
/// in a different order describe the same account.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[non_exhaustive]
pub struct Policy {
    /// Every member's key, each exactly once.
    pub members: Vec<MemberKey>,
    /// How many members have to approve a payment out of the account: at least one, and no more
    /// than there are members.
    pub threshold: u32,
    /// What the account holds its value in.
    pub denomination: MultisigDenomination,
}

impl Policy {
    /// A policy over `members`, `threshold` of whom have to approve a payment.
    ///
    /// Nothing is checked here; [`Multisig::account`](crate::Multisig::account) does that.
    pub fn new(
        members: Vec<MemberKey>,
        threshold: u32,
        denomination: MultisigDenomination,
    ) -> Policy {
        Policy {
            members,
            threshold,
            denomination,
        }
    }

    /// This policy with its members sorted, or the refusal that says why it is no policy at all.
    fn normalised(&self) -> Result<Policy> {
        let mut members = self.members.clone();
        members.sort();
        if members.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "a policy names one member more than once",
            ));
        }
        if members.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "a policy needs at least one member",
            ));
        }
        let count = u32::try_from(members.len()).unwrap_or(u32::MAX);
        if self.threshold == 0 || self.threshold > count {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                format!(
                    "a threshold of {} is not between 1 and the {count} members",
                    self.threshold
                ),
            ));
        }
        Ok(Policy {
            members,
            threshold: self.threshold,
            denomination: self.denomination,
        })
    }
}

/// Identifies a multisig account.
///
/// A digest of the account's [`Policy`]: every member who holds the same policy computes the same
/// id, and no other policy has it. Opaque, and it round-trips through
/// [`Display`](core::fmt::Display) and [`FromStr`](core::str::FromStr) as 64 lowercase hex
/// characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MultisigAccountId {
    digest: sha256::Hash,
}

impl core::fmt::Display for MultisigAccountId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(&self.digest, f)
    }
}

impl core::str::FromStr for MultisigAccountId {
    type Err = Error;

    /// Parses an account id. Returns [`ErrorCode::InvalidInput`] for a malformed value.
    fn from_str(s: &str) -> Result<MultisigAccountId> {
        let digest = s.trim().parse::<sha256::Hash>().map_err(|err| {
            Error::new(
                ErrorCode::InvalidInput,
                format!("invalid multisig account id: {err}"),
            )
        })?;
        Ok(MultisigAccountId { digest })
    }
}

/// A multisig account: the account a [`Policy`] describes.
///
/// Obtained from [`Multisig::account`](crate::Multisig::account), which checks the policy, or
/// read off a [`Proposal`](crate::Proposal). Keep the policy rather than this value: an account is
/// derived from its policy, locally, whenever it is needed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
pub struct MultisigAccount {
    id: MultisigAccountId,
    /// Normalised: members sorted, each once, and the threshold within bounds.
    policy: Policy,
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
impl MultisigAccount {
    /// This account's id.
    pub fn id(&self) -> MultisigAccountId {
        self.id
    }

    /// The policy this account was derived from, with its members sorted.
    pub fn policy(&self) -> Policy {
        self.policy.clone()
    }
}

impl MultisigAccount {
    /// The account `policy` describes, or the refusal that says why the policy describes none.
    pub(crate) fn derive(policy: &Policy) -> Result<MultisigAccount> {
        let policy = policy.normalised()?;
        let wire = PolicyWire::from(&policy);
        let digest = sha256::Hash::hash(&wire.consensus_encode_to_vec());
        Ok(MultisigAccount {
            id: MultisigAccountId { digest },
            policy,
        })
    }

    /// The policy's members, sorted.
    pub(crate) fn members(&self) -> &[MemberKey] {
        &self.policy.members
    }

    /// How many members have to approve a payment.
    pub(crate) fn threshold(&self) -> u32 {
        self.policy.threshold
    }

    /// What the account holds its value in.
    pub(crate) fn denomination(&self) -> MultisigDenomination {
        self.policy.denomination
    }
}

/// A policy as it is hashed into an account id and encoded into a proposal.
///
/// Storage format, in the sense that an id computed from it is shown to people and kept by
/// applications: change a field and every account id changes.
#[derive(Debug, Clone, PartialEq, Eq, Encodable, Decodable)]
pub(crate) struct PolicyWire {
    pub(crate) denomination: u8,
    pub(crate) threshold: u32,
    pub(crate) members: Vec<PublicKey>,
}

impl From<&Policy> for PolicyWire {
    fn from(policy: &Policy) -> PolicyWire {
        PolicyWire {
            denomination: policy.denomination.tag(),
            threshold: policy.threshold,
            members: policy.members.iter().map(MemberKey::upstream).collect(),
        }
    }
}

impl TryFrom<PolicyWire> for Policy {
    type Error = Error;

    fn try_from(wire: PolicyWire) -> Result<Policy> {
        let denomination = MultisigDenomination::from_tag(wire.denomination).ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidInput,
                format!("unknown denomination {}", wire.denomination),
            )
        })?;
        Ok(Policy {
            members: wire
                .members
                .into_iter()
                .map(MemberKey::from_upstream)
                .collect(),
            threshold: wire.threshold,
            denomination,
        })
    }
}

/// An amount as it is encoded into a proposal.
#[derive(Debug, Clone, PartialEq, Eq, Encodable, Decodable)]
pub(crate) struct AmountWire {
    pub(crate) denomination: u8,
    /// Millisatoshi for bitcoin, the currency's smallest unit for fiat.
    pub(crate) value: u64,
    /// Empty for bitcoin.
    pub(crate) currency: String,
}

impl From<&MultisigAmount> for AmountWire {
    fn from(amount: &MultisigAmount) -> AmountWire {
        match amount {
            MultisigAmount::Btc { amount } => AmountWire {
                denomination: MultisigDenomination::Btc.tag(),
                value: amount.msats(),
                currency: String::new(),
            },
            MultisigAmount::Fiat {
                minor_units,
                currency,
            } => AmountWire {
                denomination: MultisigDenomination::Fiat.tag(),
                value: *minor_units,
                currency: currency.clone(),
            },
        }
    }
}

impl TryFrom<AmountWire> for MultisigAmount {
    type Error = Error;

    fn try_from(wire: AmountWire) -> Result<MultisigAmount> {
        match MultisigDenomination::from_tag(wire.denomination) {
            Some(MultisigDenomination::Btc) => Ok(MultisigAmount::Btc {
                amount: Amount::from_msats(wire.value),
            }),
            Some(MultisigDenomination::Fiat) => Ok(MultisigAmount::Fiat {
                minor_units: wire.value,
                currency: wire.currency,
            }),
            None => Err(Error::new(
                ErrorCode::InvalidInput,
                format!("unknown denomination {}", wire.denomination),
            )),
        }
    }
}

/// Three valid, distinct keys, sorted: the curve points G, 2G and 3G.
#[cfg(test)]
pub(crate) fn test_keys() -> [MemberKey; 3] {
    let mut keys = [
        "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
        "02c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5",
        "02f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9",
    ]
    .map(|key| key.parse::<MemberKey>().expect("a valid key"));
    keys.sort();
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> [MemberKey; 3] {
        test_keys()
    }

    #[test]
    fn a_member_key_round_trips_and_rejects_what_is_not_a_key() {
        let [key, ..] = keys();
        assert_eq!(key.to_string().parse::<MemberKey>().expect("parses"), key);

        let err = "02c6047f".parse::<MemberKey>().expect_err("too short");
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn the_order_of_members_does_not_change_the_account() {
        let [a, b, c] = keys();
        let one =
            MultisigAccount::derive(&Policy::new(vec![c, a, b], 2, MultisigDenomination::Btc))
                .expect("a valid policy");
        let other =
            MultisigAccount::derive(&Policy::new(vec![a, b, c], 2, MultisigDenomination::Btc))
                .expect("a valid policy");
        assert_eq!(one.id(), other.id());
        assert_eq!(one.policy().members, vec![a, b, c]);
    }

    #[test]
    fn threshold_and_denomination_are_part_of_the_account() {
        let [a, b, c] = keys();
        let id = |threshold, denomination| {
            MultisigAccount::derive(&Policy::new(vec![a, b, c], threshold, denomination))
                .expect("a valid policy")
                .id()
        };
        assert_ne!(
            id(2, MultisigDenomination::Btc),
            id(3, MultisigDenomination::Btc)
        );
        assert_ne!(
            id(2, MultisigDenomination::Btc),
            id(2, MultisigDenomination::Fiat)
        );
    }

    #[test]
    fn a_policy_that_describes_no_account_is_refused() {
        let [a, b, _] = keys();
        for policy in [
            Policy::new(vec![], 1, MultisigDenomination::Btc),
            Policy::new(vec![a, b], 0, MultisigDenomination::Btc),
            Policy::new(vec![a, b], 3, MultisigDenomination::Btc),
            Policy::new(vec![a, b, a], 2, MultisigDenomination::Btc),
        ] {
            let err = MultisigAccount::derive(&policy).expect_err("refused");
            assert_eq!(err.code, ErrorCode::InvalidInput, "{policy:?}");
        }
    }

    #[test]
    fn an_account_id_round_trips() {
        let [a, b, c] = keys();
        let account =
            MultisigAccount::derive(&Policy::new(vec![a, b, c], 2, MultisigDenomination::Btc))
                .expect("a valid policy");
        let printed = account.id().to_string();
        assert_eq!(printed.len(), 64);
        assert_eq!(
            printed.parse::<MultisigAccountId>().expect("parses"),
            account.id()
        );
        assert!("not hex".parse::<MultisigAccountId>().is_err());
    }

    #[test]
    fn an_amount_round_trips_through_its_wire_form() {
        for amount in [
            MultisigAmount::Btc {
                amount: Amount::from_msats(1_234),
            },
            MultisigAmount::Fiat {
                minor_units: 500,
                currency: "USD".to_owned(),
            },
        ] {
            let back = MultisigAmount::try_from(AmountWire::from(&amount)).expect("decodes");
            assert_eq!(back, amount);
        }
    }
}
