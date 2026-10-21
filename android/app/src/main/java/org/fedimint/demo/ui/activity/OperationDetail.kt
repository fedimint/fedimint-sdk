package org.fedimint.demo.ui.activity

import androidx.annotation.DrawableRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import org.fedimint.demo.R
import org.fedimint.demo.ui.common.AmountHero
import org.fedimint.demo.ui.common.ButtonStyle
import org.fedimint.demo.ui.common.CardDivider
import org.fedimint.demo.ui.common.DetailRow
import org.fedimint.demo.ui.common.IconTile
import org.fedimint.demo.ui.common.Loading
import org.fedimint.demo.ui.common.MutedText
import org.fedimint.demo.ui.common.Overline
import org.fedimint.demo.ui.common.QrCard
import org.fedimint.demo.ui.common.WalletButton
import org.fedimint.demo.ui.common.WalletCard
import org.fedimint.demo.ui.common.WalletScreen
import org.fedimint.demo.ui.common.absoluteTime
import org.fedimint.demo.ui.common.appContainer
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.formatSats
import org.fedimint.demo.ui.common.shortId
import org.fedimint.demo.ui.payments.ErrorText
import org.fedimint.demo.ui.payments.OpProgress
import org.fedimint.demo.ui.payments.PaymentViewModel
import org.fedimint.demo.ui.payments.ProgressCard
import org.fedimint.demo.ui.payments.ProgressPill
import org.fedimint.demo.ui.payments.ReclaimDialog
import org.fedimint.demo.ui.payments.gatewayId
import org.fedimint.demo.ui.payments.label
import org.fedimint.demo.ui.payments.progress
import org.fedimint.demo.ui.payments.rememberCopy
import org.fedimint.demo.ui.payments.rememberCopyNotes
import org.fedimint.demo.ui.payments.sats
import org.fedimint.demo.wallet.History
import org.fedimint.demo.wallet.Payments
import org.fedimint.sdk.Amount
import org.fedimint.sdk.AnyOperation
import org.fedimint.sdk.EcashSendOperation
import org.fedimint.sdk.EcashSendState
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.OperationId
import org.fedimint.sdk.OperationKind
import org.fedimint.sdk.OperationSupport

/**
 * One operation from the history: its recorded details and its live state.
 *
 * History hands back an untyped [AnyOperation]. [AnyOperation.support] is asked
 * first, because an operation recorded by a newer SDK (or for a module this
 * build doesn't know) is still real but has no typed handle; the screen says
 * so plainly instead of pretending it failed. Only then is it narrowed with
 * `asLnSend()` and friends, which also unlocks the kind's own actions, like
 * reclaiming unredeemed ecash.
 */
