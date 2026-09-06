//! The module set this build of the SDK speaks, and what it reads off a configuration.
//!
//! One canonical init registry is built per instance and cloned per federation, because
//! `ModuleInitRegistry::attach` panics on a duplicate kind and `ClientBuilder::with_module` goes
//! straight to it.

use fedimint_client::module_init::{ClientModuleInitRegistry, IClientModuleInit};
use fedimint_connectors::ConnectorRegistry;
use fedimint_core::config::{ClientConfig, ModuleInitRegistry};
use fedimint_core::module::registry::ModuleDecoderRegistry;

use crate::{
    Capabilities, Error, ErrorCode, ErrorDetails, FederationPreview, ModuleGeneration, Network,
    Result,
};

/// Every module kind this build can operate, as one registry to clone per federation.
///
/// A kind absent from this registry is skipped when a client is built (upstream logs a `debug!`
/// and carries on, `fedimint-client/src/client/builder.rs:788-810`), which is why the
/// SDK derives capabilities from the configuration itself rather than from what the client came
/// up with.
pub(crate) fn module_inits() -> ClientModuleInitRegistry {
    let mut inits = ModuleInitRegistry::new();
    inits.attach(fedimint_ln_client::LightningClientInit::default());
    inits.attach(fedimint_lnv2_client::LightningClientInit::default());
    inits.attach(fedimint_meta_client::MetaClientInit);
    inits.attach(fedimint_mint_client::MintClientInit);
    inits.attach(fedimint_mintv2_client::MintClientInit);
    inits.attach(fedimint_wallet_client::WalletClientInit::default());
    inits.attach(fedimint_walletv2_client::WalletClientInit);
    inits
}

/// The one connector registry an instance shares with every federation it opens.
///
/// `ConnectorRegistry` has no `Default` on any target; it is `Clone` and internally reference
/// counted, and both `ClientHandle::restart` and `Client::endpoints` assume every client of one
/// application shares one. Binding is lazy: each transport is built the first time it is asked
/// for, so this touches no network.
///
/// The client defaults are taken with the environment's overrides applied, which is what lets a
/// test federation redirect a guardian's advertised URL without the SDK growing a knob for it.
pub(crate) async fn connectors() -> Result<ConnectorRegistry> {
    ConnectorRegistry::build_from_client_env()
        .map_err(|err| {
            Error::new(
                ErrorCode::Internal,
                format!("unusable transport settings: {err}"),
            )
        })?
        .bind()
        .await
        .map_err(|err| Error::new(ErrorCode::Internal, format!("no usable transport: {err}")))
}

/// The generation a module kind declares, or `None` for a kind that declares none.
///
/// `meta` and any kind this build does not know declare none: fedimint's meta module has no
/// v1/v2 split, so reading "declares nothing" as a conflict would reject every real federation.
pub(crate) fn generation_of(kind: &str) -> Option<u8> {
    match kind {
        "mint" | "wallet" | "ln" => Some(1),
        "mintv2" | "walletv2" | "lnv2" => Some(2),
        _ => None,
    }
}

/// The generation this federation runs, or the refusal that names the conflict.
///
/// The rule is federation-wide and covers every module, not only the facaded ones. A
/// configuration carrying both generations is one this SDK cannot reason about completely, and it
/// will not hold funds in one.
pub(crate) fn check_generation(kinds: &[String]) -> Result<Option<u8>> {
    let mut generations: Vec<(&str, u8)> = kinds
        .iter()
        .filter_map(|kind| generation_of(kind).map(|generation| (kind.as_str(), generation)))
        .collect();
    generations.sort_by_key(|(kind, generation)| (*generation, *kind));

    let Some((_, first)) = generations.first().copied() else {
        return Ok(None);
    };
    if generations
        .iter()
        .all(|(_, generation)| *generation == first)
    {
        return Ok(Some(first));
    }

    let modules: Vec<ModuleGeneration> = generations
        .iter()
        .map(|(kind, generation)| ModuleGeneration::new(*kind, u32::from(*generation)))
        .collect();
    let named = modules
        .iter()
        .map(|module| format!("{}=v{}", module.kind, module.generation))
        .collect::<Vec<_>>()
        .join(", ");
    Err(Error::with_details(
        ErrorCode::UnsupportedFederation,
        format!("modules {named}"),
        ErrorDetails::MixedModuleGenerations { modules },
    ))
}

