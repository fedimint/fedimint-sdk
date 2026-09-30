package org.fedimint.demo.ui.common

import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import org.fedimint.sdk.FederationStatus

/** Whether the SDK holds a live client for a federation in this status, so its handle and balance work. */
val FederationStatus.isOpen: Boolean
    get() = this is FederationStatus.Running || this is FederationStatus.Recovering

/** Whether sends and receives can run. A recovering federation is open but refuses them until it finishes. */
val FederationStatus.canTransact: Boolean
    get() = this is FederationStatus.Running

fun FederationStatus.label(): String = when (this) {
    FederationStatus.Running -> "Connected"
    FederationStatus.Recovering -> "Recovering"
    is FederationStatus.Quarantined -> "Needs attention"
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

@Composable
fun StatusBadge(status: FederationStatus, modifier: Modifier = Modifier) {
    val scheme = MaterialTheme.colorScheme
    val (container, content) = when (status) {
        FederationStatus.Running -> scheme.primaryContainer to scheme.onPrimaryContainer
        FederationStatus.Recovering -> scheme.tertiaryContainer to scheme.onTertiaryContainer
        is FederationStatus.Quarantined -> scheme.errorContainer to scheme.onErrorContainer
        else -> scheme.surfaceVariant to scheme.onSurfaceVariant
    }
    Surface(color = container, contentColor = content, shape = MaterialTheme.shapes.small, modifier = modifier) {
        Text(
            status.label(),
            style = MaterialTheme.typography.labelMedium,
            modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
            color = Color.Unspecified,
        )
    }
}
