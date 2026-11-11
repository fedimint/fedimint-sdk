package org.fedimint.demo.ui.payments

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextAlign
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.createSavedStateHandle
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import org.fedimint.demo.R
import org.fedimint.demo.ui.common.AmountHero
import org.fedimint.demo.ui.common.AmountInput
import org.fedimint.demo.ui.common.ButtonStyle
import org.fedimint.demo.ui.common.DetailCard
import org.fedimint.demo.ui.common.MutedText
import org.fedimint.demo.ui.common.Overline
import org.fedimint.demo.ui.common.QrCard
import org.fedimint.demo.ui.common.ScanHandoff
import org.fedimint.demo.ui.common.Scanned
import org.fedimint.demo.ui.common.StatusPill
import org.fedimint.demo.ui.common.Tone
import org.fedimint.demo.ui.common.WalletButton
import org.fedimint.demo.ui.common.WalletTextField
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.formatSats
import org.fedimint.demo.ui.common.shareText
import org.fedimint.demo.wallet.History
import org.fedimint.demo.wallet.Payments
import org.fedimint.sdk.Amount
import org.fedimint.sdk.EcashQuote
import org.fedimint.sdk.EcashSendOperation
import org.fedimint.sdk.EcashSendState
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.OperationId
import org.fedimint.sdk.OperationKind

// ── Send ─────────────────────────────────────────────────────────────────

class EcashSendViewModel(
    private val payments: Payments,
    private val federationId: FederationId,
    savedState: SavedStateHandle,
    history: History,
) : PaymentViewModel(savedState, reconcileWith(history, federationId, OperationKind.ECASH_SEND)) {

    private val quote = SingleUseQuote<EcashQuote>()

    fun onAmountChanged() {
        quote.drop()
        clearReview()
    }

    fun quote(sats: ULong) = step {
        val q = quote.hold(payments.ecashQuote(federationId, sats * 1_000uL))
        val rows = buildList {
            add("Via" to "Ecash notes")
            // Notes come in fixed denominations, so the value handed over can exceed what was asked for.
            if (q.notesValue() != q.requestedAmount()) add("Notes value" to sats(q.notesValue()))
            add("Fee" to sats(q.fee()))
        }
        val review = Review(rows, sats(q.total()), expiry(q.expiresAt()), amount = formatSats(q.requestedAmount()))
        mutableState.update { it.copy(review = review) }
    }

    private var sent: EcashSendOperation? = null

    /** The amount the notes on screen were created for. */
    private val _sentAmount = MutableStateFlow<Amount?>(null)
    val sentAmount = _sentAmount.asStateFlow()

    /** The notes can still be reclaimed: nobody has redeemed them and no reclaim was asked for. */
    private val _canReclaim = MutableStateFlow(false)
    val canReclaim = _canReclaim.asStateFlow()

    fun send() = execute {
        // Single use, and spent even by a failed send: taken, so nothing can submit it
        // twice. A failure drops the review (execute).
        val q = quote.take()
        val handle = payments.ecashSend(federationId, q)
        _sentAmount.value = q.requestedAmount()
        val notes = handle.notes.owned()
        val operation = handle.operation.owned()
        sent = operation
        // `display()` is the deliberate way to take notes out as text; the
        // handle never prints them. They are a bearer instrument, like cash.
        mutableState.update { it.copy(output = Output("Ecash notes", notes.display())) }
        follow(Payments.states(operation::updates) { it.next() }) { s ->
            _canReclaim.value = s == EcashSendState.CREATED
            s.progress()
        }
        operation.id()
    }

    /**
     * Asks the federation to take the notes back. Durable once this returns;
     * the outcome (reclaimed, or redeemed first) arrives as a state change.
     */
    fun reclaim() = step { sent?.requestCancel() }
}

