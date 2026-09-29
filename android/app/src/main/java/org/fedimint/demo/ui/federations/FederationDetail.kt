package org.fedimint.demo.ui.federations

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.fedimint.demo.ui.common.StatusBadge
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.explanation
import org.fedimint.demo.ui.common.isOpen
import org.fedimint.demo.ui.common.label
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.demo.wallet.WalletSession
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
 * | Closed, Quarantined   | Reopen, Forget                  |
 * | Forgetting            | Retry removal                   |
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
            "${userMessage(e)} Nothing was deleted, but the federation is now closed. Reopen it to use it again."
        } else {
            userMessage(e)
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FederationDetailScreen(id: FederationId, onBack: () -> Unit, onShownOnHome: () -> Unit) {
    val vm = appViewModel { FederationDetailViewModel(it.session, id) }
    val state by vm.state.collectAsStateWithLifecycle()
    var confirm by remember { mutableStateOf<FederationDetailViewModel.Action?>(null) }

    LaunchedEffect(state.removed) { if (state.removed) onBack() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(state.federation?.name ?: "Federation") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
            )
        },
    ) { padding ->
        val federation = state.federation ?: return@Scaffold
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Summary(federation, state.inviteCode)
            federation.status.let { status ->
                if (status is FederationStatus.Quarantined) Diagnostic(status)
                else status.explanation()?.let { Note(it) }
            }
            state.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            Actions(
                status = federation.status,
                working = state.working,
                onShowOnHome = {
                    vm.showOnHome()
                    onShownOnHome()
                },
                onReopen = vm::reopen,
                onConfirm = { confirm = it },
            )
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

@Composable
private fun Summary(federation: FederationInfo, inviteCode: String?) {
    val clipboard = LocalClipboardManager.current
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row { Text("Status", Modifier.weight(1f)); StatusBadge(federation.status) }
            Row { Text("Network", Modifier.weight(1f)); Text(federation.network.label()) }
            Text("Federation ID", style = MaterialTheme.typography.labelMedium)
            Text(federation.id, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
            if (inviteCode != null) {
                TextButton(onClick = { clipboard.setText(AnnotatedString(inviteCode)) }) {
                    Text("Copy invite code")
                }
            }
        }
    }
}

/** Why a quarantined federation was set aside, straight from the SDK's diagnostic. */
@Composable
private fun Diagnostic(status: FederationStatus.Quarantined) {
    Card(
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.errorContainer),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text("Couldn't open this federation", style = MaterialTheme.typography.titleMedium)
            Text(
                "It was set aside so it can't do anything unsafe. Your funds are not spent. " +
                    "Reopen tries again; Close stops it being retried on every launch.",
                style = MaterialTheme.typography.bodyMedium,
            )
            Text("Code: ${status.diagnostic.code}", style = MaterialTheme.typography.labelMedium)
            Text(status.diagnostic.message, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
        }
    }
}

@Composable
private fun Note(text: String) {
    Card(
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.secondaryContainer),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Text(text, modifier = Modifier.padding(16.dp), style = MaterialTheme.typography.bodyMedium)
    }
}

@Composable
private fun Actions(
    status: FederationStatus,
    working: FederationDetailViewModel.Action?,
    onShowOnHome: () -> Unit,
    onReopen: () -> Unit,
    onConfirm: (FederationDetailViewModel.Action) -> Unit,
) {
    val busy = working != null
    @Composable
    fun Label(action: FederationDetailViewModel.Action, text: String) {
        if (working == action) CircularProgressIndicator(Modifier.height(20.dp)) else Text(text)
    }

    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        when {
            status.isOpen -> {
                Button(onClick = onShowOnHome, enabled = !busy, modifier = Modifier.fillMaxWidth()) {
                    Text("Show on Home")
                }
                OutlinedButton(
                    onClick = { onConfirm(FederationDetailViewModel.Action.Close) },
                    enabled = !busy,
                    modifier = Modifier.fillMaxWidth(),
                ) { Label(FederationDetailViewModel.Action.Close, "Close") }
            }
            status is FederationStatus.Closed || status is FederationStatus.Quarantined -> {
                Button(onClick = onReopen, enabled = !busy, modifier = Modifier.fillMaxWidth()) {
                    Label(FederationDetailViewModel.Action.Reopen, "Reopen")
                }
                if (status is FederationStatus.Quarantined) {
                    OutlinedButton(
                        onClick = { onConfirm(FederationDetailViewModel.Action.Close) },
                        enabled = !busy,
                        modifier = Modifier.fillMaxWidth(),
                    ) { Label(FederationDetailViewModel.Action.Close, "Stop retrying (close)") }
                }
            }
        }
        Spacer(Modifier.height(8.dp))
        OutlinedButton(
            onClick = { onConfirm(FederationDetailViewModel.Action.Forget) },
            enabled = !busy,
            colors = ButtonDefaults.outlinedButtonColors(contentColor = MaterialTheme.colorScheme.error),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Label(
                FederationDetailViewModel.Action.Forget,
                if (status is FederationStatus.Forgetting) "Retry removal" else "Remove from this device",
            )
        }
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
            "Remove this federation?",
            if (recovering) {
                "Recovery hasn't finished. Removing throws away everything recovered so far, and " +
                    "any funds it would have found are lost to this device. Rejoining needs the " +
                    "invite code and starts recovery from the beginning."
            } else {
                "Its history is deleted from this device for good. This only works once its " +
                    "balance is zero and no payments are in progress."
            },
            "Remove",
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
