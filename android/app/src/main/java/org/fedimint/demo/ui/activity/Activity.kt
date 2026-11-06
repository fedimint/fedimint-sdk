package org.fedimint.demo.ui.activity

import androidx.annotation.DrawableRes
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.fedimint.demo.R
import org.fedimint.demo.ui.common.ButtonStyle
import org.fedimint.demo.ui.common.CardDivider
import org.fedimint.demo.ui.common.ChoicePill
import org.fedimint.demo.ui.common.ErrorMessage
import org.fedimint.demo.ui.common.IconTile
import org.fedimint.demo.ui.common.Loading
import org.fedimint.demo.ui.common.MutedText
import org.fedimint.demo.ui.common.Overline
import org.fedimint.demo.ui.common.TabTitle
import org.fedimint.demo.ui.common.WalletButton
import org.fedimint.demo.ui.common.WalletCard
import org.fedimint.demo.ui.common.WalletScreen
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.formatSats
import org.fedimint.demo.ui.common.isOpen
import org.fedimint.demo.ui.common.label
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.demo.ui.theme.WalletText
import org.fedimint.demo.ui.theme.walletColors
import org.fedimint.demo.wallet.WalletSession
import org.fedimint.sdk.ActivityItem
import org.fedimint.sdk.ActivityPage
import org.fedimint.sdk.ActivityStatus
import org.fedimint.sdk.Cursor
import org.fedimint.sdk.Direction
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.FederationInfo
import org.fedimint.sdk.OperationId
import org.fedimint.sdk.OperationKind
import org.fedimint.sdk.Timestamp

/**
 * The federation's history, newest first, a page at a time. The next page
 * loads when the user asks for more; pulling down starts over from the first
 * page, which is also how pending rows get their latest status.
 *
 * Refresh and paging can overlap, so every load carries the generation it was
 * started in. A refresh starts a new generation and cancels any page still
 * loading; a result lands only if its generation is current and, for a later
 * page, it continues from the cursor it was asked for. Rows are also kept
 * unique by operation id, which the list's keys require.
 *
 * Takes the page loader rather than the SDK, so the rules are testable.
 */
class ActivityViewModel(private val load: suspend (cursor: Cursor?) -> ActivityPage) : ViewModel() {
    data class UiState(
        val items: List<ActivityItem> = emptyList(),
        /** The cursor for the page after [items], or null once the end is reached. */
        val next: Cursor? = null,
        val loaded: Boolean = false,
        val loadingMore: Boolean = false,
        val refreshing: Boolean = false,
        val error: String? = null,
    ) {
        val hasMore: Boolean get() = next != null
    }

    private val _state = MutableStateFlow(UiState())
    val state = _state.asStateFlow()

    private var generation = 0
    private var refreshJob: Job? = null
    private var moreJob: Job? = null

    init {
        refresh()
    }

    fun refresh() {
        if (_state.value.refreshing) return
        val gen = ++generation
        moreJob?.cancel()
        _state.update { it.copy(refreshing = true, loadingMore = false, error = null) }
        refreshJob = viewModelScope.launch {
            attempt { load(null) }
                .onSuccess { page ->
                    if (gen != generation) return@onSuccess
                    _state.value = UiState(items = page.items.distinctBy { it.operationId }, next = page.next, loaded = true)
                }
                .onFailure { e ->
                    if (gen != generation) return@onFailure
                    _state.update { it.copy(refreshing = false, loaded = true, error = userMessage(e)) }
                }
        }
    }

    fun loadMore() {
        val s = _state.value
        val cursor = s.next ?: return
        if (s.loadingMore || s.refreshing) return
        val gen = generation
        _state.update { it.copy(loadingMore = true) }
        moreJob = viewModelScope.launch {
            attempt { load(cursor) }
                .onSuccess { page ->
                    _state.update {
                        // Stale: a refresh happened, or another page already continued from here.
                        if (gen != generation) {
                            it
                        } else if (it.next != cursor) {
                            it.copy(loadingMore = false)
                        } else {
                            it.copy(
                                items = (it.items + page.items).distinctBy { item -> item.operationId },
                                next = page.next,
                                loadingMore = false,
                            )
                        }
                    }
                }
                .onFailure { e ->
                    if (gen != generation) return@onFailure
                    _state.update { it.copy(loadingMore = false, error = userMessage(e)) }
                }
        }
    }
}

/** The federation Home shows, which the Activity tab follows. Null inside while loading or when there is none. */
class ActiveFederationViewModel(session: WalletSession) : ViewModel() {
    data class UiState(val loaded: Boolean = false, val active: FederationInfo? = null)

    val state = combine(session.federations, session.selectedFederationId) { list, selected ->
        UiState(loaded = list != null, active = list?.let { WalletSession.pickActive(it, selected) })
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), UiState())
}