@Composable
fun EcashSendPane(
    federationId: FederationId,
    chrome: PaneChrome,
    context: PaymentContextViewModel.UiState,
    onDone: () -> Unit,
    onOpenOperation: (OperationId) -> Unit,
) {
    val vm = appViewModel { EcashSendViewModel(it.payments, federationId, createSavedStateHandle(), it.history) }
    val state by vm.state.collectAsStateWithLifecycle()
    val canReclaim by vm.canReclaim.collectAsStateWithLifecycle()
    val sentAmount by vm.sentAmount.collectAsStateWithLifecycle()
    var amount by rememberSaveable { mutableStateOf("") }
    var confirmReclaim by remember { mutableStateOf(false) }
    val sats = amount.toULongOrNull()
    val review = state.review
    val output = state.output
    val android = LocalContext.current
    val copyNotes = rememberCopyNotes()
    SideEffect {
        when {
            output != null -> chrome.set("Ecash notes", busy = state.submitting, showTabs = false)
            review != null && !state.started -> chrome.set("Review payment", busy = state.submitting, showTabs = false, back = vm::onAmountChanged)
            else -> chrome.set("Send", busy = state.submitting)
        }
    }

    PaymentPane(
        bottom = {
            when {
                output != null && canReclaim -> {
                    WalletButton("Share notes", { shareText(android, output.text, "Share ecash notes") }, icon = R.drawable.ic_share)
                    WalletButton("Reclaim notes", { confirmReclaim = true }, style = ButtonStyle.Secondary, working = state.working)
                }
                // A reclaim was asked for and hasn't settled yet (the one unsettled state of an ecash send).
                output != null && state.progress?.settled == false ->
                    WalletButton("Reclaiming…", {}, style = ButtonStyle.Secondary, enabled = false)
                state.started -> DoneButton(state, onDone)
                review == null -> WalletButton("Review", { vm.quote(sats ?: 0uL) }, enabled = sats != null && sats > 0uL, working = state.working)
                else -> {
                    WalletButton("Create notes", vm::send, working = state.working)
                    WalletButton("Cancel", vm::onAmountChanged, style = ButtonStyle.Secondary, enabled = !state.working)
                }
            }
        },
    ) {
        when {
            output != null -> {
                Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                    sentAmount?.let { AmountHero(formatSats(it)) }
                    context.federation?.name?.let { MutedText("From $it") }
                }
                val progress = state.progress
                if (canReclaim || progress?.settled != true) {
                    QrCard(output.qr, output.text, onCopy = { copyNotes(output.text) }, status = {
                        when {
                            canReclaim -> StatusPill("Waiting for recipient to redeem", Tone.Warning)
                            progress != null -> ProgressPill(progress)
                        }
                    })
                } else {
                    // Redeemed or reclaimed: the notes are spent, so their QR is no longer shown.
                    ProgressPill(progress, Modifier.align(Alignment.CenterHorizontally))
                }
                MutedText(
                    if (canReclaim) {
                        "If no one redeems these notes, you can reclaim them back to your balance. " +
                            "Whoever has them can redeem them, so send them only to the person you're paying."
                    } else {
                        state.progress?.detail.orEmpty()
                    },
                    Modifier.fillMaxWidth(),
                    TextAlign.Center,
                )
            }
            state.started -> SentStatus(state)
            review != null -> ReviewContent(review)
            else -> {
                AmountInput(amount, {
                    amount = it
                    vm.onAmountChanged()
                }, enabled = !state.working, modifier = Modifier.align(Alignment.CenterHorizontally))
                AvailableBalance(context.balance)
            }
        }
        SubmittedNotice(state, onOpenOperation, vm::checkSubmission)
        ErrorText(state.error)
    }

    if (confirmReclaim) {
        ReclaimDialog(
            onConfirm = {
                confirmReclaim = false
                vm.reclaim()
            },
            onDismiss = { confirmReclaim = false },
        )
    }
}

