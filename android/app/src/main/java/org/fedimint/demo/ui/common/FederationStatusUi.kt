package org.fedimint.demo.ui.common

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import org.fedimint.sdk.FederationStatus

/** Whether the SDK holds a live client for a federation in this status, so its handle and balance work. */
val FederationStatus.isOpen: Boolean
    get() = this is FederationStatus.Running || this is FederationStatus.Recovering

/** Whether sends and receives can run. A recovering federation is open but refuses them until it finishes. */
val FederationStatus.canTransact: Boolean
    get() = this is FederationStatus.Running

fun FederationStatus.label(): String = when (this) {
    // Running is the local client's lifecycle (open, handle live), not whether
    // the guardians are reachable right now; live connectivity isn't shown yet.
    FederationStatus.Running -> "Open"
    FederationStatus.Recovering -> "Recovering"
    is FederationStatus.Quarantined -> "Quarantined"
    FederationStatus.Closed -> "Closed"
    FederationStatus.Forgetting -> "Removing…"
    FederationStatus.Forgotten -> "Removed"
}

/** One line on what the status means for the user, or null when there is nothing to say. */
fun FederationStatus.explanation(): String? = when (this) {
    FederationStatus.Running -> null
    FederationStatus.Recovering ->
        "Scanning this federation's history for your funds. It can take a long time, and " +
            "your balance is incomplete until it finishes. Sending and receiving unlock then."
    is FederationStatus.Quarantined ->
        "This federation couldn't be opened safely, so it has been set aside. Your funds are " +
            "not spent. ${diagnostic.message}"
    FederationStatus.Closed -> "Closed on this device. Reopen it to use it again."
    FederationStatus.Forgetting -> "Being removed from this device."
    FederationStatus.Forgotten -> null
}

/** The pill colour for a status: green open, amber recovering, red quarantined, grey otherwise. */
fun FederationStatus.tone(): Tone = when (this) {
    FederationStatus.Running -> Tone.Success
    FederationStatus.Recovering -> Tone.Warning
    is FederationStatus.Quarantined -> Tone.Danger
    else -> Tone.Neutral
}

@Composable
fun StatusBadge(status: FederationStatus, modifier: Modifier = Modifier) = StatusPill(status.label(), status.tone(), modifier)