/** Which rail's operations the list shows; the filter applies to the rows loaded so far. */
private enum class Filter(val label: String, val kinds: Set<OperationKind>?) {
    All("All", null),
    Lightning("Lightning", setOf(OperationKind.LN_SEND, OperationKind.LN_RECEIVE)),
    Ecash("Ecash", setOf(OperationKind.ECASH_SEND, OperationKind.ECASH_RECEIVE)),
    Onchain("Onchain", setOf(OperationKind.ONCHAIN_SEND, OperationKind.ONCHAIN_RECEIVE)),
}

/** The Activity tab: the history of the federation on Home. */
@Composable
fun ActivityTabScreen(onOpen: (FederationId, OperationId) -> Unit, onJoinFederation: () -> Unit) {
    val vm = appViewModel { ActiveFederationViewModel(it.session) }
    val state by vm.state.collectAsStateWithLifecycle()
    val active = state.active
    when {
        !state.loaded -> WalletScreen { TabTitle("Activity"); Loading() }
        active == null || !active.status.isOpen -> WalletScreen {
            TabTitle("Activity")
            MutedText(
                if (active == null) {
                    "Join a federation to start. Payments you send and receive show up here."
                } else {
                    "${active.name ?: "This federation"} is ${active.status.label().lowercase()}. Reopen it to see its activity."
                },
            )
            if (active == null) WalletButton("Join with invite code", onJoinFederation, style = ButtonStyle.Secondary)
        }
        else -> ActivityList(active.id, onOpen = { onOpen(active.id, it) })
    }
}

/**
 * One federation's history, grouped by day, newest first. "Load more" fetches
 * the next page; pulling down starts over, which is also how pending rows get
 * their latest status.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ActivityList(federationId: FederationId, onOpen: (OperationId) -> Unit) {
    val vm = appViewModel(key = federationId) { c -> ActivityViewModel { cursor -> c.history.page(federationId, cursor) } }
    val state by vm.state.collectAsStateWithLifecycle()
    var filter by rememberSaveable { mutableStateOf(Filter.All) }
    val shown = state.items.filter { filter.kinds == null || it.kind in filter.kinds!! }
    val groups = shown.groupBy { dayLabel(it.time) }

    PullToRefreshBox(
        isRefreshing = state.refreshing && state.loaded,
        onRefresh = vm::refresh,
        modifier = Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background).statusBarsPadding(),
    ) {
        LazyColumn(
            Modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = 20.dp, end = 20.dp, top = 8.dp, bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item { TabTitle("Activity") }
            item {
                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Filter.entries.forEach { f -> ChoicePill(f.label, f == filter, { filter = f }) }
                }
            }
            when {
                !state.loaded -> item { Loading() }
                shown.isEmpty() && !state.hasMore -> item {
                    if (state.error != null) {
                        ErrorMessage(state.error)
                    } else {
                        MutedText(
                            if (filter == Filter.All) {
                                "No activity yet. Payments you send and receive will show up here."
                            } else {
                                "No ${filter.label} payments yet."
                            },
                        )
                    }
                }
                else -> {
                    groups.forEach { (day, rows) ->
                        item(key = "day-$day") { Overline(day, Modifier.padding(top = 8.dp)) }
                        item(key = "rows-$day") {
                            WalletCard {
                                rows.forEachIndexed { i, row ->
                                    if (i > 0) CardDivider()
                                    ActivityRow(row, onClick = { onOpen(row.operationId) }, showDay = false)
                                }
                            }
                        }
                    }
                    if (state.hasMore) {
                        item {
                            WalletButton("Load more", vm::loadMore, style = ButtonStyle.Secondary, working = state.loadingMore)
                        }
                    }
                    state.error?.let { item { ErrorMessage(it) } }
                }
            }
        }
    }
}

/**
 * One history row: an icon for its rail, what happened, when (or that it is
 * still pending), and the signed amount, green when money came in.
 */
@Composable
fun ActivityRow(item: ActivityItem, onClick: () -> Unit, showDay: Boolean = true) {
    val incoming = item.direction == Direction.INCOMING
    val settledBadly = item.status in setOf(ActivityStatus.FAILED, ActivityStatus.REFUNDED, ActivityStatus.CANCELED)
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        IconTile(item.icon())
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(item.headline(), style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
            val time = if (item.status == ActivityStatus.PENDING) "Pending" else if (showDay) shortTime(item.time) else clockTime(item.time)
            MutedText("${item.kind.rail()} · ${if (item.status == ActivityStatus.UNKNOWN) item.statusLabel() else time}")
        }
        item.amount?.let { amount ->
            val sign = when (item.direction) {
                Direction.INCOMING -> "+"
                Direction.OUTGOING -> "−"
                null -> ""
            }
            Text(
                "$sign${formatSats(amount)}",
                style = WalletText.amount,
                color = when {
                    settledBadly -> MaterialTheme.colorScheme.onSurfaceVariant
                    incoming && item.status == ActivityStatus.SUCCESS -> MaterialTheme.walletColors.success
                    else -> MaterialTheme.colorScheme.onSurface
                },
            )
        }
    }
}

