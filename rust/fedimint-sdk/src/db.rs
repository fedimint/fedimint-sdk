//! The SDK's own records, and the byte layout that lets many federations share one store.
//!
//! The root keyspace is split by a one-byte tag:
//!
//! ```text
//! 0x00                              the SDK's own records (this module)
//! 0x01 ++ <32-byte federation id>   one federation's namespace, handed to fedimint-client
//! ```
//!
//! Both are fixed width, which matters because `Database::with_prefix` concatenates bytes with no
//! length delimiter: a variable-width tag could make one federation's keyspace start inside
//! another's. Inside a federation's namespace the SDK owns `0xb0..=0xcf`, which upstream reserves
//! for embedders; the operation records at `0xb0` arrive with the operation infrastructure.
//!
//! Every read and write here goes through the *raw* transaction ops. The typed ones
//! (`get_value`, `insert_entry`, `commit_tx`) call `.expect(..)` on a backend failure and on a
//! decode failure, and this crate reports storage trouble as `ErrorCode::Storage` rather than
//! taking the host application down with it.

use fedimint_core::config::FederationId;
use fedimint_core::db::{
    Committable, Database, DatabaseKey, DatabaseKeyPrefix, DatabaseRecord, DatabaseTransaction,
    DatabaseValue, IDatabaseTransactionOpsCore,
};
use fedimint_core::encoding::{Decodable, DecodeError, Encodable};
use fedimint_core::invite_code::InviteCode;
use fedimint_core::module::registry::ModuleDecoderRegistry;
use fedimint_core::{impl_db_lookup, impl_db_record};
use futures::StreamExt;

use crate::{Capabilities, Error, ErrorCode, Network, Result};

/// The tag the SDK's own records live under.
pub(crate) const SDK_NAMESPACE_TAG: u8 = 0x00;

/// The tag every federation namespace starts with.
pub(crate) const FEDERATION_NAMESPACE_TAG: u8 = 0x01;

/// The prefix `fedimint-client` is handed for the federation `id`.
pub(crate) fn federation_prefix(id: &FederationId) -> [u8; 33] {
    let mut prefix = [0u8; 33];
    prefix[0] = FEDERATION_NAMESPACE_TAG;
    prefix[1..].copy_from_slice(id.0.as_ref());
    prefix
}

/// The prefix the SDK's own records live under.
pub(crate) fn sdk_prefix() -> [u8; 1] {
    [SDK_NAMESPACE_TAG]
}

/// The record kinds inside the SDK's own namespace.
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub(crate) enum SdkDbPrefix {
    /// The one seed record an instance has.
    Seed = 0x01,
    /// One row per federation the storage remembers, in any state.
    Federation = 0x02,
}

/// The key of the single seed record.
#[derive(Debug, Clone, Encodable, Decodable)]
pub(crate) struct SeedKey;

/// Query prefix for [`SeedKey`], so the seed shows up in a full scan.
#[derive(Debug, Clone, Encodable, Decodable)]
pub(crate) struct SeedKeyPrefix;

/// The instance's BIP-39 phrase, stored as the words joined by single spaces.
#[derive(Debug, Clone, PartialEq, Eq, Encodable, Decodable)]
pub(crate) struct SeedRecord {
    pub(crate) phrase: String,
}

impl_db_record!(
    key = SeedKey,
    value = SeedRecord,
    db_prefix = SdkDbPrefix::Seed
);
impl_db_lookup!(key = SeedKey, query_prefix = SeedKeyPrefix);

/// The key of one federation's registry row.
#[derive(Debug, Clone, PartialEq, Eq, Encodable, Decodable)]
pub(crate) struct FederationKey(pub(crate) FederationId);

/// Query prefix for every [`FederationKey`].
#[derive(Debug, Clone, Encodable, Decodable)]
pub(crate) struct FederationKeyPrefix;