/// What the SDK can do with a federation running `kinds`.
///
/// Either generation of a family answers for that family: the facade is the same either way, only
/// the module behind it differs.
pub(crate) fn capabilities_of(kinds: &[String]) -> Capabilities {
    let has = |wanted: &str| kinds.iter().any(|kind| kind == wanted);
    Capabilities {
        ecash: has("mint") || has("mintv2"),
        lightning: has("ln") || has("lnv2"),
        onchain: has("wallet") || has("walletv2"),
    }
}

/// Every module kind this configuration declares, sorted, deduplicated.
pub(crate) fn module_kinds(config: &ClientConfig) -> Vec<String> {
    let mut kinds: Vec<String> = config
        .modules
        .values()
        .map(|module| module.kind.as_str().to_owned())
        .collect();
    kinds.sort();
    kinds.dedup();
    kinds
}

/// Decoders for the module kinds this build knows, so a raw configuration can be redecoded.
///
/// A configuration straight off `ClientPreview::config()` still holds each module's config as raw
/// bytes; typed access needs it redecoded first, and `fedimint-client`'s own helper for this is
/// private, so the registry is assembled from each init's decoder here.
pub(crate) fn decoders(
    inits: &ClientModuleInitRegistry,
    config: &ClientConfig,
) -> ModuleDecoderRegistry {
    ModuleDecoderRegistry::new(config.modules.iter().filter_map(|(id, module)| {
        let init = inits.get(&module.kind)?;
        Some((
            *id,
            module.kind.clone(),
            IClientModuleInit::decoder(&**init),
        ))
    }))
}

/// The Bitcoin network this federation operates on.
///
/// `GlobalClientConfig` carries no network of its own, so it is read from the modules that do name
/// one: both wallet generations and both lightning generations. A federation with no wallet module
/// is still a federation this SDK describes — `Federation::onchain()` is an `Option` and
/// `Capabilities.onchain` may be false — so the lightning configurations answer for it rather than
/// the whole federation being refused.
pub(crate) fn network_of(
    inits: &ClientModuleInitRegistry,
    config: &ClientConfig,
) -> Result<Network> {
    let decoded = config
        .clone()
        .redecode_raw(&decoders(inits, config))
        .map_err(|err| {
            Error::new(
                ErrorCode::UnsupportedFederation,
                format!("unreadable module configuration: {err}"),
            )
        })?;

    // In this order: a wallet module exists to speak to the chain, so it is the primary source,
    // and the lightning modules are what is left when a federation has no wallet. Every one of
    // the four is read out of the same redecoded configuration by the same call.
    let mut candidates: Vec<(&str, fedimint_core::bitcoin::Network)> = Vec::new();
    if let Ok((_, wallet)) = decoded
        .get_first_module_by_kind::<fedimint_wallet_client::common::config::WalletClientConfig>(
            "wallet",
        )
    {
        candidates.push(("wallet", wallet.network.0));
    }
    if let Ok((_, wallet)) = decoded
        .get_first_module_by_kind::<fedimint_walletv2_client::common::config::WalletClientConfig>(
            "walletv2",
        )
    {
        candidates.push(("walletv2", wallet.network));
    }
    if let Ok((_, lightning)) = decoded
        .get_first_module_by_kind::<fedimint_ln_client::common::config::LightningClientConfig>("ln")
    {
        candidates.push(("ln", lightning.network.0));
    }
    if let Ok((_, lightning)) = decoded
        .get_first_module_by_kind::<fedimint_lnv2_client::common::config::LightningClientConfig>(
            "lnv2",
        )
    {
        candidates.push(("lnv2", lightning.network));
    }

    network_from_candidates(&candidates)
}

