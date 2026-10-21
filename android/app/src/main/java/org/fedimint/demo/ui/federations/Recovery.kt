package org.fedimint.demo.ui.federations

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.fedimint.demo.ui.common.ButtonStyle
import org.fedimint.demo.ui.common.CardDivider
import org.fedimint.demo.ui.common.DetailRow
import org.fedimint.demo.ui.common.Loading
import org.fedimint.demo.ui.common.MutedText
import org.fedimint.demo.ui.common.Overline
import org.fedimint.demo.ui.common.ProgressBar
import org.fedimint.demo.ui.common.WalletButton
import org.fedimint.demo.ui.common.WalletCard
import org.fedimint.demo.ui.common.WalletScreen
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.formatSats
import org.fedimint.demo.ui.payments.ErrorText
import org.fedimint.demo.ui.payments.PaymentViewModel
import org.fedimint.demo.ui.payments.progress
import org.fedimint.demo.ui.theme.WalletText
import org.fedimint.demo.ui.theme.walletColors
import org.fedimint.demo.wallet.History
import org.fedimint.demo.wallet.Payments
import org.fedimint.demo.wallet.WalletSession
import org.fedimint.sdk.Amount
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.OperationKind
import org.fedimint.sdk.RecoveryState
import org.fedimint.sdk.Timestamp

/**
 * A federation's recovery: rebuilding this seed's wallet from the federation's
 * history after a restore. Until it is Done the federation is locked: its
 * balance is shown and keeps growing as funds are found, but every send and
 * receive is refused.
 *
 * Progress comes from the SDK: `Running` carries how far the rescan has got
 * (`complete` of `total`), shown as a determinate bar. The elapsed time, from
 * the recovery operation's start in the activity history, sits beside it,
 * because a long history can take hours. The balance is shown too, but it is
 * incomplete until Done, which the SDK reaches once the wallet holds what the
 * scan found. `resumeRecovery` is called automatically only for a
 * running recovery, where it merely reattaches; on a failed one it starts a
 * new attempt, so that is left to the user's Retry.
 */
class RecoveryViewModel(
    private val session: WalletSession,
    private val history: History,
    private val federationId: FederationId,
) :
    PaymentViewModel() {

    /** Null until known; [NeverRecovered] if the federation was joined plainly. */
    private val _status = MutableStateFlow<Status?>(null)
    val status = _status.asStateFlow()

    private val _recovered = MutableStateFlow<Amount?>(null)
    val recovered = _recovered.asStateFlow()

    /** When the current recovery attempt started, from the activity history. */
    private val _startedAt = MutableStateFlow<Timestamp?>(null)
    val startedAt = _startedAt.asStateFlow()

    sealed interface Status {
        data object NeverRecovered : Status
        data object Failed : Status
        data object Following : Status
    }

    init {
        load()
        viewModelScope.launch {
            session.balance(federationId).catch { }.collect { _recovered.value = it }
        }
    }

    /** Reads the recovery's state; also the retry when that read failed and the status is still unknown. */
    fun load() = step {
        when (session.recoveryStatus(federationId)) {
            null -> _status.value = Status.NeverRecovered
            is RecoveryState.Failed -> {
                _status.value = Status.Failed
                mutableState.update { it.copy(progress = RecoveryState.Failed("The last attempt stopped.").progress()) }
            }
            is RecoveryState.Running, RecoveryState.Done -> reattach()
        }
    }

    /** Starts a new attempt after a failed one. The stopped attempt stays in the history. */
    fun retry() = step { reattach() }

    private suspend fun reattach() {
        val operation = session.resumeRecovery(federationId).owned()
        _status.value = Status.Following
        // The newest recovery row is the current attempt (a retry adds a new one).
        _startedAt.value = attempt { history.page(federationId).items }
            .getOrNull()
            ?.firstOrNull { it.kind == OperationKind.RECOVERY }
            ?.time
        follow(Payments.states(operation::updates) { it.next() }) { state ->
            if (state is RecoveryState.Failed) _status.value = Status.Failed
            state.progress()
        }
    }
}

