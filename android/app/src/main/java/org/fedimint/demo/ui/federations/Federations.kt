package org.fedimint.demo.ui.federations

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import org.fedimint.demo.ui.common.StatusBadge
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.label
import org.fedimint.demo.wallet.WalletSession
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.FederationInfo

class FederationsViewModel(session: WalletSession) : ViewModel() {
    /** Every federation the wallet holds, whatever its status. Null while loading. */
    val federations = session.federations
        .map { list -> list?.sortedBy { it.name ?: it.id } }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)
}

/** Every federation this wallet holds, including closed, quarantined and ones being removed. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FederationsScreen(onBack: () -> Unit, onOpen: (FederationId) -> Unit, onJoin: () -> Unit) {
    val vm = appViewModel { FederationsViewModel(it.session) }
    val federations by vm.federations.collectAsStateWithLifecycle()

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Federations") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
            )
        },
        floatingActionButton = {
            ExtendedFloatingActionButton(
                onClick = onJoin,
                icon = { Icon(Icons.Filled.Add, contentDescription = null) },
                text = { Text("Join") },
            )
        },
    ) { padding ->
        val list = federations
        when {
            list == null -> Column(Modifier.fillMaxSize().padding(padding), horizontalAlignment = Alignment.CenterHorizontally) {
                CircularProgressIndicator(Modifier.padding(32.dp))
            }
            list.isEmpty() -> Text(
                "You haven't joined a federation yet.",
                modifier = Modifier.padding(padding).padding(24.dp),
                style = MaterialTheme.typography.bodyLarge,
            )
            else -> LazyColumn(Modifier.fillMaxSize().padding(padding)) {
                items(list, key = { it.id }) { federation ->
                    FederationRow(federation, onClick = { onOpen(federation.id) })
                    HorizontalDivider()
                }
            }
        }
    }
}

@Composable
private fun FederationRow(federation: FederationInfo, onClick: () -> Unit) {
    ListItem(
        headlineContent = { Text(federation.name ?: "Unnamed federation") },
        supportingContent = { Text(federation.network.label()) },
        trailingContent = { StatusBadge(federation.status) },
        modifier = Modifier.fillMaxWidth().clickable(onClick = onClick),
    )
}
