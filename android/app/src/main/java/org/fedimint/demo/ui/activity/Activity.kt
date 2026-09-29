package org.fedimint.demo.ui.activity

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.formatSats
import org.fedimint.demo.ui.common.relativeTime
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.demo.wallet.History
import org.fedimint.sdk.ActivityItem
import org.fedimint.sdk.ActivityStatus
import org.fedimint.sdk.Cursor
import org.fedimint.sdk.Direction
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.OperationId
import org.fedimint.sdk.OperationKind

/**
 * The federation's history, newest first, a page at a time. The next page
 * loads as the user nears the end of the list; pulling down starts over from
 * the first page, which is also how pending rows get their latest status.
 */
class ActivityViewModel(private val history: History, private val federationId: FederationId) : ViewModel() {
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

    init {
        refresh()
    }

    fun refresh() {
        if (_state.value.refreshing) return
        _state.update { it.copy(refreshing = true, error = null) }
        viewModelScope.launch {
            attempt { history.page(federationId) }
                .onSuccess { page -> _state.update { UiState(items = page.items, next = page.next, loaded = true) } }
                .onFailure { e -> _state.update { it.copy(refreshing = false, loaded = true, error = userMessage(e)) } }
        }
    }

    fun loadMore() {
        val cursor = _state.value.next ?: return
        if (_state.value.loadingMore || _state.value.refreshing) return
        _state.update { it.copy(loadingMore = true) }
        viewModelScope.launch {
            attempt { history.page(federationId, cursor) }
                .onSuccess { page ->
                    _state.update { it.copy(items = it.items + page.items, next = page.next, loadingMore = false) }
                }
                .onFailure { e -> _state.update { it.copy(loadingMore = false, error = userMessage(e)) } }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ActivityScreen(federationId: FederationId, onBack: () -> Unit, onOpen: (OperationId) -> Unit) {
    val vm = appViewModel { ActivityViewModel(it.history, federationId) }
    val state by vm.state.collectAsStateWithLifecycle()
    val list = rememberLazyListState()

    // Ask for the next page when the last few rows come into view.
    val nearEnd by remember {
        derivedStateOf {
            val last = list.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: 0
            last >= list.layoutInfo.totalItemsCount - 5
        }
    }
    LaunchedEffect(nearEnd, state.hasMore) { if (nearEnd && state.hasMore) vm.loadMore() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Activity") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
            )
        },
    ) { padding ->
        PullToRefreshBox(
            isRefreshing = state.refreshing && state.loaded,
            onRefresh = vm::refresh,
            modifier = Modifier.fillMaxSize().padding(padding),
        ) {
            when {
                !state.loaded -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
                state.items.isEmpty() -> LazyColumn(Modifier.fillMaxSize()) {
                    // Scrollable even when empty, so pull-to-refresh still works.
                    item {
                        Text(
                            state.error ?: "No activity yet. Payments you send and receive will show up here.",
                            modifier = Modifier.padding(24.dp),
                            color = if (state.error != null) MaterialTheme.colorScheme.error else Color.Unspecified,
                        )
                    }
                }
                else -> LazyColumn(state = list, modifier = Modifier.fillMaxSize()) {
                    items(state.items, key = { it.operationId }) { item ->
                        ActivityRow(item, onClick = { onOpen(item.operationId) })
                        HorizontalDivider()
                    }
                    if (state.loadingMore) {
                        item {
                            Box(Modifier.fillMaxWidth().padding(16.dp), contentAlignment = Alignment.Center) {
                                CircularProgressIndicator()
                            }
                        }
                    }
                    state.error?.let { item { Text(it, Modifier.padding(16.dp), color = MaterialTheme.colorScheme.error) } }
                }
            }
        }
    }
}

/** One history row: what it was, when, how it ended, and the signed amount. */
@Composable
fun ActivityRow(item: ActivityItem, onClick: () -> Unit) {
    val incoming = item.direction == Direction.INCOMING
    val settledBadly = item.status in setOf(ActivityStatus.FAILED, ActivityStatus.REFUNDED, ActivityStatus.CANCELED)
    ListItem(
        headlineContent = { Text(item.kind.title()) },
        supportingContent = { Text("${item.statusLabel()} · ${relativeTime(item.time)}") },
        trailingContent = {
            item.amount?.let { amount ->
                val sign = when (item.direction) {
                    Direction.INCOMING -> "+"
                    Direction.OUTGOING -> "−"
                    null -> ""
                }
                Text(
                    "$sign${formatSats(amount)} sats",
                    style = MaterialTheme.typography.bodyLarge,
                    color = when {
                        settledBadly -> MaterialTheme.colorScheme.onSurfaceVariant
                        incoming -> MaterialTheme.colorScheme.primary
                        else -> MaterialTheme.colorScheme.onSurface
                    },
                )
            }
        },
        modifier = Modifier.clickable(onClick = onClick),
    )
}

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

private fun ActivityItem.statusLabel(): String = when (status) {
    ActivityStatus.PENDING -> "Pending"
    ActivityStatus.SUCCESS -> if (direction == Direction.INCOMING) "Received" else "Completed"
    ActivityStatus.FAILED -> "Failed"
    ActivityStatus.REFUNDED -> "Refunded"
    ActivityStatus.CANCELED -> if (kind == OperationKind.ECASH_SEND) "Reclaimed" else "Canceled"
    // A row this SDK version can't interpret: say so rather than guess an outcome.
    ActivityStatus.UNKNOWN -> if (isFinal) "Finished (details unavailable)" else "In progress (details unavailable)"
}
