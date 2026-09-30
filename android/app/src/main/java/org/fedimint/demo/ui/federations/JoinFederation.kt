package org.fedimint.demo.ui.federations

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.label
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.demo.wallet.WalletSession
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.FederationPreview
import org.fedimint.sdk.Network

/**
 * Join a federation in two steps: preview what the invite code points at, then
 * join. Previewing first lets the user check the name, network and guardians
 * before the wallet commits to anything.
 */
class JoinFederationViewModel(private val session: WalletSession) : ViewModel() {
    data class UiState(
        val invite: String = "",
        val previewing: Boolean = false,
        val preview: FederationPreview? = null,
        val joining: Boolean = false,
        val error: String? = null,
        /** Set once joined: the federation, and whether it is recovering. */
        val joined: Joined? = null,
    ) {
        val busy: Boolean get() = previewing || joining
    }

    data class Joined(val id: FederationId, val recovering: Boolean)

    private val _state = MutableStateFlow(UiState())
    val state = _state.asStateFlow()

    /**
     * A restored seed may already have funds in this federation, and only a
     * recovery finds them; a plain join can't be turned into one later.
     */
    val restoredWallet: Boolean = session.isRestored

    /** Editing the code invalidates a preview of the old one. */
    fun onInviteChange(text: String) {
        _state.update { it.copy(invite = text, preview = null, error = null) }
    }

    fun useTestFederation() = onInviteChange(MUTINYNET_INVITE)

    fun preview() {
        val invite = _state.value.invite
        if (_state.value.busy || invite.isBlank()) return
        _state.update { it.copy(previewing = true, error = null) }
        viewModelScope.launch {
            attempt { session.preview(invite) }
                .onSuccess { p -> _state.update { it.copy(previewing = false, preview = p) } }
                .onFailure { e -> _state.update { it.copy(previewing = false, error = userMessage(e)) } }
        }
    }

    fun join() = enter(recover = false)

    fun recover() = enter(recover = true)

    private fun enter(recover: Boolean) {
        val invite = _state.value.invite
        if (_state.value.busy) return
        _state.update { it.copy(joining = true, error = null) }
        viewModelScope.launch {
            attempt { if (recover) session.recover(invite) else session.join(invite) }
                .onSuccess { id -> _state.update { it.copy(joining = false, joined = Joined(id, recover)) } }
                .onFailure { e ->
                    val message = userMessage(e) + if (recover) {
                        // A failed recover may still have joined; the way back is Reopen.
                        " If the federation now shows in your list as needing attention, reopen it there."
                    } else {
                        ""
                    }
                    _state.update { it.copy(joining = false, error = message) }
                }
        }
    }

    private companion object {
        /** A public test federation on Mutinynet (signet); fund it free at faucet.mutinynet.com. */
        const val MUTINYNET_INVITE =
            "fed11qgqrgvnhwden5te0v9k8q6rp9ekh2arfdeukuet595cr2ttpd3jhq6rzve6zuer9wchxvetyd938gcewvdhk6tcqqysptkuvknc7erjgf4em3zfh90kffqf9srujn6q53d6r056e4apze5cw27h75"
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun JoinFederationScreen(onBack: () -> Unit, onJoined: (FederationId, recovering: Boolean) -> Unit) {
    val vm = appViewModel { JoinFederationViewModel(it.session) }
    val state by vm.state.collectAsStateWithLifecycle()

    LaunchedEffect(state.joined) { state.joined?.let { onJoined(it.id, it.recovering) } }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Join a federation") },
                navigationIcon = {
                    IconButton(onClick = onBack, enabled = !state.joining) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .imePadding()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 24.dp, vertical = 8.dp),
        ) {
            Text(
                "Paste the invite code the federation gave you. It starts with fed1.",
                style = MaterialTheme.typography.bodyLarge,
            )
            Spacer(Modifier.height(16.dp))
            OutlinedTextField(
                value = state.invite,
                onValueChange = vm::onInviteChange,
                label = { Text("Invite code") },
                minLines = 3,
                enabled = !state.busy,
                keyboardOptions = KeyboardOptions(autoCorrectEnabled = false, keyboardType = KeyboardType.Uri),
                modifier = Modifier.fillMaxWidth(),
            )
            TextButton(onClick = vm::useTestFederation, enabled = !state.busy) {
                Text("Use the Mutinynet test federation")
            }

            state.error?.let {
                Text(it, color = MaterialTheme.colorScheme.error)
                Spacer(Modifier.height(12.dp))
            }

            val preview = state.preview
            if (preview == null) {
                Spacer(Modifier.height(8.dp))
                OutlinedButton(
                    onClick = vm::preview,
                    enabled = !state.busy && state.invite.isNotBlank(),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    if (state.previewing) CircularProgressIndicator(Modifier.height(20.dp)) else Text("Preview")
                }
            } else {
                PreviewCard(preview)
                Spacer(Modifier.height(16.dp))
                if (vm.restoredWallet) {
                    Text(
                        "You restored this wallet from a recovery phrase. Recovering scans this " +
                            "federation for funds that belong to it. Sending and receiving unlock " +
                            "when the scan finishes.",
                        style = MaterialTheme.typography.bodyMedium,
                    )
                    Spacer(Modifier.height(12.dp))
                    Button(onClick = vm::recover, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) {
                        if (state.joining) CircularProgressIndicator(Modifier.height(20.dp)) else Text("Join and recover funds")
                    }
                    TextButton(onClick = vm::join, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) {
                        Text("Join without recovering (I never used it)")
                    }
                } else {
                    Button(onClick = vm::join, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) {
                        if (state.joining) {
                            CircularProgressIndicator(Modifier.height(20.dp))
                        } else {
                            Text("Join ${preview.name ?: "federation"}")
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun PreviewCard(preview: FederationPreview) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp)) {
            Text(preview.name ?: "Unnamed federation", style = MaterialTheme.typography.titleLarge)
            preview.meta["welcome_message"]?.let {
                Spacer(Modifier.height(8.dp))
                Text(it, style = MaterialTheme.typography.bodyMedium)
            }
            Spacer(Modifier.height(12.dp))
            Fact("Network", preview.network.label())
            Fact("Guardians", preview.guardians.toString())
            Fact("Modules", preview.modules.joinToString())
            if (preview.network != Network.BITCOIN) {
                Spacer(Modifier.height(8.dp))
                Text(
                    "This is a test network. Its coins have no real value.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.tertiary,
                )
            }
        }
    }
}

@Composable
private fun Fact(label: String, value: String) {
    Row(Modifier.padding(vertical = 2.dp)) {
        Text(label, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.width(96.dp))
        Text(value, style = MaterialTheme.typography.bodyMedium)
    }
}
