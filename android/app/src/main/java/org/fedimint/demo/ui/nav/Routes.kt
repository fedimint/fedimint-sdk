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
