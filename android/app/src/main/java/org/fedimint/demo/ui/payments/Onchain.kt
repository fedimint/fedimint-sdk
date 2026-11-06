package org.fedimint.demo.ui.payments

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
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.createSavedStateHandle
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.fedimint.demo.R
import org.fedimint.demo.ui.common.AmountInput
import org.fedimint.demo.ui.common.ButtonStyle
import org.fedimint.demo.ui.common.DetailCard
import org.fedimint.demo.ui.common.Loading
import org.fedimint.demo.ui.common.QrCard
import org.fedimint.demo.ui.common.ScanHandoff
import org.fedimint.demo.ui.common.Scanned
import org.fedimint.demo.ui.common.WalletButton
import org.fedimint.demo.ui.common.WalletTextField
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.formatSats
import org.fedimint.demo.ui.common.label
import org.fedimint.demo.ui.common.shareText
import org.fedimint.demo.ui.common.shortId
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.demo.wallet.History
import org.fedimint.demo.wallet.Payments
import org.fedimint.sdk.Amount
import org.fedimint.sdk.FederationId
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
 *
 * One subscription at a time, and one deposit followed at a time: a newer
 * deposit replaces the one on screen, and the older one's updates stop
 * writing to it, so one deposit's "Received" never shows under another.
 */
class OnchainReceiveViewModel(private val source: Source) : PaymentViewModel() {
    /** The SDK calls, behind an interface so the one-at-a-time rules are testable. */
    interface Source {
        suspend fun deposits(): Deposits

        suspend fun address(): String
    }

    /** A deposits subscription; closing it also releases the deposits it handed out. */
    interface Deposits : AutoCloseable {
        /** The next confirmed deposit. The subscription ends by throwing (e.g. the federation closed). */
        suspend fun next(): Deposit
    }

    class Deposit(val id: OperationId, val progress: Flow<OpProgress>)

    private var deposits: Deposits? = null
    private var watching: Job? = null
    private var following: Job? = null

    init {
        newAddress()
    }

    /**
     * A fresh address and subscription. The old subscription closes first, so
     * if this fails the old address is taken off screen too (nothing watches
     * it any more), leaving the pane's Try again.
     */
    fun newAddress() = step(onFailure = { mutableState.update { it.copy(output = null, progress = null, operationId = null, steps = emptyList()) } }) {
        stopWatching()
        val opened = source.deposits()
        deposits = opened
        val address = source.address()
        mutableState.update {
            it.copy(
                output = Output("Deposit address", address, qr = "bitcoin:$address"),
                operationId = null,
                steps = emptyList(),
                progress = OpProgress(
                    "Waiting for a deposit",
                    "Send bitcoin to the address above. It shows here once it has its confirmations.",
                ),
            )
        }
        watch(opened)
    }

    private fun watch(opened: Deposits) {
        watching = viewModelScope.launch {
            while (true) {
                val deposit = attempt { opened.next() }
                    .onFailure { e -> mutableState.update { it.copy(error = userMessage(e)) } }
                    .getOrNull() ?: break // ended: the error is on screen
                following?.cancel()
                // Everything about the previous deposit goes at once: until this one's
                // first state arrives (or if it never does), it reads as pending, never
                // as the last deposit's outcome.
                mutableState.update {
                    it.copy(
                        operationId = deposit.id,
                        progress = OpProgress("Deposit found", "Checking its status…"),
                        steps = emptyList(),
                        followFailed = false,
                        error = null,
                    )
                }
                following = follow(deposit.progress) { it }
            }
        }
    }

    private fun stopWatching() {
        following?.cancel()
        watching?.cancel()
        deposits?.let { runCatching { it.close() } }
        deposits = null
    }

    override fun onCleared() {
        stopWatching()
        super.onCleared()
    }

    companion object {
        fun from(payments: Payments, federationId: FederationId) = object : Source {
            override suspend fun deposits(): Deposits {
                val subscription = payments.onchainDeposits(federationId)
                return object : Deposits {
                    private val operations = mutableListOf<AutoCloseable>()

                    override suspend fun next(): Deposit {
                        val operation = subscription.next()
                        synchronized(operations) { operations += operation }
                        return Deposit(operation.id(), Payments.states(operation::updates) { it.next() }.map { it.progress() })
                    }

                    override fun close() {
                        synchronized(operations) { operations.forEach { runCatching { it.close() } } }
                        subscription.close()
                    }
                }
            }

            override suspend fun address(): String = payments.onchainReceive(federationId)
        }
    }
}

