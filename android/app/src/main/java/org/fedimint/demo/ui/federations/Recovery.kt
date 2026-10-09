package org.fedimint.demo.ui.federations

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Card
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.formatSats
import org.fedimint.demo.ui.payments.ErrorText
import org.fedimint.demo.ui.payments.OpProgress
import org.fedimint.demo.ui.payments.PaymentScreen
import org.fedimint.demo.ui.payments.PaymentViewModel
import org.fedimint.demo.ui.payments.PrimaryButton
import org.fedimint.demo.ui.payments.ProgressCard
import org.fedimint.demo.ui.payments.progress
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
        step {
            when (session.recoveryStatus(federationId)) {
                null -> _status.value = Status.NeverRecovered
                is RecoveryState.Failed -> {
                    _status.value = Status.Failed
                    mutableState.update { it.copy(progress = RecoveryState.Failed("The last attempt stopped.").progress()) }
                }
                is RecoveryState.Running, RecoveryState.Done -> reattach()
            }
        }
        viewModelScope.launch {
            session.balance(federationId).catch { }.collect { _recovered.value = it }
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
fun RecoveryScreen(federationId: FederationId, onBack: () -> Unit) {
    val vm = appViewModel { RecoveryViewModel(it.session, it.history, federationId) }
    val state by vm.state.collectAsStateWithLifecycle()
    val status by vm.status.collectAsStateWithLifecycle()
    val recovered by vm.recovered.collectAsStateWithLifecycle()
    val startedAt by vm.startedAt.collectAsStateWithLifecycle()

    PaymentScreen("Recovery", onBack) {
        when (status) {
            null -> CircularProgressIndicator(Modifier.align(Alignment.CenterHorizontally))
            RecoveryViewModel.Status.NeverRecovered -> Text(
                "This federation was joined without recovery, so there is nothing to recover. " +
                    "It is fully usable.",
                style = MaterialTheme.typography.bodyLarge,
            )
            else -> {
                state.progress?.let { ProgressCard(it) }
                RecoveredCard(recovered, state.progress, startedAt)
                if (status == RecoveryViewModel.Status.Failed) {
                    PrimaryButton("Try again", vm::retry, enabled = true, working = state.working)
                }
            }
        }
        ErrorText(state.error)
        if (state.progress?.settled == true && state.progress?.ok == true) {
            OutlinedButton(onClick = onBack, modifier = Modifier.fillMaxWidth()) { Text("Done") }
        }
    }
}

@Composable
private fun RecoveredCard(recovered: Amount?, progress: OpProgress?, startedAt: Timestamp?) {
    val finished = progress?.settled == true && progress.ok
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(if (finished) "Recovered" else "Balance so far", style = MaterialTheme.typography.labelLarge)
            Text(
                recovered?.let { "${formatSats(it)} sats" } ?: "—",
                style = MaterialTheme.typography.headlineMedium,
            )
            if (!finished) {
                startedAt?.let { Elapsed(it) }
                Text(
                    "On a federation with a long history this can take a long time. The " +
                        "balance is incomplete until recovery finishes, and sending and " +
                        "receiving unlock then. You can leave this screen or close the app; " +
                        "the scan carries on.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
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
    Text("Running for $text", style = MaterialTheme.typography.bodyMedium)
}
