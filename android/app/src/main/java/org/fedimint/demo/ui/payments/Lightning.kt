package org.fedimint.demo.ui.payments

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.createSavedStateHandle
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import org.fedimint.demo.R
import org.fedimint.demo.ui.common.AmountHero
import org.fedimint.demo.ui.common.AmountInput
import org.fedimint.demo.ui.common.ButtonStyle
import org.fedimint.demo.ui.common.MutedText
import org.fedimint.demo.ui.common.Overline
import org.fedimint.demo.ui.common.QrCard
import org.fedimint.demo.ui.common.ScanHandoff
import org.fedimint.demo.ui.common.Scanned
import org.fedimint.demo.ui.common.WalletButton
import org.fedimint.demo.ui.common.WalletTextField
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.formatSats
import org.fedimint.demo.ui.common.shareText
import org.fedimint.demo.ui.common.shortId
import org.fedimint.demo.wallet.History
import org.fedimint.demo.wallet.Payments
import org.fedimint.sdk.Amount
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.LnQuote
import org.fedimint.sdk.OperationId
import org.fedimint.sdk.OperationKind

// ── Receive ──────────────────────────────────────────────────────────────

class LightningReceiveViewModel(
    private val payments: Payments,
    private val federationId: FederationId,
    savedState: SavedStateHandle,
    history: History,
) : PaymentViewModel(savedState, reconcileWith(history, federationId, OperationKind.LN_RECEIVE)) {

    /** What the invoice on screen asks for, as it was created: shown with it, whatever the fields say now. */
    data class Request(val sats: ULong, val description: String)

    private val _request = MutableStateFlow<Request?>(null)
    val request = _request.asStateFlow()

    fun create(sats: ULong, description: String) = execute {
        val handle = payments.lightningReceive(federationId, sats * 1_000uL, description)
        _request.value = Request(sats, description)
        val operation = handle.operation.owned()
        // Upper case packs into a denser QR code; BOLT11 is case-insensitive.
        mutableState.update { it.copy(output = Output("Lightning invoice", handle.invoice, handle.invoice.uppercase())) }
        follow(Payments.states(operation::updates) { it.next() }) { it.progress() }
        operation.id()
    }
}

@Composable
fun LightningReceivePane(
    federationId: FederationId,
    chrome: PaneChrome,
    onDone: () -> Unit,
    onOpenOperation: (OperationId) -> Unit,
) {
    val vm = appViewModel { LightningReceiveViewModel(it.payments, federationId, createSavedStateHandle(), it.history) }
    val state by vm.state.collectAsStateWithLifecycle()
    val request by vm.request.collectAsStateWithLifecycle()
    var amount by rememberSaveable { mutableStateOf("") }
    var description by rememberSaveable { mutableStateOf("") }
    val sats = amount.toULongOrNull()
    val context = LocalContext.current
    val copy = rememberCopy()
    // Locked while the invoice is being created too, so the fields can't drift from the request.
    val editable = !state.started && !state.working
    SideEffect { chrome.set("Receive", busy = state.submitting) }

    PaymentPane(
        bottom = {
            val output = state.output
            when {
                !state.started -> WalletButton(
                    "Create invoice",
                    onClick = { vm.create(sats ?: 0uL, description) },
                    enabled = sats != null && sats > 0uL,
                    working = state.working,
                )
                state.progress?.settled == true -> DoneButton(state, onDone)
                output != null -> WalletButton(
                    "Share invoice",
                    onClick = { shareText(context, output.text, "Share invoice") },
                    icon = R.drawable.ic_share,
                )
            }
        },
    ) {
        val output = state.output
        if (output == null) {
            AmountInput(amount, { amount = it }, enabled = editable, modifier = Modifier.align(Alignment.CenterHorizontally))
            WalletTextField(description, { description = it }, label = "Note (optional)", enabled = editable)
        } else {
            request?.let { r ->
                Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                    AmountHero(formatSats(r.sats * 1_000uL))
                    if (r.description.isNotBlank()) MutedText(r.description)
                }
            }
            QrCard(output.qr, output.text, onCopy = { copy(output.text) }, status = { state.progress?.let { ProgressPill(it) } })
            state.progress?.detail?.let { MutedText(it, Modifier.fillMaxWidth(), TextAlign.Center) }
        }
        SubmittedNotice(state, onOpenOperation, vm::checkSubmission)
        ErrorText(state.error)
    }
}

// ── Send ─────────────────────────────────────────────────────────────────

