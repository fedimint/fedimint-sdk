package org.fedimint.demo.ui.federations

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.fedimint.demo.ui.common.ButtonStyle
import org.fedimint.demo.ui.common.CardDivider
import org.fedimint.demo.ui.common.DetailRow
import org.fedimint.demo.ui.common.ErrorMessage
import org.fedimint.demo.ui.common.FederationAvatar
import org.fedimint.demo.ui.common.Loading
import org.fedimint.demo.ui.common.Overline
import org.fedimint.demo.ui.common.StatusBadge
import org.fedimint.demo.ui.common.WalletButton
import org.fedimint.demo.ui.common.WalletCard
import org.fedimint.demo.ui.common.WalletScreen
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.explanation
import org.fedimint.demo.ui.common.formatSats
import org.fedimint.demo.ui.common.isOpen
import org.fedimint.demo.ui.common.label
import org.fedimint.demo.ui.common.shortId
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.demo.ui.payments.rememberCopy
import org.fedimint.demo.ui.theme.WalletText
import org.fedimint.demo.ui.theme.walletColors
import org.fedimint.demo.wallet.WalletSession
import org.fedimint.sdk.Amount
import org.fedimint.sdk.ErrorCode
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.FederationInfo
import org.fedimint.sdk.FederationStatus
import org.fedimint.sdk.Exception as SdkException

/**
 * One federation, and the lifecycle actions its status allows:
 *
 * | Status                | Actions                         |
 * |-----------------------|---------------------------------|
 * | Running, Recovering   | Show on Home, Close, Forget     |
 * | Closed                | Reopen, Forget                  |
 * | Quarantined           | Reopen, Close, Forget           |
 * | Forgetting            | Retry forget                    |
 *
 * The status itself comes from the session's live list, never from the
 * result of an action, so the screen shows what the SDK says happened.
 */
class FederationDetailViewModel(private val session: WalletSession, private val id: FederationId) : ViewModel() {
    enum class Action { Close, Reopen, Forget }

    data class UiState(
        /** Null until loaded, and again once the federation is gone from the wallet. */
        val federation: FederationInfo? = null,
        val loaded: Boolean = false,
        val inviteCode: String? = null,
        val working: Action? = null,
        val error: String? = null,
    ) {
        /** Loaded, and no longer held: it was forgotten. */
        val removed: Boolean get() = loaded && federation == null
    }

    private val progress = MutableStateFlow(UiState())

    val state = combine(session.federations, progress) { list, p ->
        p.copy(federation = list?.firstOrNull { it.id == id }, loaded = list != null)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), UiState())

    /** The balance while the federation is open; null otherwise. */
    @OptIn(ExperimentalCoroutinesApi::class)
    val balance = session.federations
        .map { list -> list?.firstOrNull { it.id == id }?.status?.isOpen == true }
        .distinctUntilChanged()
        .flatMapLatest { open ->
            if (open) session.balance(id).map<Amount, Amount?> { it }.catch { emit(null) } else flowOf(null)
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    init {
        refreshInviteCode()
    }

    fun showOnHome() = session.select(id)

    fun close() = run(Action.Close) { session.close(id) }

    fun reopen() = run(Action.Reopen) {
        session.reopen(id)
        refreshInviteCode()
    }

    fun forget() = run(Action.Forget) { session.forget(id) }

    private fun refreshInviteCode() {
        viewModelScope.launch {
            val code = attempt { session.inviteCode(id) }.getOrNull()
            progress.update { it.copy(inviteCode = code) }
        }
    }

    private fun run(action: Action, block: suspend () -> Unit) {
        if (progress.value.working != null) return
        progress.update { it.copy(working = action, error = null) }
        viewModelScope.launch {
            val error = attempt { block() }.exceptionOrNull()?.let { messageFor(action, it) }
            progress.update { it.copy(working = null, error = error) }
        }
    }

    /** A refused forget still stops the federation, which the user must hear about. */
    private fun messageFor(action: Action, e: Throwable): String {
        val refused = e is SdkException &&
            (e.code() == ErrorCode.BALANCE_NOT_EMPTY || e.code() == ErrorCode.PENDING_OPERATIONS)
        return if (action == Action.Forget && refused) {
            "${userMessage(e)} Nothing was forgotten, but the federation is now closed. Reopen it to use it again."
        } else {
            userMessage(e)
        }
    }
}