/// What the storage remembers about a federation's place in the lifecycle.
///
/// This is deliberately narrower than the public `FederationStatus`: quarantine and recovery are
/// facts about a running instance, not about the storage, and must not survive a restart as
/// anything other than "reopen this one and see".
// The `Encodable`/`Decodable` pair below is hand-written because the enum `Decodable` derive
// expands to unqualified `anyhow::` paths and this crate has no `anyhow`; see the note above
// this step. One `u8` tag per variant, so a stored row is one byte and an unknown tag is
// rejected rather than guessed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StoredStatus {
    /// A join was committed but the client state was not finished being written.
    ///
    /// The next open wipes the namespace and redoes the join from `invite`: no value can exist
    /// there yet, because the caller never received a handle to receive with.
    Joining,
    /// Reopen this federation on every build.
    Open,
    /// Left alone deliberately with `close_federation`; later builds do not reopen it.
    Closed,
    /// An erase is committed and owed. Never opened again, never resurrected.
    Forgetting,
}

impl Encodable for StoredStatus {
    fn consensus_encode<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let tag: u8 = match self {
            StoredStatus::Joining => 0,
            StoredStatus::Open => 1,
            StoredStatus::Closed => 2,
            StoredStatus::Forgetting => 3,
        };
        tag.consensus_encode(writer)
    }
}

impl Decodable for StoredStatus {
    fn consensus_decode_partial<D: std::io::Read>(
        d: &mut D,
        modules: &ModuleDecoderRegistry,
    ) -> std::result::Result<StoredStatus, DecodeError> {
        match u8::consensus_decode_partial(d, modules)? {
            0 => Ok(StoredStatus::Joining),
            1 => Ok(StoredStatus::Open),
            2 => Ok(StoredStatus::Closed),
            3 => Ok(StoredStatus::Forgetting),
            _ => Err(DecodeError::from_str("unknown StoredStatus tag")),
        }
    }
}

/// The capability set of the last configuration that validated.
///
/// A storable mirror of the public `Capabilities`: the public type must not carry an upstream
/// encoding trait, and this one must not grow a field without a storage format decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encodable, Decodable)]
pub(crate) struct StoredCapabilities {
    pub(crate) ecash: bool,
    pub(crate) lightning: bool,
    pub(crate) onchain: bool,
}

impl From<Capabilities> for StoredCapabilities {
    fn from(capabilities: Capabilities) -> StoredCapabilities {
        StoredCapabilities {
            ecash: capabilities.ecash,
            lightning: capabilities.lightning,
            onchain: capabilities.onchain,
        }
    }
}

impl From<StoredCapabilities> for Capabilities {
    fn from(stored: StoredCapabilities) -> Capabilities {
        Capabilities {
            ecash: stored.ecash,
            lightning: stored.lightning,
            onchain: stored.onchain,
        }
    }
}

/// The Bitcoin network of the last configuration that validated.
///
/// A storable mirror of the public `Network`, for the same reason as [`StoredCapabilities`].
// Hand-written codecs, for the same reason as `StoredStatus`'.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StoredNetwork {
    Bitcoin,
    Testnet,
    Testnet4,
    Signet,
    Regtest,
}

impl Encodable for StoredNetwork {
    fn consensus_encode<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let tag: u8 = match self {
            StoredNetwork::Bitcoin => 0,
            StoredNetwork::Testnet => 1,
            StoredNetwork::Testnet4 => 2,
            StoredNetwork::Signet => 3,
            StoredNetwork::Regtest => 4,
        };
        tag.consensus_encode(writer)
    }
}

impl Decodable for StoredNetwork {
    fn consensus_decode_partial<D: std::io::Read>(
        d: &mut D,
        modules: &ModuleDecoderRegistry,
    ) -> std::result::Result<StoredNetwork, DecodeError> {
        match u8::consensus_decode_partial(d, modules)? {
            0 => Ok(StoredNetwork::Bitcoin),
            1 => Ok(StoredNetwork::Testnet),
            2 => Ok(StoredNetwork::Testnet4),
            3 => Ok(StoredNetwork::Signet),
            4 => Ok(StoredNetwork::Regtest),
            _ => Err(DecodeError::from_str("unknown StoredNetwork tag")),
        }
    }
}

