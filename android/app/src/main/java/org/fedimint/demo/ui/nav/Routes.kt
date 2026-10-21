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

/** The Activity tab: the history of the federation shown on Home. */
@Serializable data object ActivityTab

// Payments work on one federation, named by id. Each is one screen with a tab
// per rail (Lightning, Ecash, Onchain) the federation runs.

@Serializable data class Send(val federationId: String)

@Serializable data class Receive(val federationId: String)

@Serializable data class OperationDetail(val federationId: String, val operationId: String)

@Serializable data class Recovery(val federationId: String)

@Serializable data class FederationMeta(val federationId: String)

/** The recovery phrase again, from Home (onboarding's [Backup] in review mode). */
@Serializable data object RecoveryPhrase
