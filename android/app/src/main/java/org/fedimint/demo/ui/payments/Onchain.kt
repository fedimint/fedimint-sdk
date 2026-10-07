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
import org.fedimint.demo.wallet.Payments
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.OnchainQuote
import org.fedimint.sdk.OperationId

// ── Receive ──────────────────────────────────────────────────────────────

/** A deposit address is created as the screen opens: each one is its own operation to follow. */
class OnchainReceiveViewModel(
    private val payments: Payments,
    private val federationId: FederationId,
    savedState: SavedStateHandle,
) : PaymentViewModel(savedState) {

    init {
        // A screen restored after process death already has its operation; don't open another.
        if (state.value.operationId == null) newAddress()
    }

    fun newAddress() = execute {
        val handle = payments.onchainReceive(federationId)
        val operation = handle.operation.owned()
        mutableState.update {
            it.copy(output = Output("Deposit address", handle.address, qr = "bitcoin:${handle.address}"))
        }
        follow(Payments.states(operation::updates) { it.next() }) { it.progress() }
        operation.id()
    }
}

@Composable
fun OnchainReceiveScreen(federationId: FederationId, onBack: () -> Unit, onOpenOperation: (OperationId) -> Unit) {
    val vm = appViewModel { OnchainReceiveViewModel(it.payments, federationId, createSavedStateHandle()) }
    val state by vm.state.collectAsStateWithLifecycle()

    PaymentScreen("Deposit bitcoin", onBack) {
        state.output?.let { OutputCard(it) }
        state.progress?.let { ProgressCard(it) }
        SubmittedNotice(state, onOpenOperation)
        ErrorText(state.error)
        if (!state.started && !state.working) PrimaryButton("Try again", vm::newAddress, enabled = true, working = false)
        DoneButton(state, onBack)
    }
}

// ── Send ─────────────────────────────────────────────────────────────────

class OnchainSendViewModel(
    private val payments: Payments,
    private val federationId: FederationId,
    savedState: SavedStateHandle,
) : PaymentViewModel(savedState) {

    private var quote: OnchainQuote? = null

    fun onInputChanged() {
        quote = null
        clearReview()
    }

    fun quote(address: String, sats: ULong) = step {
        val q = payments.onchainQuote(federationId, address, sats).owned()
        quote = q
        val review = Review(
            rows = listOf(
                "Amount" to "${formatSats(q.amount() * 1_000uL)} sats",
                "Network and federation fees" to sats(q.fee()),
            ),
            total = sats(q.total()),
            expiresAtMillis = expiry(q.expiresAt()),
        )
        mutableState.update { it.copy(review = review) }
    }

    fun send() = execute {
        val q = checkNotNull(quote) { "That quote was already used. Review the send again." }
        quote = null
        val operation = payments.onchainSend(federationId, q).owned()
        follow(Payments.states(operation::updates) { it.next() }) { it.progress() }
        operation.id()
    }
}

@Composable
fun OnchainSendScreen(federationId: FederationId, onBack: () -> Unit, onOpenOperation: (OperationId) -> Unit) {
    val vm = appViewModel { OnchainSendViewModel(it.payments, federationId, createSavedStateHandle()) }
    val state by vm.state.collectAsStateWithLifecycle()
    var address by rememberSaveable { mutableStateOf("") }
    var amount by rememberSaveable { mutableStateOf("") }
    val sats = amount.toULongOrNull()
    val editable = !state.started && !state.working

    PaymentScreen("Send on-chain", onBack) {
        TextInput(address, {
            address = it
            vm.onInputChanged()
        }, "Bitcoin address", enabled = editable)
        SatsField(amount, {
            amount = it
            vm.onInputChanged()
        }, enabled = editable)
        val review = state.review
        when {
            state.started -> Unit
            review == null -> PrimaryButton(
                "Review",
                { vm.quote(address, sats ?: 0uL) },
                enabled = address.isNotBlank() && sats != null && sats > 0uL,
                working = state.working,
            )
            else -> {
                ReviewCard(review)
                PrimaryButton("Send ${review.total}", vm::send, enabled = true, working = state.working)
            }
        }
        state.progress?.let { ProgressCard(it) }
        SubmittedNotice(state, onOpenOperation)
        ErrorText(state.error)
        DoneButton(state, onBack)
    }
}