/// The one network a federation's modules agree on, or the refusal that says why there is none.
///
/// Split out of [`network_of`] so the rule is testable without a federation: building a typed
/// module configuration by hand needs threshold-key material this dependency set has no cheap
/// constructor for, so the decoding half is covered by the integration tests in Task 8 instead.
fn network_from_candidates(
    candidates: &[(&str, fedimint_core::bitcoin::Network)],
) -> Result<Network> {
    let Some(&(first_kind, first)) = candidates.first() else {
        return Err(Error::new(
            ErrorCode::UnsupportedFederation,
            "the federation declares no module that names a Bitcoin network",
        ));
    };
    // Two modules of one federation naming different networks is not a case to pick a winner
    // from: one of the two is wrong, and every address this SDK validates is validated against
    // whichever it believed. Refusing is recoverable; a wrong network is not.
    if let Some(&(kind, other)) = candidates.iter().find(|(_, network)| *network != first) {
        return Err(Error::new(
            ErrorCode::UnsupportedFederation,
            format!("modules disagree about the network: {first_kind}={first}, {kind}={other}"),
        ));
    }
    Ok(from_bitcoin_network(first))
}

/// Everything a "join this federation?" screen needs, and the validation that earns it.
///
/// Producing a preview runs the same generation check `join` runs, so a federation this SDK could
/// not operate on is refused here rather than previewed and then refused.
pub(crate) fn preview_of(
    inits: &ClientModuleInitRegistry,
    config: &ClientConfig,
) -> Result<FederationPreview> {
    let modules = module_kinds(config);
    check_generation(&modules)?;
    Ok(FederationPreview {
        id: crate::FederationId::from_upstream(config.calculate_federation_id()),
        name: config.global.federation_name().map(ToOwned::to_owned),
        network: network_of(inits, config)?,
        guardians: u16::try_from(config.global.api_endpoints.len()).unwrap_or(u16::MAX),
        modules,
        meta: config.global.meta.clone(),
    })
}