/** Reclaiming races the receiver: say so before asking. */
@Composable
fun ReclaimDialog(onConfirm: () -> Unit, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Reclaim these notes?") },
        text = {
            Text(
                "The federation takes them back into your balance. If the receiver redeems " +
                    "them first, they keep them and nothing comes back.",
            )
        },
        confirmButton = { TextButton(onClick = onConfirm) { Text("Reclaim") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

// ── Receive ──────────────────────────────────────────────────────────────

/**
 * Redeems notes someone handed over: parse and show their value first, then
 * redeem. The pasted text is held here, in memory only, never in saved state:
 * it is spendable by whoever holds it.
 *
 * Redeeming consumes the parsed notes, so after a failed redeem the screen
 * offers [check] again for the same text; it never redeems on its own.
 */
class EcashReceiveViewModel(
    private val source: Source,
    savedState: SavedStateHandle? = null,
    reconcile: (suspend (Long) -> Reconciled)? = null,
) : PaymentViewModel(savedState, reconcile) {
    /** The SDK calls, behind an interface so the check/redeem/retry rules are testable. */
    interface Source {
        suspend fun parse(text: String): Parsed
    }

    /** Parsed notes: their value, and the one-shot redeem of exactly these notes. */
    class Parsed(val value: Amount, val handle: AutoCloseable?, val redeem: suspend () -> Redemption)

    class Redemption(val id: OperationId, val progress: Flow<OpProgress>, val handle: AutoCloseable?)

    private val _notesText = MutableStateFlow("")
    val notesText = _notesText.asStateFlow()

    private var parsed: Parsed? = null

    fun onNotesChanged(text: String) {
        _notesText.value = text
        release(parsed?.handle)
        parsed = null
        clearReview()
    }

    /** Parses the current text into a review; also the retry after a failed redeem. */
    fun check() = step {
        val p = source.parse(_notesText.value)
        p.handle?.owned()
        release(parsed?.handle)
        parsed = p
        val value = sats(p.value)
        mutableState.update { it.copy(review = Review(listOf("Value" to value), value, expiresAtMillis = null)) }
    }

    fun redeem() = execute {
        val p = checkNotNull(parsed) { "Check the notes again." }
        parsed = null
        val redemption = p.redeem()
        redemption.handle?.owned()
        follow(redemption.progress) { it }
        redemption.id
    }

    companion object {
        fun create(payments: Payments, federationId: FederationId, savedState: SavedStateHandle, history: History) =
            EcashReceiveViewModel(
                object : Source {
                    override suspend fun parse(text: String): Parsed {
                        val notes = payments.parseNotes(text)
                        return Parsed(notes.value(), notes) {
                            val operation = payments.ecashReceive(federationId, notes)
                            Redemption(
                                operation.id(),
                                Payments.states(operation::updates) { it.next() }.map { it.progress() },
                                operation,
                            )
                        }
                    }
                },
                savedState,
                reconcileWith(history, federationId, OperationKind.ECASH_RECEIVE),
            )
    }
}

@Composable
fun EcashReceivePane(
    federationId: FederationId,
    chrome: PaneChrome,
    context: PaymentContextViewModel.UiState,
    onDone: () -> Unit,
    onOpenOperation: (OperationId) -> Unit,
) {
    val vm = appViewModel { EcashReceiveViewModel.create(it.payments, federationId, createSavedStateHandle(), it.history) }
    val state by vm.state.collectAsStateWithLifecycle()
    val notesText by vm.notesText.collectAsStateWithLifecycle()
    val editable = !state.started && !state.working
    val review = state.review
    SideEffect { chrome.set("Receive", busy = state.submitting) }
    LaunchedEffect(Unit) { ScanHandoff.take<Scanned.Notes>()?.let { vm.onNotesChanged(it.text) } }
    // Check the notes shortly after the text stops changing, so the preview follows what was pasted.
    LaunchedEffect(notesText) {
        if (notesText.isNotBlank() && vm.state.value.review == null && !vm.state.value.started) {
            delay(400)
            vm.check()
        }
    }

    PaymentPane(
        bottom = {
            when {
                state.started -> DoneButton(state, onDone)
                else -> WalletButton(
                    review?.let { "Redeem ${it.total}" } ?: "Redeem",
                    vm::redeem,
                    enabled = review != null,
                    working = state.working,
                )
            }
        },
    ) {
        if (state.started) {
            SentStatus(state, heading = "You redeemed")
        } else {
            Overline("Paste ecash notes")
            WalletTextField(
                notesText,
                vm::onNotesChanged,
                enabled = editable,
                placeholder = "Notes someone gave you",
                minLines = 3,
                maxLines = 4,
                mono = true,
                keyboardOptions = CodeKeyboard,
            )
            PasteScanRow(enabled = editable, onText = vm::onNotesChanged)
            // A failed check or redeem leaves the same text unchecked; offer the check again.
            if (review == null && !state.working && state.error != null && notesText.isNotBlank()) {
                WalletButton("Check notes again", vm::check, style = ButtonStyle.Secondary, icon = R.drawable.ic_refresh)
            }
            if (review != null) {
                Overline("Preview")
                DetailCard(
                    listOfNotNull(
                        "Amount" to review.total,
                        context.federation?.let { "Into" to (it.name ?: "this federation") },
                        "Notes" to "Readable · checked on redeem",
                    ),
                )
            }
        }
        SubmittedNotice(state, onOpenOperation, vm::checkSubmission)
        ErrorText(state.error)
    }
}
