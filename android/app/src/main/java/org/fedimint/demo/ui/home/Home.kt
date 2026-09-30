package org.fedimint.demo.ui.home

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowDropDown
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapLatest
import kotlinx.coroutines.flow.stateIn
import org.fedimint.demo.ui.activity.ActivityRow
import org.fedimint.demo.ui.common.StatusBadge
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.canTransact
import org.fedimint.demo.ui.common.explanation
import org.fedimint.demo.ui.common.formatSats
import org.fedimint.demo.ui.common.isOpen
import org.fedimint.demo.ui.common.label
import org.fedimint.demo.ui.common.msatRemainder
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.demo.ui.payments.PaymentDirection
import org.fedimint.demo.ui.payments.Rail
import org.fedimint.demo.wallet.History
import org.fedimint.demo.wallet.WalletSession
import org.fedimint.sdk.ActivityItem
import org.fedimint.sdk.Amount
import org.fedimint.sdk.Capabilities
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.FederationInfo
import org.fedimint.sdk.FederationStatus
import org.fedimint.sdk.Network
import org.fedimint.sdk.OperationId

@OptIn(ExperimentalCoroutinesApi::class)
class HomeViewModel(private val session: WalletSession, private val history: History) : ViewModel() {
    sealed interface Balance {
        data object Loading : Balance
        data class Value(val msats: Amount) : Balance
        data class Unavailable(val message: String?) : Balance
    }

    data class UiState(
        /** Null until the wallet's federations have loaded. */
        val federations: List<FederationInfo>? = null,
        val active: FederationInfo? = null,
        val balance: Balance = Balance.Loading,
        val capabilities: Capabilities? = null,
        /** The newest few history rows for the active federation. */
        val recent: List<ActivityItem> = emptyList(),
    )

    private val federations = combine(session.federations, session.selectedFederationId) { list, selected ->
        list?.sortedBy { it.name ?: it.id } to list?.let { WalletSession.pickActive(it, selected) }
    }

    /**
     * The federation whose live data to follow, keyed on its id and whether it
     * is open, so the subscriptions restart when the user switches federation
     * or the federation closes or reopens, and not on any other status change.
     */
    private val openActiveId = federations
        .map { (_, active) -> active?.takeIf { it.status.isOpen }?.id }
        .distinctUntilChanged()

    private val balance = openActiveId.flatMapLatest { id ->
        if (id == null) {
            flowOf(Balance.Unavailable(null))
        } else {
            session.balance(id)
                .map<Amount, Balance> { Balance.Value(it) }
                .catch { emit(Balance.Unavailable(userMessage(it))) }
        }
    }

    private val capabilities = openActiveId.flatMapLatest { id ->
        flow { emit(id?.let { session.capabilities(it) }) }.catch { emit(null) }
    }

    /**
     * Reloaded whenever the balance moves: money arriving or leaving is when a
     * row appears or settles. Its own balance subscription, since each one is
     * an independent cursor.
     */
    private val recent = openActiveId.flatMapLatest { id ->
        if (id == null) {
            flowOf(emptyList())
        } else {
            session.balance(id)
                .mapLatest { history.page(id, limit = RECENT_ROWS).items }
                .catch { emit(emptyList()) }
        }
    }

