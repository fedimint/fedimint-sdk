package org.fedimint.demo.ui.payments

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.wallet.Payments
import org.fedimint.sdk.EcashQuote
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.Notes

// ── Send ─────────────────────────────────────────────────────────────────

class EcashSendViewModel(private val payments: Payments, private val federationId: FederationId) :
    PaymentViewModel() {

    private var quote: EcashQuote? = null

    fun onAmountChanged() {
        quote = null
        clearReview()
    }

    fun quote(sats: ULong) = step {
        val q = payments.ecashQuote(federationId, sats * 1_000uL).owned()
        quote = q
        val rows = buildList {
            add("Amount" to sats(q.requestedAmount()))
            // Notes come in fixed denominations, so the value handed over can exceed what was asked for.
            if (q.notesValue() != q.requestedAmount()) add("Notes value" to sats(q.notesValue()))
            add("Fee" to sats(q.fee()))
        }
        mutableState.update { it.copy(review = Review(rows, sats(q.total()), expiry(q.expiresAt()))) }
    }

    fun send() = step {
        val q = quote ?: return@step
        quote = null
        val handle = payments.ecashSend(federationId, q)
        val notes = handle.notes.owned()
        val operation = handle.operation.owned()
        // `display()` is the deliberate way to take notes out as text; the
        // handle never prints them. They are a bearer instrument, like cash.
        mutableState.update { it.copy(review = null, output = Output("Ecash notes", notes.display())) }
        follow(Payments.states(operation::updates) { it.next() }) { it.progress() }
    }
}

@Composable
fun EcashSendScreen(federationId: FederationId, onBack: () -> Unit) {
    val vm = appViewModel { EcashSendViewModel(it.payments, federationId) }
    val state by vm.state.collectAsStateWithLifecycle()
    var amount by rememberSaveable { mutableStateOf("") }
    val sats = amount.toULongOrNull()

    PaymentScreen("Send ecash", onBack) {
        SatsField(amount, {
            amount = it
            vm.onAmountChanged()
        }, enabled = !state.started && !state.working)
        val review = state.review
        when {
            state.started -> Unit
            review == null -> PrimaryButton("Review", { vm.quote(sats ?: 0uL) }, sats != null && sats > 0uL, state.working)
            else -> {
                ReviewCard(review)
                PrimaryButton("Create notes for ${review.total}", vm::send, enabled = true, working = state.working)
            }
        }
        state.output?.let {
            OutputCard(it)
            Text(
                "Whoever has these notes can redeem them. Send them only to the person you're paying.",
                style = MaterialTheme.typography.bodySmall,
            )
        }
        state.progress?.let { ProgressCard(it) }
        ErrorText(state.error)
        DoneButton(state, onBack)
    }
}

// ── Receive ──────────────────────────────────────────────────────────────

/**
 * Redeems notes someone handed over: parse and show their value first, then
 * redeem. The pasted text is held here, in memory only, never in saved state:
 * it is spendable by whoever holds it.
 */
class EcashReceiveViewModel(private val payments: Payments, private val federationId: FederationId) :
    PaymentViewModel() {

    private val _notesText = MutableStateFlow("")
    val notesText = _notesText.asStateFlow()

    private var notes: Notes? = null

    fun onNotesChanged(text: String) {
        _notesText.value = text
        notes = null
        clearReview()
    }

    fun check() = step {
        val parsed = payments.parseNotes(_notesText.value).owned()
        notes = parsed
        val value = sats(parsed.value())
        mutableState.update { it.copy(review = Review(listOf("Value" to value), value, expiresAtMillis = null)) }
    }

    fun redeem() = step {
        val n = notes ?: return@step
        notes = null
        val operation = payments.ecashReceive(federationId, n).owned()
        mutableState.update { it.copy(review = null) }
        follow(Payments.states(operation::updates) { it.next() }) { it.progress() }
    }
}

@Composable
fun EcashReceiveScreen(federationId: FederationId, onBack: () -> Unit) {
    val vm = appViewModel { EcashReceiveViewModel(it.payments, federationId) }
    val state by vm.state.collectAsStateWithLifecycle()
    val notesText by vm.notesText.collectAsStateWithLifecycle()

    PaymentScreen("Redeem ecash", onBack) {
        TextInput(notesText, vm::onNotesChanged, "Ecash notes", enabled = !state.started && !state.working, minLines = 4)
        val review = state.review
        when {
            state.started -> Unit
            review == null -> PrimaryButton("Check notes", vm::check, notesText.isNotBlank(), state.working)
            else -> {
                ReviewCard(review)
                PrimaryButton("Redeem ${review.total}", vm::redeem, enabled = true, working = state.working)
            }
        }
        state.progress?.let { ProgressCard(it) }
        ErrorText(state.error)
        DoneButton(state, onBack)
    }
}