@Composable
fun RecoveryScreen(federationId: FederationId, onBack: () -> Unit, onHome: () -> Unit) {
    val vm = appViewModel { RecoveryViewModel(it.session, it.history, federationId) }
    val state by vm.state.collectAsStateWithLifecycle()
    val status by vm.status.collectAsStateWithLifecycle()
    val recovered by vm.recovered.collectAsStateWithLifecycle()
    val startedAt by vm.startedAt.collectAsStateWithLifecycle()
    val progress = state.progress
    val finished = progress?.settled == true && progress.ok

    WalletScreen(
        title = "Recovery",
        onBack = onBack,
        bottom = {
            if (status == RecoveryViewModel.Status.Failed) {
                WalletButton("Retry recovery", vm::retry, working = state.working)
            }
            // The first read failed: nothing is known yet, so offer to read again.
            if (status == null && state.error != null) {
                WalletButton("Try again", vm::load, working = state.working)
            }
            WalletButton("Go to Home", onHome, style = if (finished) ButtonStyle.Primary else ButtonStyle.Secondary)
        },
    ) {
        when (status) {
            null -> if (state.error == null) Loading()
            RecoveryViewModel.Status.NeverRecovered -> MutedText(
                "This federation was joined without recovery, so there is nothing to recover. It is fully usable.",
            )
            else -> {
                Overline(if (finished) "Wallet restored" else "Restoring wallet")
                Text(
                    when {
                        finished -> "Done"
                        progress?.fraction != null -> "${(progress.fraction * 100).toInt()}%"
                        else -> "0%"
                    },
                    style = MaterialTheme.typography.displaySmall,
                )
                Text(
                    if (finished) "Your funds are back" else "Recovering your funds",
                    style = MaterialTheme.typography.titleLarge,
                )
                MutedText(
                    if (finished) {
                        "Sending and receiving are unlocked."
                    } else {
                        "Scanning this federation's history for ecash issued to your recovery phrase. " +
                            "On a long history this can take a while."
                    },
                )
                WalletCard(padding = PaddingValues(16.dp), spacing = 12.dp) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text("Federation history scan", style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
                        progress?.let {
                            Text(
                                when {
                                    finished -> "Done"
                                    it.settled -> "Failed"
                                    it.fraction != null -> "${(it.fraction * 100).toInt()}%"
                                    else -> "Starting"
                                },
                                style = WalletText.mono,
                                color = when {
                                    finished -> MaterialTheme.walletColors.success
                                    it.settled -> MaterialTheme.colorScheme.error
                                    else -> MaterialTheme.colorScheme.onSurfaceVariant
                                },
                            )
                        }
                    }
                    ProgressBar(
                        if (finished) 1f else progress?.fraction,
                        color = if (finished) MaterialTheme.walletColors.success else MaterialTheme.colorScheme.primary,
                    )
                    progress?.detail?.let { MutedText(it) }
                    CardDivider()
                    DetailRow(if (finished) "Recovered" else "Balance so far", recovered?.let { "${formatSats(it)} sats" } ?: "—", mono = true)
                    if (!finished) startedAt?.let { Elapsed(it) }
                }
                if (!finished) {
                    Text(
                        "Recovery keeps running if you leave this screen or close the app. The balance " +
                            "is incomplete until it finishes.",
                        style = WalletText.mono,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        textAlign = TextAlign.Center,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            }
        }
        ErrorText(state.error)
    }
}

/** "Running for 1 h 4 min", ticking once a minute: the sign a long scan is still alive. */
@Composable
private fun Elapsed(startedAt: Timestamp) {
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(startedAt) {
        while (true) {
            delay(60_000)
            now = System.currentTimeMillis()
        }
    }
    val minutes = ((now - startedAt.toLong()) / 60_000).coerceAtLeast(0)
    val text = if (minutes < 60) "$minutes min" else "${minutes / 60} h ${minutes % 60} min"
    DetailRow("Running for", text, mono = true)
}