    /** Live while the screen is visible, and for 5 s after, so rotation doesn't restart the subscriptions. */
    val state = combine(federations, balance, capabilities, recent) { (list, active), balance, caps, recent ->
        UiState(federations = list, active = active, balance = balance, capabilities = caps, recent = recent)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), UiState())

    fun select(federation: FederationInfo) = session.select(federation.id)

    private companion object {
        const val RECENT_ROWS = 5
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(
    onJoinFederation: () -> Unit,
    onOpenFederations: () -> Unit,
    onOpenDeveloperTools: () -> Unit,
    onPay: (PaymentDirection, Rail, FederationId) -> Unit,
    onOpenActivity: (FederationId) -> Unit,
    onOpenOperation: (FederationId, OperationId) -> Unit,
    onOpenRecovery: (FederationId) -> Unit,
    onOpenRecoveryPhrase: () -> Unit,
) {
    val vm = appViewModel { HomeViewModel(it.session, it.history) }
    val state by vm.state.collectAsStateWithLifecycle()
    var sheet by remember { mutableStateOf<PaymentDirection?>(null) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { FederationSwitcher(state.federations.orEmpty(), state.active, vm::select) },
                actions = { OverflowMenu(onOpenFederations, onJoinFederation, onOpenRecoveryPhrase, onOpenDeveloperTools) },
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            val active = state.active
            when {
                state.federations == null -> CircularProgressIndicator(Modifier.align(Alignment.CenterHorizontally))
                active == null -> NoFederation(onJoinFederation)
                else -> {
                    BalanceCard(active, state.balance)
                    active.status.explanation()?.let { StatusNote(active.status, it, onViewRecovery = { onOpenRecovery(active.id) }) }
                    val canTransact = active.status.canTransact && state.capabilities.hasAny()
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        Button(
                            onClick = { sheet = PaymentDirection.Receive },
                            enabled = canTransact,
                            modifier = Modifier.weight(1f),
                        ) { Text("Receive") }
                        Button(
                            onClick = { sheet = PaymentDirection.Send },
                            enabled = canTransact,
                            modifier = Modifier.weight(1f),
                        ) { Text("Send") }
                    }
                    if (active.status.isOpen) {
                        RecentActivity(
                            items = state.recent,
                            onOpen = { onOpenOperation(active.id, it) },
                            onSeeAll = { onOpenActivity(active.id) },
                        )
                    }
                }
            }
        }
    }

    val caps = state.capabilities
    val direction = sheet
    val active = state.active
    if (direction != null && caps != null && active != null) {
        PaymentMethodSheet(
            direction,
            caps,
            onPick = { rail ->
                sheet = null
                onPay(direction, rail, active.id)
            },
            onDismiss = { sheet = null },
        )
    }
}

@Composable
private fun FederationSwitcher(
    federations: List<FederationInfo>,
    active: FederationInfo?,
    onSelect: (FederationInfo) -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    val title = active?.let { it.name ?: "Unnamed federation" } ?: "Wallet"
    if (federations.size < 2) {
        Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis)
        return
    }
    TextButton(onClick = { open = true }) {
        Text(title, style = MaterialTheme.typography.titleLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Icon(Icons.Filled.ArrowDropDown, contentDescription = "Switch federation")
    }
    DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
        federations.forEach { federation ->
            DropdownMenuItem(
                text = { Text(federation.name ?: "Unnamed federation") },
                trailingIcon = { StatusBadge(federation.status) },
                onClick = {
                    open = false
                    onSelect(federation)
                },
            )
        }
    }
}

@Composable
private fun OverflowMenu(
    onOpenFederations: () -> Unit,
    onJoinFederation: () -> Unit,
    onOpenRecoveryPhrase: () -> Unit,
    onOpenDeveloperTools: () -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    IconButton(onClick = { open = true }) {
        Icon(Icons.Filled.MoreVert, contentDescription = "More")
    }
    DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
        DropdownMenuItem(text = { Text("Federations") }, onClick = {
            open = false
            onOpenFederations()
        })
        DropdownMenuItem(text = { Text("Join a federation") }, onClick = {
            open = false
            onJoinFederation()
        })
        DropdownMenuItem(text = { Text("Recovery phrase") }, onClick = {
            open = false
            onOpenRecoveryPhrase()
        })
        DropdownMenuItem(text = { Text("Developer tools") }, onClick = {
            open = false
            onOpenDeveloperTools()
        })
    }
}

@Composable
private fun NoFederation(onJoinFederation: () -> Unit) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(20.dp)) {
            Text("Join a federation", style = MaterialTheme.typography.titleLarge)
            Spacer(Modifier.height(8.dp))
            Text(
                "A federation is a group of guardians who hold bitcoin together and issue ecash " +
                    "to you. You need an invite code from one to get started.",
                style = MaterialTheme.typography.bodyMedium,
            )
            Spacer(Modifier.height(16.dp))
            Button(onClick = onJoinFederation) { Text("Join with an invite code") }
        }
    }
}