class OperationDetailViewModel(
    private val history: History,
    private val federationId: FederationId,
    private val operationId: OperationId,
) : PaymentViewModel() {

    data class Detail(
        val title: String,
        val rows: List<Pair<String, String>>,
        /** The amount that moved, and which way; null where there isn't one (recovery, unknown). */
        val amount: Amount? = null,
        val incoming: Boolean = false,
        /** Long values worth copying: an invoice, an address, a txid. */
        val copyable: List<Pair<String, String>> = emptyList(),
        /** Why there is no live state, for an operation this build can't read. */
        val unsupported: String? = null,
    )

    private val _detail = MutableStateFlow<Detail?>(null)
    val detail = _detail.asStateFlow()

    /** Set only for an ecash send: its notes, while they can still be handed over or reclaimed. */
    private val _ecashNotes = MutableStateFlow<String?>(null)
    val ecashNotes = _ecashNotes.asStateFlow()

    private val _canReclaim = MutableStateFlow(false)
    val canReclaim = _canReclaim.asStateFlow()

    private var ecashSend: EcashSendOperation? = null

    init {
        step { load() }
    }

    /**
     * Asks the federation to take back ecash notes nobody has redeemed. The
     * request is durable the moment this returns; the outcome (reclaimed, or
     * redeemed first by the receiver) arrives as a state change.
     */
    fun reclaim() = step { ecashSend?.requestCancel() }

    private suspend fun load() {
        val op = history.operation(federationId, operationId)?.owned()
            ?: throw IllegalStateException("This payment isn't in this federation's history.")

        if (op.support() != OperationSupport.OBSERVABLE) {
            _detail.value = unsupported(op)
            return
        }
        when (op.kind()) {
            OperationKind.LN_RECEIVE -> {
                val o = checkNotNull(op.asLnReceive()).owned()
                val d = o.details()
                _detail.value = Detail(
                    title = "Lightning receive",
                    amount = d.netCredit,
                    incoming = true,
                    rows = listOfNotNull(
                        "Requested" to sats(d.requestedAmount),
                        "Fee" to sats(d.fee),
                        "Credited" to sats(d.netCredit),
                        d.description.takeIf { it.isNotBlank() }?.let { "Description" to it },
                        d.gatewayId?.let { "Gateway" to shortId(it) },
                        "Created" to absoluteTime(d.createdAt),
                        "Invoice expires" to absoluteTime(d.expiresAt),
                    ),
                    copyable = listOf("Invoice" to d.invoice),
                )
                follow(Payments.states(o::updates) { it.next() }) { it.progress() }
            }
            OperationKind.LN_SEND -> {
                val o = checkNotNull(op.asLnSend()).owned()
                val d = o.details()
                _detail.value = Detail(
                    title = "Lightning payment",
                    amount = d.total,
                    rows = listOfNotNull(
                        "Invoice amount" to sats(d.invoiceAmount),
                        "Fee" to sats(d.fee),
                        "Total" to sats(d.total),
                        "Route" to d.route.label(),
                        d.route.gatewayId()?.let { "Gateway" to shortId(it) },
                        "Created" to absoluteTime(d.createdAt),
                    ),
                    copyable = listOf("Invoice" to d.invoice),
                )
                follow(Payments.states(o::updates) { it.next() }) { it.progress() }
            }
            OperationKind.ECASH_SEND -> {
                val o = checkNotNull(op.asEcashSend()).owned()
                ecashSend = o
                val d = o.details()
                val notes = d.notes.owned()
                _detail.value = Detail(
                    title = "Ecash sent",
                    amount = d.totalDebited,
                    rows = listOf(
                        "Amount" to sats(d.requestedAmount),
                        "Notes value" to sats(d.notesValue),
                        "Fee" to sats(d.fee),
                        "Total debited" to sats(d.totalDebited),
                        "Created" to absoluteTime(d.createdAt),
                        // Fixed at creation: from then on an unredeemed send is reclaimed automatically.
                        "Automatic reclaim" to absoluteTime(d.reclaimAt),
                    ),
                )
                follow(Payments.states(o::updates) { it.next() }) { s ->
                    // The notes are only worth showing, and reclaiming only possible, until redeemed or reclaimed.
                    val open = s == EcashSendState.CREATED
                    _canReclaim.value = open
                    _ecashNotes.value = if (open) notes.display() else null
                    s.progress()
                }
            }
            OperationKind.ECASH_RECEIVE -> {
                val o = checkNotNull(op.asEcashReceive()).owned()
                val d = o.details()
                d.notes?.owned()
                _detail.value = Detail(
                    title = "Ecash redeemed",
                    amount = d.netCredit,
                    incoming = true,
                    rows = listOf(
                        "Notes value" to sats(d.notesValue),
                        "Fee" to sats(d.fee),
                        "Credited" to sats(d.netCredit),
                        "Created" to absoluteTime(d.createdAt),
                    ),
                )
                follow(Payments.states(o::updates) { it.next() }) { it.progress() }
            }
            OperationKind.ONCHAIN_RECEIVE -> {
                val o = checkNotNull(op.asOnchainReceive()).owned()
                val d = o.details()
                _detail.value = Detail(
                    title = "Onchain deposit",
                    amount = d.grossDeposited * 1_000uL,
                    incoming = true,
                    rows = listOfNotNull(
                        "Deposited" to sats(d.grossDeposited * 1_000uL),
                        d.fee?.let { "Fee" to sats(it) },
                        d.netCredit?.let { "Credited" to sats(it) },
                        "Created" to absoluteTime(d.createdAt),
                    ),
                    copyable = listOf("Address" to d.address, "Transaction" to d.txid),
                )
                follow(Payments.states(o::updates) { it.next() }) { it.progress() }
            }
            OperationKind.ONCHAIN_SEND -> {
                val o = checkNotNull(op.asOnchainSend()).owned()
                val d = o.details()
                _detail.value = Detail(
                    title = "Onchain send",
                    amount = d.total,
                    rows = listOf(
                        "Amount" to sats(d.amount * 1_000uL),
                        "Fee" to sats(d.fee),
                        "Total" to sats(d.total),
                        "Created" to absoluteTime(d.createdAt),
                    ),
                    copyable = listOf("Address" to d.address),
                )
                follow(Payments.states(o::updates) { it.next() }) { it.progress() }
            }
            OperationKind.RECOVERY -> {
                val o = checkNotNull(op.asRecovery()).owned()
                _detail.value = Detail(title = "Wallet recovery", rows = emptyList())
                follow(Payments.states(o::updates) { it.next() }) { it.progress() }
            }
            OperationKind.UNKNOWN -> _detail.value = unsupported(op)
        }
    }

    private fun unsupported(op: AnyOperation): Detail {
        val raw = op.rawKind()
        val why = when (op.support()) {
            OperationSupport.STATE_SCHEMA_TOO_NEW ->
                "It was recorded by a newer version of the app. Update the app to see its details."
            else -> "This version of the app doesn't recognise this kind of payment."
        }
        return Detail(
            title = "Unrecognised operation",
            rows = listOfNotNull(
                "Kind" to raw.kind,
                raw.module?.let { "Module" to it },
                raw.schemaVersion?.let { "Schema version" to it.toString() },
            ),
            unsupported = why,
        )
    }

}

