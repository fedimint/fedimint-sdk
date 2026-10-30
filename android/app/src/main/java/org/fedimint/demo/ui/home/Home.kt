package org.fedimint.demo.ui.home

import android.widget.Toast
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.transformLatest
import org.fedimint.demo.R
import org.fedimint.demo.ui.activity.ActivityRow
import org.fedimint.demo.ui.common.AmountHero
import org.fedimint.demo.ui.common.ButtonStyle
import org.fedimint.demo.ui.common.CardDivider
import org.fedimint.demo.ui.common.ErrorMessage
import org.fedimint.demo.ui.common.Loading
import org.fedimint.demo.ui.common.MutedText
import org.fedimint.demo.ui.common.Overline
import org.fedimint.demo.ui.common.QrScanner
import org.fedimint.demo.ui.common.ScanHandoff
import org.fedimint.demo.ui.common.Scanned
import org.fedimint.demo.ui.common.SectionHeader
import org.fedimint.demo.ui.common.StatusBadge
import org.fedimint.demo.ui.common.WalletButton
import org.fedimint.demo.ui.common.WalletCard
import org.fedimint.demo.ui.common.WalletIcon
import org.fedimint.demo.ui.common.WalletScreen
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.canTransact
import org.fedimint.demo.ui.common.explanation
import org.fedimint.demo.ui.common.formatSats
import org.fedimint.demo.ui.common.isOpen
import org.fedimint.demo.ui.common.label
import org.fedimint.demo.ui.common.msatRemainder
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.demo.ui.theme.WalletText
import org.fedimint.demo.ui.theme.walletColors
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

/**
 * Home's data for the federation on screen: balance, capabilities and recent
 * activity, each loaded on its own.
 *
 * Switching federation changes the one on screen at once, while the new
 * federation's reads are still running. So every result is tagged with the
 * federation it was read for, and only results for the federation on screen
 * are shown; anything else reads as loading or empty. Each stream also starts
 * over from loading when the federation changes. Home can never show one
 * federation's funds or payments under another's name.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class HomeViewModel(private val source: Source) : ViewModel() {
    /** What Home reads, behind an interface so the switching rules are testable. */
    interface Source {
        val federations: Flow<List<FederationInfo>?>
        val selected: Flow<FederationId?>

        fun balance(id: FederationId): Flow<Amount>

        suspend fun capabilities(id: FederationId): Capabilities?

        suspend fun recent(id: FederationId): List<ActivityItem>

        fun select(id: FederationId)
    }

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

    /** A value and the federation it was read for (null: none open). */
    private data class For<T>(val id: FederationId?, val value: T)

    private val federations = combine(source.federations, source.selected) { list, selected ->
        list?.sortedBy { it.name ?: it.id } to list?.let { WalletSession.pickActive(it, selected) }
    }

    /**
     * The federation whose live data to follow, keyed on its id and whether it
     * is open, so the subscriptions restart when the user switches federation
     * or the federation closes or reopens, and not on any other status change.
     */
    private val openActiveId = federations
        .map { (_, active) -> active?.openId() }
        .distinctUntilChanged()

    private val balance = openActiveId.flatMapLatest { id ->
        if (id == null) {
            flowOf(For(null, Balance.Unavailable(null)))
        } else {
            source.balance(id)
                .map<Amount, For<Balance>> { For(id, Balance.Value(it)) }
                .onStart { emit(For(id, Balance.Loading)) }
                .catch { emit(For(id, Balance.Unavailable(userMessage(it)))) }
        }
    }

    private val capabilities = openActiveId.flatMapLatest { id ->
        flow { emit(For(id, id?.let { source.capabilities(it) })) }
            .onStart { emit(For(id, null)) }
            .catch { emit(For(id, null)) }
    }

    private val refreshes = MutableSharedFlow<Unit>(extraBufferCapacity = 1)

    /**
     * Home came back into view. Operations change without the balance moving
     * (one created on another screen, say), so it reloads then too.
     */
    fun refresh() {
        refreshes.tryEmit(Unit)
    }

    /**
     * Reloaded when the balance moves (its own subscription: each is an
     * independent cursor) and when Home comes back into view. While a row is
     * still pending it keeps reloading on a timer, because settling doesn't
     * always move the balance: an ecash send redeemed by its receiver was
     * debited long before. The timer backs off (10 s, doubling to 5 min), since
     * a row can stay pending for days, like unredeemed ecash waiting for its
     * automatic reclaim; any trigger starts it over. All of it runs only while
     * Home is collected.
     */
    private val recent = openActiveId.flatMapLatest { id ->
        if (id == null) {
            flowOf(For(null, emptyList()))
        } else {
            merge(source.balance(id).map { }.catch { }, refreshes)
                .transformLatest {
                    var wait = PENDING_REFRESH_MILLIS
                    while (true) {
                        val items = attempt { source.recent(id) }.getOrNull()
                        if (items != null) emit(For(id, items))
                        if (items != null && items.all { item -> item.isFinal }) break
                        delay(wait)
                        wait = (wait * 2).coerceAtMost(PENDING_REFRESH_MAX_MILLIS)
                    }
                }
                .onStart { emit(For(id, emptyList())) }
        }
    }

    /** Live while the screen is visible, and for 5 s after, so rotation doesn't restart the subscriptions. */
    val state = combine(federations, balance, capabilities, recent) { (list, active), balance, caps, recent ->
        val shown = active?.openId()
        UiState(
            federations = list,
            active = active,
            // A result read for another federation is never shown under this one.
            balance = if (balance.id == shown) balance.value else Balance.Loading,
            capabilities = caps.value.takeIf { caps.id == shown },
            recent = if (recent.id == shown) recent.value else emptyList(),
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), UiState())

    fun select(federation: FederationInfo) = source.select(federation.id)

    private fun FederationInfo.openId(): FederationId? = id.takeIf { status.isOpen }

    companion object {
        private const val RECENT_ROWS = 5

        /** How soon recent activity first reloads while a row is pending and Home is visible. */
        const val PENDING_REFRESH_MILLIS = 10_000L

        /** The longest wait between those reloads, once the backoff has grown. */
        const val PENDING_REFRESH_MAX_MILLIS = 300_000L

        fun from(session: WalletSession, history: History) = HomeViewModel(
            object : Source {
                override val federations = session.federations
                override val selected = session.selectedFederationId

                override fun balance(id: FederationId) = session.balance(id)

                override suspend fun capabilities(id: FederationId) = session.capabilities(id)

                override suspend fun recent(id: FederationId) = history.page(id, limit = RECENT_ROWS).items

                override fun select(id: FederationId) = session.select(id)
            },
        )
    }
}