@Composable
private fun BalanceCard(federation: FederationInfo, balance: HomeViewModel.Balance) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(20.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Balance", style = MaterialTheme.typography.labelLarge, modifier = Modifier.weight(1f))
                if (federation.network != Network.BITCOIN) {
                    Text(
                        federation.network.label(),
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.tertiary,
                    )
                    Spacer(Modifier.width(8.dp))
                }
                StatusBadge(federation.status)
            }
            Spacer(Modifier.height(8.dp))
            when (balance) {
                HomeViewModel.Balance.Loading -> CircularProgressIndicator(Modifier.height(40.dp))
                is HomeViewModel.Balance.Value -> {
                    Row(verticalAlignment = Alignment.Bottom) {
                        Text(formatSats(balance.msats), style = MaterialTheme.typography.displaySmall)
                        Spacer(Modifier.width(8.dp))
                        Text("sats", style = MaterialTheme.typography.titleMedium, modifier = Modifier.padding(bottom = 6.dp))
                    }
                    msatRemainder(balance.msats)?.let {
                        Text("+ $it msats", style = MaterialTheme.typography.bodySmall)
                    }
                }
                is HomeViewModel.Balance.Unavailable -> {
                    Text("—", style = MaterialTheme.typography.displaySmall)
                    balance.message?.let {
                        Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
                    }
                }
            }
            if (federation.network != Network.BITCOIN) {
                Spacer(Modifier.height(8.dp))
                Text(
                    "Test network: these coins have no real value.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

@Composable
private fun StatusNote(status: FederationStatus, text: String, onViewRecovery: () -> Unit) {
    val error = status is FederationStatus.Quarantined
    Card(
        colors = CardDefaults.cardColors(
            containerColor = if (error) MaterialTheme.colorScheme.errorContainer else MaterialTheme.colorScheme.secondaryContainer,
        ),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(Modifier.padding(16.dp)) {
            Text(text, style = MaterialTheme.typography.bodyMedium)
            if (status is FederationStatus.Quarantined) {
                Spacer(Modifier.height(4.dp))
                Text("Code: ${status.diagnostic.code}", style = MaterialTheme.typography.labelSmall)
            }
            if (status is FederationStatus.Recovering) {
                TextButton(onClick = onViewRecovery) { Text("View progress") }
            }
        }
    }
}

@Composable
private fun RecentActivity(items: List<ActivityItem>, onOpen: (OperationId) -> Unit, onSeeAll: () -> Unit) {
    Column {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("Recent activity", style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
            TextButton(onClick = onSeeAll) { Text("See all") }
        }
        if (items.isEmpty()) {
            Text(
                "Nothing yet. Your payments will show up here.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        } else {
            Card(Modifier.fillMaxWidth()) {
                items.forEachIndexed { i, item ->
                    if (i > 0) HorizontalDivider()
                    ActivityRow(item, onClick = { onOpen(item.operationId) })
                }
            }
        }
    }
}

/** The methods this federation offers in one direction. Only supported ones are listed. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun PaymentMethodSheet(
    direction: PaymentDirection,
    caps: Capabilities,
    onPick: (Rail) -> Unit,
    onDismiss: () -> Unit,
) {
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(Modifier.navigationBarsPadding().padding(bottom = 16.dp)) {
            Text(
                if (direction == PaymentDirection.Send) "Send with" else "Receive with",
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.padding(horizontal = 24.dp, vertical = 8.dp),
            )
            methods(direction, caps).forEachIndexed { i, (rail, detail) ->
                if (i > 0) HorizontalDivider()
                ListItem(
                    headlineContent = { Text(rail.title) },
                    supportingContent = { Text(detail) },
                    modifier = Modifier.clickable { onPick(rail) },
                )
            }
        }
    }
}

private fun methods(direction: PaymentDirection, caps: Capabilities): List<Pair<Rail, String>> = buildList {
    val send = direction == PaymentDirection.Send
    if (caps.lightning) add(Rail.Lightning to if (send) "Pay an invoice" else "Create an invoice")
    if (caps.ecash) add(Rail.Ecash to if (send) "Hand over notes directly" else "Redeem notes you were given")
    if (caps.onchain) add(Rail.Onchain to if (send) "Send to a bitcoin address" else "Deposit from a bitcoin address")
}

private fun Capabilities?.hasAny(): Boolean = this != null && (ecash || lightning || onchain)