@Composable
fun OperationDetailScreen(federationId: FederationId, operationId: OperationId, onBack: () -> Unit) {
    val vm = appViewModel { OperationDetailViewModel(it.history, federationId, operationId) }
    val federations by appContainer().session.federations.collectAsStateWithLifecycle()
    val federationName = federations?.firstOrNull { it.id == federationId }?.name
    val state by vm.state.collectAsStateWithLifecycle()
    val detail by vm.detail.collectAsStateWithLifecycle()
    val notes by vm.ecashNotes.collectAsStateWithLifecycle()
    val canReclaim by vm.canReclaim.collectAsStateWithLifecycle()
    var showNotes by remember { mutableStateOf(false) }
    var confirmReclaim by remember { mutableStateOf(false) }
    val copy = rememberCopy()
    val copyNotes = rememberCopyNotes()

    WalletScreen(
        title = "Payment",
        onBack = onBack,
        bottom = if (notes != null || canReclaim) {
            {
                if (notes != null && !showNotes) {
                    WalletButton("Show notes again", { showNotes = true }, style = ButtonStyle.Secondary)
                }
                if (canReclaim) {
                    WalletButton("Reclaim notes", { confirmReclaim = true }, style = ButtonStyle.Danger, working = state.working)
                }
            }
        } else {
            null
        },
    ) {
        val d = detail
        if (d == null) {
            if (state.working) Loading()
        } else {
            Summary(d, state.progress)
            d.unsupported?.let { MutedText(it) }
            WalletCard(padding = PaddingValues(horizontal = 16.dp)) {
                DetailRow("Operation ID", shortId(operationId, keep = 4), mono = true, onCopy = { copy(operationId) })
                federationName?.let {
                    CardDivider()
                    DetailRow("Federation", it)
                }
                d.rows.forEach { (label, value) ->
                    CardDivider()
                    DetailRow(label, value, mono = value.endsWith(" sats"))
                }
                d.copyable.forEach { (label, value) ->
                    CardDivider()
                    DetailRow(label, shortId(value, keep = 8), mono = true, onCopy = { copy(value) })
                }
            }
            state.progress?.let {
                Overline("Progress")
                ProgressCard(it, state.steps)
            }
            val n = notes
            if (n != null && showNotes) QrCard(n, n, onCopy = { copyNotes(n) })
        }
        ErrorText(state.error)
    }

    if (confirmReclaim) {
        ReclaimDialog(
            onConfirm = {
                confirmReclaim = false
                showNotes = false
                vm.reclaim()
            },
            onDismiss = { confirmReclaim = false },
        )
    }
}

/** The icon, what it was, the signed amount, and its state. */
@Composable
private fun Summary(detail: OperationDetailViewModel.Detail, progress: OpProgress?) {
    Column(
        Modifier.fillMaxWidth(),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        IconTile(detail.icon(), size = 52.dp, iconSize = 26.dp, radius = 14.dp)
        Text(detail.title, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        detail.amount?.let { AmountHero((if (detail.incoming) "+" else "−") + formatSats(it)) }
        progress?.let { ProgressPill(it) }
    }
}

@DrawableRes
private fun OperationDetailViewModel.Detail.icon(): Int = when {
    title.startsWith("Lightning") -> if (incoming) R.drawable.ic_arrow_down_left else R.drawable.ic_zap
    title.startsWith("Ecash") -> R.drawable.ic_coins
    title.startsWith("Onchain") -> R.drawable.ic_link
    title == "Wallet recovery" -> R.drawable.ic_refresh
    else -> R.drawable.ic_alert
}