impl From<Network> for StoredNetwork {
    fn from(network: Network) -> StoredNetwork {
        match network {
            Network::Bitcoin => StoredNetwork::Bitcoin,
            Network::Testnet => StoredNetwork::Testnet,
            Network::Testnet4 => StoredNetwork::Testnet4,
            Network::Signet => StoredNetwork::Signet,
            Network::Regtest => StoredNetwork::Regtest,
            // `Network` is `#[non_exhaustive]` for the sake of future test networks, but that
            // attribute does nothing inside the defining crate: this match is already exhaustive
            // here, so a catch-all arm would be an unreachable pattern and fail `-D warnings`. A
            // variant added without a storage decision breaks this match at compile time, which
            // is exactly where it should break.
        }
    }
}

impl From<StoredNetwork> for Network {
    fn from(stored: StoredNetwork) -> Network {
        match stored {
            StoredNetwork::Bitcoin => Network::Bitcoin,
            StoredNetwork::Testnet => Network::Testnet,
            StoredNetwork::Testnet4 => Network::Testnet4,
            StoredNetwork::Signet => Network::Signet,
            StoredNetwork::Regtest => Network::Regtest,
        }
    }
}

/// Everything the SDK knows about a federation without opening it.
///
/// It is what `stored_federations`, `federation_status` and the descriptive accessors on a closed
/// `Federation` answer from, which is why the whole of the last configuration that validated is
/// snapshotted here rather than derived from a live client.
///
/// `Debug` is hand-written and redacts the invite code: upstream's derived `Debug` prints every
/// part of it, `api_secret` included, and this record is reachable from the `Debug` of every
/// handle that holds it.
#[derive(Clone, PartialEq, Eq, Encodable, Decodable)]
pub(crate) struct FederationRecord {
    /// Stored as the upstream type rather than its string form, so a row that decodes at all
    /// always yields an invite code and `Federation::invite_code` cannot fail.
    pub(crate) invite: InviteCode,
    pub(crate) network: StoredNetwork,
    pub(crate) status: StoredStatus,
    pub(crate) capabilities: StoredCapabilities,
    /// `1` or `2`, or `None` for a federation whose modules declare no generation at all.
    pub(crate) generation: Option<u8>,
    pub(crate) name: Option<String>,
}

impl core::fmt::Debug for FederationRecord {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FederationRecord")
            .field("invite", &"<redacted>")
            .field("network", &self.network)
            .field("status", &self.status)
            .field("capabilities", &self.capabilities)
            .field("generation", &self.generation)
            .field("name", &self.name)
            .finish()
    }
}

impl_db_record!(
    key = FederationKey,
    value = FederationRecord,
    db_prefix = SdkDbPrefix::Federation,
);
impl_db_lookup!(key = FederationKey, query_prefix = FederationKeyPrefix);

/// Reads one record, reporting a backend or decode failure instead of panicking.
///
/// `Cap: Send` on all four helpers is upstream's bound, not a choice: the raw transaction ops are
/// only implemented for a `DatabaseTransaction` whose capability marker is `Send`.
pub(crate) async fn read<K, Cap>(
    dbtx: &mut DatabaseTransaction<'_, Cap>,
    key: &K,
) -> Result<Option<K::Value>>
where
    K: DatabaseRecord,
    Cap: Send,
{
    let key_bytes = key.to_bytes();
    let Some(value_bytes) = read_raw(dbtx, &key_bytes).await? else {
        return Ok(None);
    };
    K::Value::from_bytes(&value_bytes, &ModuleDecoderRegistry::default())
        .map(Some)
        .map_err(|err| Error::new(ErrorCode::Storage, format!("unreadable record: {err}")))
}

/// Reads raw bytes, so a caller that has to tell "absent" from "unusable" can.
pub(crate) async fn read_raw<Cap>(
    dbtx: &mut DatabaseTransaction<'_, Cap>,
    key_bytes: &[u8],
) -> Result<Option<Vec<u8>>>
where
    Cap: Send,
{
    dbtx.raw_get_bytes(key_bytes).await.map_err(backend_error)
}

/// Writes one record.
pub(crate) async fn write<K, Cap>(
    dbtx: &mut DatabaseTransaction<'_, Cap>,
    key: &K,
    value: &K::Value,
) -> Result<()>
where
    K: DatabaseRecord,
    Cap: Send,
{
    dbtx.raw_insert_bytes(&key.to_bytes(), &value.to_bytes())
        .await
        .map(|_| ())
        .map_err(backend_error)
}