@Composable
fun HomeScreen(
    onJoinFederation: () -> Unit,
    onSend: (FederationId) -> Unit,
    onReceive: (FederationId) -> Unit,
    onOpenActivity: () -> Unit,
    onOpenOperation: (FederationId, OperationId) -> Unit,
    onOpenRecovery: (FederationId) -> Unit,
    onOpenRecoveryPhrase: () -> Unit,
) {
    val vm = appViewModel { HomeViewModel.from(it.session, it.history) }
    LifecycleEventEffect(Lifecycle.Event.ON_RESUME) { vm.refresh() }
    val state by vm.state.collectAsStateWithLifecycle()
    val context = LocalContext.current
    var scanning by remember { mutableStateOf(false) }
    val active = state.active
    val canTransact = active != null && active.status.canTransact && state.capabilities.hasAny()

    WalletScreen(spacing = 20.dp) {
        TopBar(
            federations = state.federations.orEmpty(),
            active = active,
            onSelect = vm::select,
            onScan = { scanning = true },
            onRecoveryPhrase = onOpenRecoveryPhrase,
        )
        when {
            state.federations == null -> Loading()
            active == null -> NoFederation(onJoinFederation)
            else -> {
                Balance(active, state.balance, state.capabilities)
                active.status.explanation()?.let { StatusNote(active.status, it, onViewRecovery = { onOpenRecovery(active.id) }) }
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    WalletButton(
                        "Receive",
                        { onReceive(active.id) },
                        enabled = canTransact,
                        icon = R.drawable.ic_arrow_down_left,
                        modifier = Modifier.weight(1f),
                    )
                    WalletButton(
                        "Send",
                        { onSend(active.id) },
                        style = ButtonStyle.Secondary,
                        enabled = canTransact,
                        icon = R.drawable.ic_arrow_up_right,
                        modifier = Modifier.weight(1f),
                    )
                }
                if (active.status.isOpen) {
                    RecentActivity(
                        items = state.recent,
                        onOpen = { onOpenOperation(active.id, it) },
                        onSeeAll = onOpenActivity,
                    )
                }
            }
        }
    }

    if (scanning) {
        QrScanner(
            onResult = { text ->
                scanning = false
                val scanned = Scanned.classify(text)
                val id = active?.id
                when {
                    scanned is Scanned.Invite -> {
                        ScanHandoff.put(scanned)
                        onJoinFederation()
                    }
                    id == null || !canTransact ->
                        Toast.makeText(context, "Join a federation that can send and receive first.", Toast.LENGTH_LONG).show()
                    scanned is Scanned.Notes -> {
                        ScanHandoff.put(scanned)
                        onReceive(id)
                    }
                    else -> {
                        ScanHandoff.put(scanned)
                        onSend(id)
                    }
                }
            },
            onDismiss = { scanning = false },
        )
    }
}

