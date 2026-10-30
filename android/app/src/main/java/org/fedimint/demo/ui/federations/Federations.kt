package org.fedimint.demo.ui.federations

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import org.fedimint.demo.R
import org.fedimint.demo.ui.common.ButtonStyle
import org.fedimint.demo.ui.common.CardDivider
import org.fedimint.demo.ui.common.FederationAvatar
import org.fedimint.demo.ui.common.Loading
import org.fedimint.demo.ui.common.MutedText
import org.fedimint.demo.ui.common.Overline
import org.fedimint.demo.ui.common.StatusBadge
import org.fedimint.demo.ui.common.TabTitle
import org.fedimint.demo.ui.common.WalletButton
import org.fedimint.demo.ui.common.WalletCard
import org.fedimint.demo.ui.common.WalletIcon
import org.fedimint.demo.ui.common.WalletScreen
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.isOpen
import org.fedimint.demo.ui.common.label
import org.fedimint.demo.wallet.WalletSession
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.FederationInfo
import org.fedimint.sdk.FederationStatus

class FederationsViewModel(session: WalletSession) : ViewModel() {
    /** Every federation the wallet holds, whatever its status. Null while loading. */
    val federations = session.federations
        .map { list -> list?.sortedBy { it.name ?: it.id } }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)
}

/**
 * The Federations tab: every federation this wallet holds, open ones first,
 * then closed, quarantined and ones being removed.
 */
@Composable
fun FederationsScreen(onOpen: (FederationId) -> Unit, onJoin: () -> Unit) {
    val vm = appViewModel { FederationsViewModel(it.session) }
    val federations by vm.federations.collectAsStateWithLifecycle()

    WalletScreen(
        spacing = 12.dp,
        bottom = { WalletButton("Join with invite code", onJoin, style = ButtonStyle.Secondary, icon = R.drawable.ic_plus) },
    ) {
        TabTitle("Federations") {
            IconButton(onClick = onJoin) { WalletIcon(R.drawable.ic_plus, contentDescription = "Join a federation") }
        }
        val list = federations
        when {
            list == null -> Loading()
            list.isEmpty() -> MutedText("You haven't joined a federation yet. Ask a guardian for an invite code.")
            else -> {
                val (active, inactive) = list.partition { it.status.isOpen }
                Group("Active", active, onOpen)
                if (active.isNotEmpty() && inactive.isNotEmpty()) Spacer(Modifier.height(4.dp))
                Group("Inactive", inactive, onOpen)
            }
        }
    }
}

@Composable
private fun Group(title: String, federations: List<FederationInfo>, onOpen: (FederationId) -> Unit) {
    if (federations.isEmpty()) return
    Overline("$title · ${federations.size}")
    WalletCard {
        federations.forEachIndexed { i, federation ->
            if (i > 0) CardDivider()
            FederationRow(federation, onClick = { onOpen(federation.id) })
        }
    }
}

@Composable
private fun FederationRow(federation: FederationInfo, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        FederationAvatar(federation.name)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(
                federation.name ?: "Unnamed federation",
                style = MaterialTheme.typography.titleMedium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            MutedText(federation.subtitle())
            StatusBadge(federation.status)
        }
        WalletIcon(R.drawable.ic_chevron_right, contentDescription = null, size = 20.dp, tint = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** One line under the name: the network, and what to do about an inactive status. */
private fun FederationInfo.subtitle(): String = when (val s = status) {
    FederationStatus.Closed -> "${network.label()} · Closed, tap to reopen"
    is FederationStatus.Quarantined -> "${network.label()} · ${s.diagnostic.code}"
    else -> network.label()
}
