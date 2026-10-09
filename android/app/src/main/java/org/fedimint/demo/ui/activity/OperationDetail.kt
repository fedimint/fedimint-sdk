package org.fedimint.demo.ui.activity

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Card
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import org.fedimint.demo.ui.common.absoluteTime
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.formatSats
import org.fedimint.demo.ui.common.shortId
import org.fedimint.demo.ui.payments.ErrorText
import org.fedimint.demo.ui.payments.OutputCard
import org.fedimint.demo.ui.payments.PaymentScreen
import org.fedimint.demo.ui.payments.PaymentViewModel
import org.fedimint.demo.ui.payments.ProgressCard
import org.fedimint.demo.ui.payments.gatewayId
import org.fedimint.demo.ui.payments.label
import org.fedimint.demo.ui.payments.progress
import org.fedimint.demo.wallet.History
import org.fedimint.demo.wallet.Payments
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
                    title = "On-chain deposit",
                    rows = listOfNotNull(
                        d.grossDeposited?.let { "Deposited" to sats(it * 1_000uL) },
                        d.fee?.let { "Fee" to sats(it) },
                        d.netCredit?.let { "Credited" to sats(it) },
                        "Created" to absoluteTime(d.createdAt),
                    ),
                    copyable = listOfNotNull("Address" to d.address, d.txid?.let { "Transaction" to it }),
                )
                follow(Payments.states(o::updates) { it.next() }) { it.progress() }
            }
            OperationKind.ONCHAIN_SEND -> {
                val o = checkNotNull(op.asOnchainSend()).owned()
                val d = o.details()
                _detail.value = Detail(
                    title = "On-chain send",
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

    private fun sats(msats: ULong) = "${formatSats(msats)} sats"
}

@Composable
fun OperationDetailScreen(federationId: FederationId, operationId: OperationId, onBack: () -> Unit) {
    val vm = appViewModel { OperationDetailViewModel(it.history, federationId, operationId) }
    val state by vm.state.collectAsStateWithLifecycle()
    val detail by vm.detail.collectAsStateWithLifecycle()
    val notes by vm.ecashNotes.collectAsStateWithLifecycle()
    val canReclaim by vm.canReclaim.collectAsStateWithLifecycle()
    var showNotes by remember { mutableStateOf(false) }
    var confirmReclaim by remember { mutableStateOf(false) }

    PaymentScreen(detail?.title ?: "Payment", onBack) {
        val d = detail
        if (d == null) {
            if (state.working) CircularProgressIndicator(Modifier.align(Alignment.CenterHorizontally))
        } else {
            state.progress?.let { ProgressCard(it) }
            d.unsupported?.let { Text(it, style = MaterialTheme.typography.bodyLarge) }
            if (d.rows.isNotEmpty()) Facts(d.rows)
            d.copyable.forEach { (label, value) -> Copyable(label, value) }

            val n = notes
            if (n != null) {
                if (showNotes) {
                    OutputCard(PaymentViewModel.Output("Ecash notes", n))
                } else {
                    OutlinedButton(onClick = { showNotes = true }, modifier = Modifier.fillMaxWidth()) {
                        Text("Show notes again")
                    }
                }
            }
            if (canReclaim) {
                OutlinedButton(
                    onClick = { confirmReclaim = true },
                    enabled = !state.working,
                    modifier = Modifier.fillMaxWidth(),
                ) { Text("Reclaim unredeemed notes") }
            }
        }
        ErrorText(state.error)
    }

    if (confirmReclaim) {
        AlertDialog(
            onDismissRequest = { confirmReclaim = false },
            title = { Text("Reclaim these notes?") },
            text = {
                Text(
                    "The federation takes them back into your balance. If the receiver redeems " +
                        "them first, they keep them and nothing comes back.",
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    confirmReclaim = false
                    showNotes = false
                    vm.reclaim()
                }) { Text("Reclaim") }
            },
            dismissButton = { TextButton(onClick = { confirmReclaim = false }) { Text("Cancel") } },
        )
    }
}

@Composable
private fun Facts(rows: List<Pair<String, String>>) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            rows.forEach { (label, value) ->
                Row(Modifier.fillMaxWidth()) {
                    Text(label, Modifier.weight(1f), color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Text(value)
                }
            }
        }
    }
}

@Composable
private fun Copyable(label: String, value: String) {
    val clipboard = LocalClipboardManager.current
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp)) {
            Text(label, style = MaterialTheme.typography.labelLarge)
            Text(value, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall, maxLines = 3)
            TextButton(onClick = { clipboard.setText(AnnotatedString(value)) }) { Text("Copy") }
        }
    }
}