/** The federation on screen (a menu when there are several), and scan. */
@Composable
private fun TopBar(
    federations: List<FederationInfo>,
    active: FederationInfo?,
    onSelect: (FederationInfo) -> Unit,
    onScan: () -> Unit,
    onRecoveryPhrase: () -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    Row(Modifier.fillMaxWidth().padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
        Row(
            Modifier
                .weight(1f)
                .clip(MaterialTheme.shapes.small)
                .clickable(enabled = federations.size > 1) { open = true }
                .padding(vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Image(painterResource(R.drawable.brand_mark), contentDescription = null, modifier = Modifier.size(26.dp))
            Text(
                active?.let { it.name ?: "Unnamed federation" } ?: "Fedimint",
                style = MaterialTheme.typography.titleMedium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false),
            )
            if (federations.size > 1) WalletIcon(R.drawable.ic_chevron_down, contentDescription = "Switch federation", size = 16.dp)
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
        IconButton(onClick = onRecoveryPhrase) { WalletIcon(R.drawable.ic_shield, contentDescription = "Recovery phrase") }
        IconButton(onClick = onScan) { WalletIcon(R.drawable.ic_scan, contentDescription = "Scan a QR code") }
    }
}

@Composable
private fun NoFederation(onJoinFederation: () -> Unit) {
    WalletCard(padding = PaddingValues(20.dp), spacing = 12.dp) {
        Text("Join a federation", style = MaterialTheme.typography.titleLarge)
        MutedText(
            "A federation is a group of guardians who hold bitcoin together and issue ecash " +
                "to you. You need an invite code from one to get started.",
        )
        WalletButton("Join with invite code", onJoinFederation, icon = R.drawable.ic_plus)
    }
}

/**
 * Balance, then one line on the connection: the federation's network and the
 * modules it runs. The design also shows a fiat value and guardians online;
 * the SDK reports neither, so they are left out rather than invented.
 */
@Composable
private fun Balance(federation: FederationInfo, balance: HomeViewModel.Balance, caps: Capabilities?) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Overline("Total balance")
        when (balance) {
            HomeViewModel.Balance.Loading -> Box(Modifier.height(48.dp), contentAlignment = Alignment.CenterStart) {
                CircularProgressIndicator(Modifier.size(28.dp), color = MaterialTheme.colorScheme.primary, strokeWidth = 3.dp)
            }
            is HomeViewModel.Balance.Value -> {
                AmountHero(formatSats(balance.msats))
                msatRemainder(balance.msats)?.let { MutedText("+ $it msats") }
            }
            is HomeViewModel.Balance.Unavailable -> {
                AmountHero("—")
                balance.message?.let { ErrorMessage(it) }
            }
        }
        Spacer(Modifier.height(6.dp))
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            val running = federation.status.canTransact
            Box(
                Modifier
                    .size(7.dp)
                    .clip(CircleShape)
                    .background(if (running) MaterialTheme.walletColors.success else MaterialTheme.walletColors.warning),
            )
            val modules = listOfNotNull(
                "LN".takeIf { caps?.lightning == true },
                "Ecash".takeIf { caps?.ecash == true },
                "Onchain".takeIf { caps?.onchain == true },
            )
            Text(
                (listOf(federation.network.label() + if (federation.network != Network.BITCOIN) " (test coins)" else "") + modules)
                    .joinToString(" · "),
                style = WalletText.mono,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun StatusNote(status: FederationStatus, text: String, onViewRecovery: () -> Unit) {
    val error = status is FederationStatus.Quarantined
    WalletCard(
        padding = PaddingValues(16.dp),
        spacing = 8.dp,
        color = if (error) MaterialTheme.colorScheme.errorContainer else MaterialTheme.walletColors.warningSoft,
        border = null,
    ) {
        Text(text, style = MaterialTheme.typography.bodyMedium)
        if (status is FederationStatus.Quarantined) {
            Text("code: ${status.diagnostic.code}", style = WalletText.mono, color = MaterialTheme.colorScheme.error)
        }
        if (status is FederationStatus.Recovering) {
            WalletButton("View progress", onViewRecovery, style = ButtonStyle.Secondary)
        }
    }
}

@Composable
private fun RecentActivity(items: List<ActivityItem>, onOpen: (OperationId) -> Unit, onSeeAll: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        SectionHeader("Recent activity", "See all", onSeeAll)
        if (items.isEmpty()) {
            MutedText("Nothing yet. Your payments will show up here.")
        } else {
            WalletCard {
                items.forEachIndexed { i, item ->
                    if (i > 0) CardDivider()
                    ActivityRow(item, onClick = { onOpen(item.operationId) })
                }
            }
        }
    }
}

private fun Capabilities?.hasAny(): Boolean = this != null && (ecash || lightning || onchain)