@Composable
fun FederationDetailScreen(
    id: FederationId,
    onBack: () -> Unit,
    onShownOnHome: () -> Unit,
    onOpenMeta: () -> Unit,
    onOpenRecovery: () -> Unit,
) {
    val vm = appViewModel { FederationDetailViewModel(it.session, id) }
    val state by vm.state.collectAsStateWithLifecycle()
    val balance by vm.balance.collectAsStateWithLifecycle()
    var confirm by remember { mutableStateOf<FederationDetailViewModel.Action?>(null) }
    val copy = rememberCopy()

    LaunchedEffect(state.removed) { if (state.removed) onBack() }

    val federation = state.federation
    WalletScreen(
        title = "Federation",
        onBack = onBack,
        bottom = federation?.let { f ->
            {
                Actions(
                    status = f.status,
                    working = state.working,
                    onShowOnHome = {
                        vm.showOnHome()
                        onShownOnHome()
                    },
                    onReopen = vm::reopen,
                    onConfirm = { confirm = it },
                )
            }
        },
    ) {
        if (federation == null) {
            if (!state.loaded) Loading()
            return@WalletScreen
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
            FederationAvatar(federation.name, size = 52.dp)
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(federation.name ?: "Unnamed federation", style = MaterialTheme.typography.titleLarge)
                StatusBadge(federation.status)
            }
        }
        federation.status.let { status ->
            if (status is FederationStatus.Quarantined) Diagnostic(status)
            else status.explanation()?.let { Note(it) }
        }
        ErrorMessage(state.error)
        Overline("Federation info")
        WalletCard(padding = PaddingValues(horizontal = 16.dp)) {
            DetailRow("Federation ID", shortId(federation.id, keep = 4), mono = true, onCopy = { copy(federation.id) })
            CardDivider()
            DetailRow("Network", federation.network.label())
            balance?.let {
                CardDivider()
                DetailRow("Balance", "${formatSats(it)} sats", mono = true)
            }
            state.inviteCode?.let { code ->
                CardDivider()
                DetailRow("Invite code", shortId(code, keep = 6), mono = true, onCopy = { copy(code) })
            }
        }
        if (federation.status is FederationStatus.Recovering) {
            WalletButton("Recovery progress", onOpenRecovery, style = ButtonStyle.Soft)
        }
        if (federation.status.isOpen) {
            WalletButton("Federation metadata", onOpenMeta, style = ButtonStyle.Secondary)
        }
    }

    confirm?.let { action ->
        ConfirmDialog(
            action = action,
            recovering = state.federation?.status is FederationStatus.Recovering,
            onConfirm = {
                confirm = null
                if (action == FederationDetailViewModel.Action.Close) vm.close() else vm.forget()
            },
            onDismiss = { confirm = null },
        )
    }
}

/** Why a quarantined federation was set aside, straight from the SDK's diagnostic. */
@Composable
private fun Diagnostic(status: FederationStatus.Quarantined) {
    val error = MaterialTheme.colorScheme.error
    WalletCard(padding = PaddingValues(16.dp), spacing = 6.dp, color = MaterialTheme.colorScheme.errorContainer, border = null) {
        Overline("Diagnostics", color = error)
        Text("code: ${status.diagnostic.code}", style = WalletText.mono, color = error)
        Text(status.diagnostic.message, style = WalletText.mono, color = error)
        Spacer(Modifier.height(4.dp))
        Text(
            "It was set aside so it can't do anything unsafe. Your funds are not spent. " +
                "Reopen tries again; Close stops it being retried on every launch.",
            style = MaterialTheme.typography.bodyMedium,
        )
    }
}

@Composable
private fun Note(text: String) {
    WalletCard(padding = PaddingValues(16.dp), color = MaterialTheme.walletColors.warningSoft, border = null) {
        Text(text, style = MaterialTheme.typography.bodyMedium)
    }
}

/**
 * The lifecycle actions the status allows: the main one full width, then
 * Close and Forget side by side.
 */
@Composable
private fun Actions(
    status: FederationStatus,
    working: FederationDetailViewModel.Action?,
    onShowOnHome: () -> Unit,
    onReopen: () -> Unit,
    onConfirm: (FederationDetailViewModel.Action) -> Unit,
) {
    val busy = working != null
    val canClose = status.isOpen || status is FederationStatus.Quarantined
    when {
        status.isOpen -> WalletButton("Show on Home", onShowOnHome, enabled = !busy)
        status is FederationStatus.Closed || status is FederationStatus.Quarantined ->
            WalletButton("Reopen", onReopen, enabled = !busy, working = working == FederationDetailViewModel.Action.Reopen)
    }
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        if (canClose) {
            WalletButton(
                "Close",
                { onConfirm(FederationDetailViewModel.Action.Close) },
                style = ButtonStyle.Secondary,
                enabled = !busy,
                working = working == FederationDetailViewModel.Action.Close,
                modifier = Modifier.weight(1f),
            )
        }
        WalletButton(
            if (status is FederationStatus.Forgetting) "Retry forget" else "Forget",
            { onConfirm(FederationDetailViewModel.Action.Forget) },
            style = ButtonStyle.Danger,
            enabled = !busy,
            working = working == FederationDetailViewModel.Action.Forget,
            modifier = Modifier.weight(1f),
        )
    }
}

@Composable
private fun ConfirmDialog(
    action: FederationDetailViewModel.Action,
    recovering: Boolean,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val (title, body, confirm) = when (action) {
        FederationDetailViewModel.Action.Close -> Triple(
            "Close this federation?",
            "It stops syncing and can't send or receive. Your balance and history stay on this " +
                "device, and you can reopen it at any time.",
            "Close",
        )
        else -> Triple(
            "Forget this federation?",
            if (recovering) {
                "Recovery hasn't finished. Forgetting throws away everything recovered so far, and " +
                    "any funds it would have found are lost to this device. Rejoining needs the " +
                    "invite code and starts recovery from the beginning."
            } else {
                "Its history is deleted from this device for good. This only works once its " +
                    "balance is zero and no payments are in progress."
            },
            "Forget",
        )
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = { Text(body) },
        confirmButton = { TextButton(onClick = onConfirm) { Text(confirm) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
