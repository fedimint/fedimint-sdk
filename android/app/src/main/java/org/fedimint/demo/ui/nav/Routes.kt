package org.fedimint.demo.ui.nav

import kotlinx.serialization.Serializable

// Type-safe destinations (Navigation 2.8+). A screen that takes arguments
// becomes a `data class`, and its arguments are checked at compile time
// instead of being spliced into a route string.

@Serializable data object Welcome

@Serializable data object Restore

@Serializable data object Backup

@Serializable data object VerifyBackup

@Serializable data object Home

@Serializable data object JoinFederation

@Serializable data object Federations

@Serializable data class FederationDetail(val id: String)

// Payments: every screen works on one federation, named by id.

@Serializable data class LightningReceive(val federationId: String)

@Serializable data class LightningSend(val federationId: String)

@Serializable data class EcashReceive(val federationId: String)

@Serializable data class EcashSend(val federationId: String)

@Serializable data class OnchainReceive(val federationId: String)

@Serializable data class OnchainSend(val federationId: String)
