package org.fedimint.demo.ui.payments

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import org.fedimint.demo.ui.common.ErrorMessage
import org.fedimint.demo.ui.common.Loading
import org.fedimint.demo.ui.common.NavHeader
import org.fedimint.demo.ui.common.ScanHandoff
import org.fedimint.demo.ui.common.Scanned
import org.fedimint.demo.ui.common.SegmentedControl
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.demo.wallet.WalletSession
import org.fedimint.sdk.Amount
import org.fedimint.sdk.Capabilities
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.FederationInfo
import org.fedimint.sdk.OperationId

/**
 * What the Send and Receive screens need about their federation: which rails
 * it runs (the tabs), its name, and the balance a send can spend.
 */
class PaymentContextViewModel(session: WalletSession, private val federationId: FederationId) : ViewModel() {
    data class UiState(
        val rails: List<Rail>? = null,
        val federation: FederationInfo? = null,
        val balance: Amount? = null,
        val error: String? = null,
    )

    private val rails = MutableStateFlow<Pair<List<Rail>?, String?>>(null to null)

    init {
        viewModelScope.launch {
            rails.value = attempt { session.capabilities(federationId) }.fold(
                { caps -> caps?.rails().orEmpty() to null },
                { e -> emptyList<Rail>() to userMessage(e) },
            )
        }
    }

    val state = combine(
        rails,
        session.federations.map { list -> list?.firstOrNull { it.id == federationId } },
        session.balance(federationId).map<Amount, Amount?> { it }.onStart { emit(null) }.catch { emit(null) },
    ) { (rails, error), federation, balance ->
        UiState(rails, federation, balance, error)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), UiState())
}

/** The rails a federation runs, in the order the tabs show them. */
fun Capabilities.rails(): List<Rail> = buildList {
    if (lightning) add(Rail.Lightning)
    if (ecash) add(Rail.Ecash)
    if (onchain) add(Rail.Onchain)
}

/**
 * How the rail on screen wants the frame drawn: a title, whether the rail
 * tabs show (hidden while reviewing a quote), what Back does (cancel the
 * review rather than leave), and whether a step is running. Each pane sets all
 * of them as it composes.
 */
@Stable
class PaneChrome(defaultTitle: String) {
    var title by mutableStateOf(defaultTitle)
    var showTabs by mutableStateOf(true)
    var back by mutableStateOf<(() -> Unit)?>(null)

    /** A submission is running: Back waits, since leaving would cancel it (a send, a redeem) mid-call. */
    var busy by mutableStateOf(false)

    fun set(title: String, busy: Boolean, showTabs: Boolean = true, back: (() -> Unit)? = null) {
        this.title = title
        this.busy = busy
        this.showTabs = showTabs
        this.back = back
    }
}

/** Send: one screen, a tab per rail the federation runs (the design's screens 14 and 06). */
@Composable
fun SendScreen(federationId: FederationId, onBack: () -> Unit, onOpenOperation: (OperationId) -> Unit) {
    val initial = when (ScanHandoff.peek()) {
        is Scanned.Invoice -> Rail.Lightning
        is Scanned.Address -> Rail.Onchain
        else -> null
    }
    PaymentFrame(federationId, "Send", initial, onBack) { rail, chrome, context ->
        when (rail) {
            Rail.Lightning -> LightningSendPane(federationId, chrome, context.balance, onBack, onOpenOperation)
            Rail.Ecash -> EcashSendPane(federationId, chrome, context, onBack, onOpenOperation)
            Rail.Onchain -> OnchainSendPane(federationId, chrome, context.balance, onBack, onOpenOperation)
        }
    }
}

/** Receive: one screen, a tab per rail the federation runs (the design's screens 05, 15 and 16). */
@Composable
fun ReceiveScreen(federationId: FederationId, onBack: () -> Unit, onOpenOperation: (OperationId) -> Unit) {
    val initial = if (ScanHandoff.peek() is Scanned.Notes) Rail.Ecash else null
    PaymentFrame(federationId, "Receive", initial, onBack) { rail, chrome, context ->
        when (rail) {
            Rail.Lightning -> LightningReceivePane(federationId, chrome, onBack, onOpenOperation)
            Rail.Ecash -> EcashReceivePane(federationId, chrome, context, onBack, onOpenOperation)
            Rail.Onchain -> OnchainReceivePane(federationId, chrome, context, onBack, onOpenOperation)
        }
    }
}

/**
 * The frame both share. Each rail keeps its own ViewModel for the life of the
 * screen, so switching tabs and back keeps what was typed or created.
 */
@Composable
private fun PaymentFrame(
    federationId: FederationId,
    title: String,
    initial: Rail?,
    onBack: () -> Unit,
    pane: @Composable (Rail, PaneChrome, PaymentContextViewModel.UiState) -> Unit,
) {
    val vm = appViewModel { PaymentContextViewModel(it.session, federationId) }
    val context by vm.state.collectAsStateWithLifecycle()
    val chrome = remember { PaneChrome(title) }
    var selected by rememberSaveable { mutableStateOf(initial?.name) }
    val drafts = rememberSaveableStateHolder()

    val back = chrome.back
    // One rule for every rail: while a step runs Back does nothing; otherwise it
    // cancels a review if there is one, else leaves.
    BackHandler(enabled = chrome.busy || back != null) { if (!chrome.busy) back?.invoke() }

    Column(
        Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .statusBarsPadding()
            .imePadding(),
    ) {
        NavHeader(chrome.title, onBack = back ?: onBack, backEnabled = !chrome.busy)
        val rails = context.rails
        when {
            rails == null -> Loading()
            rails.isEmpty() -> Box(Modifier.padding(20.dp)) {
                ErrorMessage(context.error ?: "This federation doesn't offer any way to ${title.lowercase()} right now.")
            }
            else -> {
                val rail = rails.firstOrNull { it.name == selected } ?: rails.first()
                if (chrome.showTabs && rails.size > 1) {
                    Box(Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, bottom = 8.dp)) {
                        SegmentedControl(rails, rail, { it.title }, { selected = it.name })
                    }
                }
                // Each rail's typed drafts are kept while another tab is shown, as its ViewModel is.
                Box(Modifier.weight(1f)) { drafts.SaveableStateProvider(rail.name) { pane(rail, chrome, context) } }
            }
        }
    }
}