/// Maps the wallet module's network onto the crate's own enum.
///
/// Total, and deliberately without a catch-all arm. `bitcoin::Network` is **not**
/// `#[non_exhaustive]` — the crate documents adding a network as a breaking change for exactly
/// this reason — so a wildcard here would be an unreachable pattern and fail `-D warnings`, and
/// the exhaustive match is what makes a future variant a compile error at the one place that has
/// to decide what to do about it.
fn from_bitcoin_network(network: fedimint_core::bitcoin::Network) -> Network {
    match network {
        fedimint_core::bitcoin::Network::Bitcoin => Network::Bitcoin,
        fedimint_core::bitcoin::Network::Testnet => Network::Testnet,
        fedimint_core::bitcoin::Network::Testnet4 => Network::Testnet4,
        fedimint_core::bitcoin::Network::Signet => Network::Signet,
        fedimint_core::bitcoin::Network::Regtest => Network::Regtest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(list: &[&str]) -> Vec<String> {
        list.iter().map(|kind| (*kind).to_owned()).collect()
    }

    #[test]
    fn an_all_v1_federation_is_generation_one() {
        let found = check_generation(&kinds(&["ln", "meta", "mint", "wallet"]))
            .expect("an all-v1 federation is accepted");
        assert_eq!(found, Some(1));
    }

    #[test]
    fn an_all_v2_federation_is_generation_two() {
        let found = check_generation(&kinds(&["lnv2", "meta", "mintv2", "walletv2"]))
            .expect("an all-v2 federation is accepted");
        assert_eq!(found, Some(2));
    }

    #[test]
    fn a_federation_with_no_generation_bearing_module_is_accepted() {
        // The rule is about disagreement, not about presence: a federation running
        // only modules that declare no generation has nothing to disagree about.
        let found = check_generation(&kinds(&["meta", "somethingelse"]))
            .expect("a generation-less federation is accepted");
        assert_eq!(found, None);
    }

    #[test]
    fn mixing_lightning_generations_names_both_sides() {
        // This is the shape devimint's own wasm test stands up: `ln` alongside
        // `lnv2`. The error has to name the conflict so an application can show
        // it without parsing the message.
        let err = check_generation(&kinds(&["ln", "lnv2", "meta", "mint", "wallet"]))
            .expect_err("a mixed federation is refused");
        assert_eq!(err.code, crate::ErrorCode::UnsupportedFederation);
        match err.detail() {
            Some(crate::ErrorDetails::MixedModuleGenerations { modules }) => {
                let named: Vec<(&str, u32)> = modules
                    .iter()
                    .map(|module| (module.kind.as_str(), module.generation))
                    .collect();
                assert_eq!(
                    named,
                    vec![("ln", 1), ("mint", 1), ("wallet", 1), ("lnv2", 2)]
                );
            }
            other => panic!("expected the conflicting modules, got {other:?}"),
        }
    }

    #[test]
    fn capabilities_follow_either_generation_of_each_family() {
        assert_eq!(
            capabilities_of(&kinds(&["ln", "meta", "mint", "wallet"])),
            Capabilities {
                ecash: true,
                lightning: true,
                onchain: true
            }
        );
        assert_eq!(
            capabilities_of(&kinds(&["lnv2", "mintv2", "walletv2"])),
            Capabilities {
                ecash: true,
                lightning: true,
                onchain: true
            }
        );
        assert_eq!(
            capabilities_of(&kinds(&["meta"])),
            Capabilities {
                ecash: false,
                lightning: false,
                onchain: false
            }
        );
        assert_eq!(
            capabilities_of(&kinds(&["mint"])),
            Capabilities {
                ecash: true,
                lightning: false,
                onchain: false
            }
        );
    }

    #[test]
    fn the_registry_carries_every_kind_this_build_speaks() {
        let inits = module_inits();
        let present: Vec<String> = inits.kinds().iter().map(ToString::to_string).collect();
        assert_eq!(
            present,
            vec!["ln", "lnv2", "meta", "mint", "mintv2", "wallet", "walletv2"]
        );
    }

    #[test]
    fn a_wallet_less_federation_takes_its_network_from_lightning() {
        // A mint + ln federation is one the contract still has to describe: `onchain()` is an
        // `Option` and `Capabilities.onchain` may be false, so the lightning module's own
        // configuration is where the network comes from rather than a reason to refuse.
        let network = network_from_candidates(&[("ln", fedimint_core::bitcoin::Network::Signet)])
            .expect("a federation with no wallet module is accepted");
        assert_eq!(network, Network::Signet);
    }

    #[test]
    fn a_wallet_module_is_the_source_when_there_is_one() {
        // `network_of` collects the wallet kinds first, so a federation running a wallet
        // alongside a lightning module is read from the wallet.
        let network = network_from_candidates(&[
            ("wallet", fedimint_core::bitcoin::Network::Regtest),
            ("ln", fedimint_core::bitcoin::Network::Regtest),
        ])
        .expect("a federation with a wallet module is accepted");
        assert_eq!(network, Network::Regtest);
    }

    #[test]
    fn a_mint_only_federation_has_no_network_to_read() {
        // Nothing in a mint configuration names a chain, so this is the one shape left that is
        // refused rather than described.
        let err =
            network_from_candidates(&[]).expect_err("a federation naming no network is refused");
        assert_eq!(err.code, crate::ErrorCode::UnsupportedFederation);
    }

    #[test]
    fn modules_that_disagree_about_the_network_are_refused() {
        let err = network_from_candidates(&[
            ("wallet", fedimint_core::bitcoin::Network::Bitcoin),
            ("ln", fedimint_core::bitcoin::Network::Testnet),
        ])
        .expect_err("a federation whose modules disagree is refused");
        assert_eq!(err.code, crate::ErrorCode::UnsupportedFederation);
    }
}