/// Removes one record.
pub(crate) async fn remove<K, Cap>(dbtx: &mut DatabaseTransaction<'_, Cap>, key: &K) -> Result<()>
where
    K: DatabaseRecord,
    Cap: Send,
{
    dbtx.raw_remove_entry(&key.to_bytes())
        .await
        .map(|_| ())
        .map_err(backend_error)
}

/// Commits, reporting a failure instead of panicking the way `commit_tx` does.
pub(crate) async fn commit(dbtx: DatabaseTransaction<'_, Committable>) -> Result<()> {
    dbtx.commit_tx_result().await.map_err(backend_error)
}

/// Whether the store holds no byte this SDK could have written.
///
/// This is the emptiness proof `SdkBuilder::build` needs before it may establish a seed. It scans
/// the whole root keyspace rather than looking for particular records, because "no seed" must
/// never be read as "fresh storage".
pub(crate) async fn is_empty(db: &Database) -> Result<bool> {
    let mut dbtx = db.begin_transaction_nc().await;
    let mut entries = dbtx.raw_find_by_prefix(&[]).await.map_err(backend_error)?;
    Ok(entries.next().await.is_none())
}

/// The registry row for `id`, if the storage has one.
pub(crate) async fn read_federation(
    db: &Database,
    id: &FederationId,
) -> Result<Option<FederationRecord>> {
    let sdk_db = db.with_prefix(sdk_prefix().to_vec());
    let mut dbtx = sdk_db.begin_transaction_nc().await;
    read(&mut dbtx, &FederationKey(*id)).await
}

/// Writes the registry row for `id`, durably, before it becomes observable.
pub(crate) async fn write_federation(
    db: &Database,
    id: &FederationId,
    record: &FederationRecord,
) -> Result<()> {
    let sdk_db = db.with_prefix(sdk_prefix().to_vec());
    let mut dbtx = sdk_db.begin_transaction().await;
    write(&mut dbtx, &FederationKey(*id), record).await?;
    commit(dbtx).await
}

/// Removes the registry row for `id`. The last step of an erase.
pub(crate) async fn remove_federation(db: &Database, id: &FederationId) -> Result<()> {
    let sdk_db = db.with_prefix(sdk_prefix().to_vec());
    let mut dbtx = sdk_db.begin_transaction().await;
    remove(&mut dbtx, &FederationKey(*id)).await?;
    commit(dbtx).await
}

/// Every federation the storage remembers, in whatever state.
pub(crate) async fn list_federations(
    db: &Database,
) -> Result<Vec<(FederationId, FederationRecord)>> {
    let sdk_db = db.with_prefix(sdk_prefix().to_vec());
    let mut dbtx = sdk_db.begin_transaction_nc().await;
    // Fully qualified: a concrete key type satisfies both the `DatabaseKeyPrefix` and the
    // `DatabaseValue` blanket impls, and each has a `to_bytes`/`from_bytes` of its own.
    let raw: Vec<(Vec<u8>, Vec<u8>)> = dbtx
        .raw_find_by_prefix(&DatabaseKeyPrefix::to_bytes(&FederationKeyPrefix))
        .await
        .map_err(backend_error)?
        .collect()
        .await;
    let decoders = ModuleDecoderRegistry::default();
    raw.into_iter()
        .map(|(key_bytes, value_bytes)| {
            let key = <FederationKey as DatabaseKey>::from_bytes(&key_bytes, &decoders)
                .map_err(|err| Error::new(ErrorCode::Storage, format!("unreadable key: {err}")))?;
            let record = FederationRecord::from_bytes(&value_bytes, &decoders).map_err(|err| {
                Error::new(
                    ErrorCode::Storage,
                    format!("unreadable federation record: {err}"),
                )
            })?;
            Ok((key.0, record))
        })
        .collect()
}

/// Deletes exactly one federation's namespace and nothing else.
///
/// `raw_remove_by_prefix(&[])` on a prefixed database resolves to the parent's
/// `raw_remove_by_prefix(<that federation's prefix>)`, so this is bounded by construction rather
/// than by a range the caller has to get right.
pub(crate) async fn wipe_federation(db: &Database, id: &FederationId) -> Result<()> {
    let fed_db = db.with_prefix(federation_prefix(id).to_vec());
    let mut dbtx = fed_db.begin_transaction().await;
    dbtx.raw_remove_by_prefix(&[])
        .await
        .map_err(backend_error)?;
    commit(dbtx).await
}

/// Maps a backend failure onto the retryable code. A store whose *contents* are wrong is a
/// different situation and never arrives here.
fn backend_error(err: fedimint_core::db::DatabaseError) -> Error {
    Error::new(
        ErrorCode::Storage,
        format!("storage backend failure: {err}"),
    )
}

#[cfg(test)]
mod tests {
    use fedimint_core::PeerId;
    // `sha256::Hash::from_byte_array` is a trait method, so the trait has to be in scope.
    use fedimint_core::bitcoin::hashes::Hash;
    use fedimint_core::config::FederationId;
    use fedimint_core::db::Database;
    use fedimint_core::db::mem_impl::MemDatabase;
    use fedimint_core::module::registry::ModuleDecoderRegistry;
    use fedimint_core::util::SafeUrl;

    use super::*;

    fn db() -> Database {
        Database::new(MemDatabase::new(), ModuleDecoderRegistry::default())
    }

    fn invite(id: FederationId) -> fedimint_core::invite_code::InviteCode {
        fedimint_core::invite_code::InviteCode::new(
            SafeUrl::parse("wss://guardian.example:5000").expect("a valid url"),
            PeerId::from(0),
            id,
            None,
        )
    }

    fn record(id: FederationId) -> FederationRecord {
        FederationRecord {
            invite: invite(id),
            network: StoredNetwork::Regtest,
            status: StoredStatus::Open,
            capabilities: StoredCapabilities {
                ecash: true,
                lightning: true,
                onchain: false,
            },
            generation: Some(1),
            name: Some("Test Federation".to_owned()),
        }
    }

    #[test]
    fn the_stored_enums_round_trip_through_their_one_byte_tag() {
        // Their codecs are hand-written rather than derived, so nothing but this
        // test stands between a mistyped tag and a store that decodes as the
        // wrong federation state.
        let modules = ModuleDecoderRegistry::default();
        for status in [
            StoredStatus::Joining,
            StoredStatus::Open,
            StoredStatus::Closed,
            StoredStatus::Forgetting,
        ] {
            let bytes = status.consensus_encode_to_vec();
            assert_eq!(bytes.len(), 1, "one byte per variant");
            assert_eq!(
                StoredStatus::consensus_decode_whole(&bytes, &modules).expect("a known tag"),
                status
            );
        }
        for network in [
            StoredNetwork::Bitcoin,
            StoredNetwork::Testnet,
            StoredNetwork::Testnet4,
            StoredNetwork::Signet,
            StoredNetwork::Regtest,
        ] {
            let bytes = network.consensus_encode_to_vec();
            assert_eq!(bytes.len(), 1, "one byte per variant");
            assert_eq!(
                StoredNetwork::consensus_decode_whole(&bytes, &modules).expect("a known tag"),
                network
            );
        }
    }

    #[test]
    fn a_stored_enum_tag_this_build_does_not_know_fails_to_decode() {
        // A row written by a newer build must surface as `ErrorCode::Storage`, not
        // as a silently wrong variant.
        let modules = ModuleDecoderRegistry::default();
        assert!(StoredStatus::consensus_decode_whole(&[4], &modules).is_err());
        assert!(StoredNetwork::consensus_decode_whole(&[5], &modules).is_err());
    }

    #[test]
    fn a_federation_namespace_never_starts_inside_another_one() {
        // The prefixes are concatenated without a length, so the only thing that
        // keeps two federations apart is that every prefix is the same width and
        // no prefix is a prefix of another.
        let a = federation_prefix(&FederationId::dummy());
        let b = federation_prefix(&fedimint_core::config::FederationId(
            fedimint_core::bitcoin::hashes::sha256::Hash::from_byte_array([7; 32]),
        ));
        assert_eq!(a.len(), b.len());
        assert_ne!(a, b);
        assert_ne!(a[0], sdk_prefix()[0]);
        assert_eq!(a[0], FEDERATION_NAMESPACE_TAG);
        assert_eq!(sdk_prefix()[0], SDK_NAMESPACE_TAG);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_seed_round_trips_through_its_own_namespace() {
        let db = db();
        let sdk_db = db.with_prefix(sdk_prefix().to_vec());
        let mut dbtx = sdk_db.begin_transaction().await;
        write(
            &mut dbtx,
            &SeedKey,
            &SeedRecord {
                phrase: "abandon abandon about".to_owned(),
            },
        )
        .await
        .expect("the write succeeds");
        commit(dbtx).await.expect("the commit succeeds");

        let mut dbtx = sdk_db.begin_transaction_nc().await;
        let found = read(&mut dbtx, &SeedKey).await.expect("the read succeeds");
        assert_eq!(
            found.map(|record| record.phrase).as_deref(),
            Some("abandon abandon about")
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_untouched_store_is_provably_empty_and_a_written_one_is_not() {
        // "No seed" must never be read as "fresh storage", so `build` needs a
        // proof that covers every byte the SDK could ever have written, not just
        // the seed key.
        let db = db();
        assert!(is_empty(&db).await.expect("the scan succeeds"));

        write_federation(&db, &FederationId::dummy(), &record(FederationId::dummy()))
            .await
            .expect("the write succeeds");
        assert!(!is_empty(&db).await.expect("the scan succeeds"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn federation_records_round_trip_and_list() {
        let db = db();
        let id = FederationId::dummy();
        assert_eq!(read_federation(&db, &id).await.expect("read"), None);

        write_federation(&db, &id, &record(id))
            .await
            .expect("write");
        assert_eq!(
            read_federation(&db, &id).await.expect("read"),
            Some(record(id))
        );

        let listed = list_federations(&db).await.expect("list");
        assert_eq!(listed, vec![(id, record(id))]);

        remove_federation(&db, &id).await.expect("remove");
        assert_eq!(read_federation(&db, &id).await.expect("read"), None);
        assert!(list_federations(&db).await.expect("list").is_empty());
    }

    #[test]
    fn a_federation_record_never_prints_its_invite_code() {
        // Upstream's `InviteCode` prints every part of itself, `api_secret`
        // included, and this record is reachable from the `Debug` of every handle
        // that holds it.
        let rendered = format!("{:?}", record(FederationId::dummy()));
        assert!(rendered.contains("invite: \"<redacted>\""), "{rendered}");
        assert!(!rendered.contains("guardian.example"), "{rendered}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn wiping_a_federation_leaves_every_other_key_alone() {
        // This is the erase primitive, so it has to be exactly as wide as one
        // federation: no wider, or a `forget_federation` would take the seed and
        // the other federations with it.
        let db = db();
        let id = FederationId::dummy();
        let other = fedimint_core::config::FederationId(
            fedimint_core::bitcoin::hashes::sha256::Hash::from_byte_array([9; 32]),
        );

        for federation in [id, other] {
            let fed_db = db.with_prefix(federation_prefix(&federation).to_vec());
            let mut dbtx = fed_db.begin_transaction().await;
            write(
                &mut dbtx,
                &SeedKey,
                &SeedRecord {
                    phrase: "x".to_owned(),
                },
            )
            .await
            .expect("write");
            commit(dbtx).await.expect("commit");
        }
        write_federation(&db, &id, &record(id))
            .await
            .expect("write");

        wipe_federation(&db, &id).await.expect("wipe");

        let wiped = db.with_prefix(federation_prefix(&id).to_vec());
        let mut dbtx = wiped.begin_transaction_nc().await;
        assert_eq!(read(&mut dbtx, &SeedKey).await.expect("read"), None);
        drop(dbtx);

        let kept = db.with_prefix(federation_prefix(&other).to_vec());
        let mut dbtx = kept.begin_transaction_nc().await;
        assert!(read(&mut dbtx, &SeedKey).await.expect("read").is_some());
        drop(dbtx);

        // The registry entry is a separate record and survives the namespace wipe;
        // `forget_federation` removes it in its own step.
        assert_eq!(
            read_federation(&db, &id).await.expect("read"),
            Some(record(id))
        );
    }
}