@Composable
fun OnchainReceivePane(
    federationId: FederationId,
    chrome: PaneChrome,
    context: PaymentContextViewModel.UiState,
    onDone: () -> Unit,
    onOpenOperation: (OperationId) -> Unit,
) {
    val vm = appViewModel { OnchainReceiveViewModel(OnchainReceiveViewModel.from(it.payments, federationId)) }
    val state by vm.state.collectAsStateWithLifecycle()
    val android = LocalContext.current
    val copy = rememberCopy()
    SideEffect { chrome.set("Receive", busy = state.submitting) }

    PaymentPane(
        bottom = {
            val output = state.output
            if (output != null) {
                WalletButton("Share address", { shareText(android, output.text, "Share address") }, icon = R.drawable.ic_share)
            }
            WalletButton(
                if (output == null) "Try again" else "New address",
                vm::newAddress,
                style = if (output == null) ButtonStyle.Primary else ButtonStyle.Secondary,
                working = state.working,
                icon = R.drawable.ic_refresh,
            )
        },
    ) {
        val output = state.output
        if (output == null) {
            if (state.working) Loading()
        } else {
            QrCard(output.qr, output.text, onCopy = { copy(output.text) })
            DetailCard(
                listOfNotNull(
                    context.federation?.let { "Network" to it.network.label() },
                    "Confirmations" to "As the federation requires",
                    "Deposit fee" to "Shown when it arrives",
                ),
            )
            state.progress?.let { ProgressCard(it, state.steps) }
        }
        SubmittedNotice(state, onOpenOperation, vm::checkSubmission)
        ErrorText(state.error)
        if (state.progress?.settled == true) WalletButton("Done", onDone, style = ButtonStyle.Secondary)
    }
}

// ── Send ─────────────────────────────────────────────────────────────────

class OnchainSendViewModel(
    private val payments: Payments,
    private val federationId: FederationId,
    savedState: SavedStateHandle,
    history: History,
) : PaymentViewModel(savedState, reconcileWith(history, federationId, OperationKind.ONCHAIN_SEND)) {

    private val quote = SingleUseQuote<OnchainQuote>()

    fun onInputChanged() {
        quote.drop()
        clearReview()
    }

    fun quote(address: String, sats: ULong) = step {
        val q = quote.hold(payments.onchainQuote(federationId, address, sats))
        val review = Review(
            rows = listOf(
                "To" to shortId(address, keep = 10),
                "Via" to "Onchain",
                "Fees" to sats(q.fee()),
            ),
            total = sats(q.total()),
            expiresAtMillis = expiry(q.expiresAt()),
            amount = formatSats(q.amount() * 1_000uL),
        )
        mutableState.update { it.copy(review = review) }
    }

    fun send() = execute {
        // Single use, and spent even by a failed send: taken, so nothing can submit it
        // twice. A failure drops the review (execute).
        val q = quote.take()
        val operation = payments.onchainSend(federationId, q).owned()
        follow(Payments.states(operation::updates) { it.next() }) { it.progress() }
        operation.id()
    }
}

@Composable
fun OnchainSendPane(
    federationId: FederationId,
    chrome: PaneChrome,
    balance: Amount?,
    onDone: () -> Unit,
    onOpenOperation: (OperationId) -> Unit,
) {
    val vm = appViewModel { OnchainSendViewModel(it.payments, federationId, createSavedStateHandle(), it.history) }
    val state by vm.state.collectAsStateWithLifecycle()
    var address by rememberSaveable { mutableStateOf("") }
    var amount by rememberSaveable { mutableStateOf("") }
    val sats = amount.toULongOrNull()
    val editable = !state.started && !state.working
    val review = state.review
    val onAddress: (String) -> Unit = {
        address = it
        vm.onInputChanged()
    }
    LaunchedEffect(Unit) { ScanHandoff.take<Scanned.Address>()?.let { onAddress(it.text) } }
    SideEffect {
        if (review != null && !state.started) {
            chrome.set("Review payment", busy = state.submitting, showTabs = false, back = vm::onInputChanged)
        } else {
            chrome.set("Send", busy = state.submitting)
        }
    }

    PaymentPane(
        bottom = {
            when {
                state.started -> DoneButton(state, onDone)
                review == null -> WalletButton(
                    "Review quote",
                    { vm.quote(address.trim(), sats ?: 0uL) },
                    enabled = address.isNotBlank() && sats != null && sats > 0uL,
                    working = state.working,
                )
                else -> {
                    WalletButton("Approve & send", vm::send, working = state.working)
                    WalletButton("Cancel", vm::onInputChanged, style = ButtonStyle.Secondary, enabled = !state.working)
                }
            }
        },
    ) {
        when {
            state.started -> SentStatus(state)
            review != null -> ReviewContent(review)
            else -> {
                WalletTextField(
                    address,
                    onAddress,
                    label = "Bitcoin address",
                    placeholder = "bc1…",
                    enabled = editable,
                    mono = true,
                    keyboardOptions = CodeKeyboard,
                )
                // Only an address goes into the field: it is saved state, and pasted ecash
                // notes must never be (SECURITY.md, rule 2).
                PasteScanRow(enabled = editable) { text ->
                    when (val code = Scanned.classify(text)) {
                        is Scanned.Address -> onAddress(code.text)
                        else -> vm.rejectInput("That isn't a bitcoin address.")
                    }
                }
                AmountInput(amount, {
                    amount = it
                    vm.onInputChanged()
                }, enabled = editable, modifier = Modifier.align(Alignment.CenterHorizontally))
                AvailableBalance(balance)
            }
        }
        SubmittedNotice(state, onOpenOperation, vm::checkSubmission)
        ErrorText(state.error)
    }
}
