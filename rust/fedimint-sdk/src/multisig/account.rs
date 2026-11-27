//! Who a group is: member keys, policies and accounts.

use fedimint_core::bitcoin::hashes::{Hash, sha256};
use fedimint_core::encoding::{Decodable, Encodable};
use fedimint_core::secp256k1::PublicKey;
use stability_pool_common::{Account, AccountType, AccountUnchecked};

use crate::{Error, ErrorCode, Result};

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
}

impl Policy {
    /// A policy over `members`, `threshold` of whom have to approve a payment.
    ///
    /// Nothing is checked here; [`Multisig::account`](crate::Multisig::account) does that.
    pub fn new(members: Vec<MemberKey>, threshold: u32) -> Policy {
        Policy { members, threshold }
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

    /// The stability pool's account for this one: a bitcoin account whose keys are the policy's
    /// members and whose threshold is the policy's.
    pub(crate) fn module_account(&self) -> Result<Account> {
        Account::try_from(AccountUnchecked {
            acc_type: AccountType::BtcDepositor,
            pub_keys: self.members().iter().map(MemberKey::upstream).collect(),
            threshold: u64::from(self.threshold()),
        })
        // Only a policy that `derive` accepted gets here, so this is a policy the module does not
        // accept although the facade does.
        .map_err(|err| {
            Error::new(
                ErrorCode::Internal,
                format!("the stability pool refuses the policy: {err}"),
            )
        })
    }
}

/// A policy as it is hashed into an account id.
///
/// Storage format, in the sense that an id computed from it is shown to people and kept by
/// applications: change a field and every account id changes.
#[derive(Debug, Clone, PartialEq, Eq, Encodable, Decodable)]
struct PolicyWire {
    threshold: u32,
    members: Vec<PublicKey>,
}

impl From<&Policy> for PolicyWire {
    fn from(policy: &Policy) -> PolicyWire {
        PolicyWire {
            threshold: policy.threshold,
            members: policy.members.iter().map(MemberKey::upstream).collect(),
        }
    }
}

/// Three distinct member key pairs, sorted by their public keys: the secret keys 1, 2 and 3.
#[cfg(test)]
pub(crate) fn test_keypairs() -> [fedimint_core::secp256k1::Keypair; 3] {
    let mut pairs = [1u8, 2, 3].map(|n| {
        let mut secret = [0u8; 32];
        secret[31] = n;
        fedimint_core::secp256k1::Keypair::from_seckey_slice(
            fedimint_core::secp256k1::SECP256K1,
            &secret,
        )
        .expect("a valid secret key")
    });
    pairs.sort_by_key(|pair| MemberKey::from_upstream(pair.public_key()));
    pairs
}

/// Three valid, distinct keys, sorted: the curve points G, 2G and 3G.
#[cfg(test)]
pub(crate) fn test_keys() -> [MemberKey; 3] {
    test_keypairs().map(|pair| MemberKey::from_upstream(pair.public_key()))
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
        let one = MultisigAccount::derive(&Policy::new(vec![c, a, b], 2)).expect("a valid policy");
        let other =
            MultisigAccount::derive(&Policy::new(vec![a, b, c], 2)).expect("a valid policy");
        assert_eq!(one.id(), other.id());
        assert_eq!(one.policy().members, vec![a, b, c]);
    }

    #[test]
    fn the_threshold_is_part_of_the_account() {
        let [a, b, c] = keys();
        let id = |threshold| {
            MultisigAccount::derive(&Policy::new(vec![a, b, c], threshold))
                .expect("a valid policy")
                .id()
        };
        assert_ne!(id(2), id(3));
    }

    #[test]
    fn a_policy_that_describes_no_account_is_refused() {
        let [a, b, _] = keys();
        for policy in [
            Policy::new(vec![], 1),
            Policy::new(vec![a, b], 0),
            Policy::new(vec![a, b], 3),
            Policy::new(vec![a, b, a], 2),
        ] {
            let err = MultisigAccount::derive(&policy).expect_err("refused");
            assert_eq!(err.code, ErrorCode::InvalidInput, "{policy:?}");
        }
    }

    #[test]
    fn an_account_id_round_trips() {
        let [a, b, c] = keys();
        let account =
            MultisigAccount::derive(&Policy::new(vec![a, b, c], 2)).expect("a valid policy");
        let printed = account.id().to_string();
        assert_eq!(printed.len(), 64);
        assert_eq!(
            printed.parse::<MultisigAccountId>().expect("parses"),
            account.id()
        );
        assert!("not hex".parse::<MultisigAccountId>().is_err());
    }
}
