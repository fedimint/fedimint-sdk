package org.fedimint.demo.ui.payments

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.createSavedStateHandle
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.formatSats
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.demo.wallet.History
import org.fedimint.demo.wallet.Payments
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.OnchainDeposits
import org.fedimint.sdk.OnchainQuote
import org.fedimint.sdk.OperationId
import org.fedimint.sdk.OperationKind

// ── Receive ──────────────────────────────────────────────────────────────

/**
 * A deposit address, created as the screen opens, then each deposit to it as
 * the federation finds it.
 *
 * An address isn't an operation (nobody may ever pay it), so handing one out
 * is a plain step, not a submission: making another is harmless, and there is
 * nothing to restore after process death. A deposit becomes an operation only
 * once it is confirmed; the deposits subscription hands it over then, and is
 * opened before the address is shown so one found straight away isn't missed.
 */
class OnchainReceiveViewModel(
    private val payments: Payments,
    private val federationId: FederationId,
) : PaymentViewModel() {

    init {
        newAddress()
    }

    fun newAddress() = step {
        val deposits = payments.onchainDeposits(federationId).owned()
        val address = payments.onchainReceive(federationId)
        mutableState.update {
            it.copy(
                output = Output("Deposit address", address, qr = "bitcoin:$address"),
                progress = OpProgress(
                    "Waiting for a deposit",
                    "Send bitcoin to the address above. It shows here once it has its confirmations.",
                ),
            )
        }
        watch(deposits)
    }

    /** Follows each deposit the federation finds; a newer one replaces the one shown. */
    private fun watch(deposits: OnchainDeposits) {
        viewModelScope.launch {
            while (true) {
                val operation = attempt { deposits.next() }
                    .onFailure { e -> mutableState.update { it.copy(error = userMessage(e)) } }
                    .getOrNull()?.owned() ?: break
                mutableState.update { it.copy(operationId = operation.id()) }
                follow(Payments.states(operation::updates) { it.next() }) { it.progress() }
            }
        }
    }
}

@Composable
fun OnchainReceiveScreen(federationId: FederationId, onBack: () -> Unit, onOpenOperation: (OperationId) -> Unit) {
    val vm = appViewModel { OnchainReceiveViewModel(it.payments, federationId) }
    val state by vm.state.collectAsStateWithLifecycle()

    PaymentScreen("Deposit bitcoin", onBack) {
        state.output?.let { OutputCard(it) }
        state.progress?.let { ProgressCard(it) }
        SubmittedNotice(state, onOpenOperation, vm::checkSubmission)
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
    history: History,
) : PaymentViewModel(savedState, reconcileWith(history, federationId, OperationKind.ONCHAIN_SEND)) {

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
    val vm = appViewModel { OnchainSendViewModel(it.payments, federationId, createSavedStateHandle(), it.history) }
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
        SubmittedNotice(state, onOpenOperation, vm::checkSubmission)
        ErrorText(state.error)
        DoneButton(state, onBack)
    }
}
