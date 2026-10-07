package org.fedimint.demo.ui.payments

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.createSavedStateHandle
import kotlinx.coroutines.flow.update
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.formatSats
import org.fedimint.demo.ui.common.shortId
import org.fedimint.demo.wallet.Payments
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.LnQuote
import org.fedimint.sdk.OperationId

// ── Receive ──────────────────────────────────────────────────────────────

class LightningReceiveViewModel(
    private val payments: Payments,
    private val federationId: FederationId,
    savedState: SavedStateHandle,
) : PaymentViewModel(savedState) {

    fun create(sats: ULong, description: String) = execute {
        val handle = payments.lightningReceive(federationId, sats * 1_000uL, description)
        val operation = handle.operation.owned()
        // Upper case packs into a denser QR code; BOLT11 is case-insensitive.
        mutableState.update { it.copy(output = Output("Lightning invoice", handle.invoice, handle.invoice.uppercase())) }
        follow(Payments.states(operation::updates) { it.next() }) { it.progress() }
        operation.id()
    }
}

@Composable
fun LightningReceiveScreen(federationId: FederationId, onBack: () -> Unit, onOpenOperation: (OperationId) -> Unit) {
    val vm = appViewModel { LightningReceiveViewModel(it.payments, federationId, createSavedStateHandle()) }
    val state by vm.state.collectAsStateWithLifecycle()
    var amount by rememberSaveable { mutableStateOf("") }
    var description by rememberSaveable { mutableStateOf("") }
    val sats = amount.toULongOrNull()

    PaymentScreen("Receive with Lightning", onBack) {
        SatsField(amount, { amount = it }, enabled = !state.started)
        TextInput(description, { description = it }, "Description (optional)", enabled = !state.started)
        if (!state.started) {
            PrimaryButton(
                "Create invoice",
                onClick = { vm.create(sats ?: 0uL, description) },
                enabled = sats != null && sats > 0uL,
                working = state.working,
            )
        }
        state.output?.let { OutputCard(it) }
        state.progress?.let { ProgressCard(it) }
        SubmittedNotice(state, onOpenOperation)
        ErrorText(state.error)
        DoneButton(state, onBack)
    }
}

// ── Send ─────────────────────────────────────────────────────────────────

class LightningSendViewModel(
    private val payments: Payments,
    private val federationId: FederationId,
    savedState: SavedStateHandle,
) : PaymentViewModel(savedState) {

    private var quote: LnQuote? = null

    /** A new invoice invalidates the quote made for the old one. */
    fun onInvoiceChanged() {
        quote = null
        clearReview()
    }

    fun quote(invoice: String) = step {
        val q = payments.lightningQuote(federationId, invoice).owned()
        quote = q
        val review = Review(
            rows = listOfNotNull(
                "Invoice amount" to sats(q.invoiceAmount()),
                "Fee" to sats(q.fee()),
                "Route" to q.route().label(),
                q.route().gatewayId()?.let { "Gateway" to shortId(it) },
            ),
            total = sats(q.total()),
            expiresAtMillis = expiry(q.expiresAt()),
        )
        mutableState.update { it.copy(review = review) }
    }

    fun pay() = execute {
        // A quote is single use, and spent even by a failed send: take it, so
        // nothing can submit it twice. A failure drops the review (execute).
        val q = checkNotNull(quote) { "That quote was already used. Review the payment again." }
        quote = null
        val operation = payments.lightningSend(federationId, q).owned()
        follow(Payments.states(operation::updates) { it.next() }) { it.progress() }
        operation.id()
    }
}

@Composable
fun LightningSendScreen(federationId: FederationId, onBack: () -> Unit, onOpenOperation: (OperationId) -> Unit) {
    val vm = appViewModel { LightningSendViewModel(it.payments, federationId, createSavedStateHandle()) }
    val state by vm.state.collectAsStateWithLifecycle()
    var invoice by rememberSaveable { mutableStateOf("") }

    PaymentScreen("Pay a Lightning invoice", onBack) {
        TextInput(
            invoice,
            {
                invoice = it
                vm.onInvoiceChanged()
            },
            "Lightning invoice (lnbc…)",
            enabled = !state.started && !state.working,
            minLines = 3,
        )
        val review = state.review
        when {
            state.started -> Unit
            review == null -> PrimaryButton("Review payment", { vm.quote(invoice) }, invoice.isNotBlank(), state.working)
            else -> {
                ReviewCard(review)
                PrimaryButton("Pay ${review.total}", vm::pay, enabled = true, working = state.working)
            }
        }
        state.progress?.let { ProgressCard(it) }
        SubmittedNotice(state, onOpenOperation)
        ErrorText(state.error)
        DoneButton(state, onBack)
    }
}

internal fun sats(msats: ULong): String = "${formatSats(msats)} sats"