class LightningSendViewModel(
    private val payments: Payments,
    private val federationId: FederationId,
    savedState: SavedStateHandle,
    history: History,
) : PaymentViewModel(savedState, reconcileWith(history, federationId, OperationKind.LN_SEND)) {

    private val quote = SingleUseQuote<LnQuote>()

    /** A new invoice invalidates the quote made for the old one. */
    fun onInvoiceChanged() {
        quote.drop()
        clearReview()
    }

    fun quote(invoice: String) = step {
        val q = quote.hold(payments.lightningQuote(federationId, invoice))
        val review = Review(
            rows = listOfNotNull(
                "To" to shortId(invoice.trim(), keep = 10),
                "Via" to q.route().label(),
                q.route().gatewayId()?.let { "Gateway" to shortId(it) },
                "Fee" to sats(q.fee()),
            ),
            total = sats(q.total()),
            expiresAtMillis = expiry(q.expiresAt()),
            amount = formatSats(q.invoiceAmount()),
        )
        mutableState.update { it.copy(review = review) }
    }

    fun pay() = execute {
        // Single use, and spent even by a failed send: taken, so nothing can submit it
        // twice. A failure drops the review (execute).
        val q = quote.take()
        val operation = payments.lightningSend(federationId, q).owned()
        follow(Payments.states(operation::updates) { it.next() }) { it.progress() }
        operation.id()
    }
}

@Composable
fun LightningSendPane(
    federationId: FederationId,
    chrome: PaneChrome,
    balance: Amount?,
    onDone: () -> Unit,
    onOpenOperation: (OperationId) -> Unit,
) {
    val vm = appViewModel { LightningSendViewModel(it.payments, federationId, createSavedStateHandle(), it.history) }
    val state by vm.state.collectAsStateWithLifecycle()
    var invoice by rememberSaveable { mutableStateOf("") }
    val editable = !state.started && !state.working
    val review = state.review
    val onInvoice: (String) -> Unit = {
        invoice = it
        vm.onInvoiceChanged()
    }
    LaunchedEffect(Unit) { ScanHandoff.take<Scanned.Invoice>()?.let { onInvoice(it.text) } }
    SideEffect {
        if (review != null && !state.started) {
            chrome.set("Review payment", busy = state.submitting, showTabs = false, back = vm::onInvoiceChanged)
        } else {
            chrome.set("Send", busy = state.submitting)
        }
    }

    PaymentPane(
        bottom = {
            when {
                state.started -> DoneButton(state, onDone)
                review == null -> WalletButton("Review quote", { vm.quote(invoice) }, enabled = invoice.isNotBlank(), working = state.working)
                else -> {
                    WalletButton("Approve & send", vm::pay, working = state.working)
                    WalletButton("Cancel", vm::onInvoiceChanged, style = ButtonStyle.Secondary, enabled = !state.working)
                }
            }
        },
    ) {
        when {
            state.started -> SentStatus(state)
            review != null -> ReviewContent(review)
            else -> {
                WalletTextField(
                    invoice,
                    onInvoice,
                    label = "Lightning invoice",
                    placeholder = "lnbc…",
                    enabled = editable,
                    minLines = 2,
                    maxLines = 3,
                    mono = true,
                    keyboardOptions = CodeKeyboard,
                )
                // Only an invoice goes into the field: it is saved state, and pasted ecash
                // notes must never be (SECURITY.md, rule 2).
                PasteScanRow(enabled = editable) { text ->
                    when (val code = Scanned.classify(text)) {
                        is Scanned.Invoice -> onInvoice(code.text)
                        else -> vm.rejectInput("That isn't a Lightning invoice.")
                    }
                }
                MutedText("The amount comes from the invoice and shows on the quote.", Modifier.fillMaxWidth(), TextAlign.Center)
                AvailableBalance(balance)
            }
        }
        SubmittedNotice(state, onOpenOperation, vm::checkSubmission)
        ErrorText(state.error)
    }
}

/** A send in flight or done: the amount, its state and the steps seen. */
@Composable
internal fun SentStatus(state: PaymentViewModel.UiState, heading: String = "You sent") {
    state.sent?.let { sent ->
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Overline(heading)
            AmountHero(sent.amount ?: sent.total.removeSuffix(" sats"))
        }
    }
    state.progress?.let { ProgressCard(it, state.steps) }
}

/** Invoices, addresses and codes: no autocorrect, no capitalisation. */
internal val CodeKeyboard = KeyboardOptions(autoCorrectEnabled = false, keyboardType = KeyboardType.Uri)

internal fun sats(msats: ULong): String = "${formatSats(msats)} sats"