@DrawableRes
private fun ActivityItem.icon(): Int = when (kind) {
    OperationKind.LN_SEND -> R.drawable.ic_zap
    OperationKind.LN_RECEIVE -> R.drawable.ic_arrow_down_left
    OperationKind.ECASH_SEND, OperationKind.ECASH_RECEIVE -> R.drawable.ic_coins
    OperationKind.ONCHAIN_SEND, OperationKind.ONCHAIN_RECEIVE -> R.drawable.ic_link
    OperationKind.RECOVERY -> R.drawable.ic_refresh
    OperationKind.UNKNOWN -> R.drawable.ic_alert
}

/** What happened, in a word or two: "Received", "Sending", "Deposit", "Reclaimed". */
private fun ActivityItem.headline(): String {
    val incoming = direction == Direction.INCOMING
    return when {
        kind == OperationKind.RECOVERY || kind == OperationKind.UNKNOWN -> kind.title()
        status == ActivityStatus.PENDING -> if (incoming) "Receiving" else "Sending"
        status == ActivityStatus.SUCCESS -> when (kind) {
            OperationKind.ONCHAIN_RECEIVE -> "Deposit"
            OperationKind.ONCHAIN_SEND -> "Withdraw"
            else -> if (incoming) "Received" else "Sent"
        }
        status == ActivityStatus.FAILED -> "Failed"
        status == ActivityStatus.REFUNDED -> "Refunded"
        status == ActivityStatus.CANCELED -> if (kind == OperationKind.ECASH_SEND) "Reclaimed" else "Canceled"
        else -> kind.title()
    }
}

/** The rail an operation ran on, as the tabs name it. */
fun OperationKind.rail(): String = when (this) {
    OperationKind.LN_SEND, OperationKind.LN_RECEIVE -> "Lightning"
    OperationKind.ECASH_SEND, OperationKind.ECASH_RECEIVE -> "Ecash"
    OperationKind.ONCHAIN_SEND, OperationKind.ONCHAIN_RECEIVE -> "Onchain"
    OperationKind.RECOVERY -> "Recovery"
    OperationKind.UNKNOWN -> "Unknown"
}

private fun localDate(ts: Timestamp): LocalDate = Instant.ofEpochMilli(ts.toLong()).atZone(ZoneId.systemDefault()).toLocalDate()

/** "Today", "Yesterday", or the date: the Activity tab's group headers. */
private fun dayLabel(ts: Timestamp): String {
    val day = localDate(ts)
    val today = LocalDate.now()
    return when (day) {
        today -> "Today"
        today.minusDays(1) -> "Yesterday"
        else -> day.format(DateTimeFormatter.ofPattern(if (day.year == today.year) "MMM d" else "MMM d, yyyy"))
    }
}

private fun clockTime(ts: Timestamp): String =
    DateTimeFormatter.ofPattern("HH:mm").withZone(ZoneId.systemDefault()).format(Instant.ofEpochMilli(ts.toLong()))

/** A time today, else the day: for rows outside a day group, like Home's. */
private fun shortTime(ts: Timestamp): String = if (localDate(ts) == LocalDate.now()) clockTime(ts) else dayLabel(ts)

fun OperationKind.title(): String = when (this) {
    OperationKind.LN_SEND -> "Lightning payment"
    OperationKind.LN_RECEIVE -> "Lightning receive"
    OperationKind.ECASH_SEND -> "Ecash sent"
    OperationKind.ECASH_RECEIVE -> "Ecash redeemed"
    OperationKind.ONCHAIN_SEND -> "On-chain send"
    OperationKind.ONCHAIN_RECEIVE -> "On-chain deposit"
    OperationKind.RECOVERY -> "Wallet recovery"
    OperationKind.UNKNOWN -> "Unrecognised operation"
}

internal fun ActivityItem.statusLabel(): String = when (status) {
    ActivityStatus.PENDING -> "Pending"
    ActivityStatus.SUCCESS -> if (direction == Direction.INCOMING) "Received" else "Completed"
    ActivityStatus.FAILED -> "Failed"
    ActivityStatus.REFUNDED -> "Refunded"
    ActivityStatus.CANCELED -> if (kind == OperationKind.ECASH_SEND) "Reclaimed" else "Canceled"
    // A row this SDK version can't interpret: say so rather than guess an outcome.
    ActivityStatus.UNKNOWN -> if (isFinal) "Finished (details unavailable)" else "In progress (details unavailable)"
}
