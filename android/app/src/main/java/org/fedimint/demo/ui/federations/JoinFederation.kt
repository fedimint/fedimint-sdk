package org.fedimint.demo.ui.federations

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.fedimint.demo.R
import org.fedimint.demo.ui.common.ButtonStyle
import org.fedimint.demo.ui.common.CardDivider
import org.fedimint.demo.ui.common.DetailRow
import org.fedimint.demo.ui.common.ErrorMessage
import org.fedimint.demo.ui.common.MutedText
import org.fedimint.demo.ui.common.Overline
import org.fedimint.demo.ui.common.QrScanner
import org.fedimint.demo.ui.common.ScanHandoff
import org.fedimint.demo.ui.common.Scanned
import org.fedimint.demo.ui.common.ScreenTitle
import org.fedimint.demo.ui.common.WalletButton
import org.fedimint.demo.ui.common.WalletCard
import org.fedimint.demo.ui.common.WalletScreen
import org.fedimint.demo.ui.common.WalletTextField
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.label
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.demo.ui.theme.WalletText
import org.fedimint.demo.ui.theme.walletColors
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
     * Whether to lead with recovery: true unless the seed is known to have
     * been created on this device (see SeedOrigin). A seed that was used
     * before may have funds here, only a recovery finds them, and a plain join
     * can't be turned into one later.
     */
    val recoverByDefault: Boolean = session.seedOrigin.recoverByDefault

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

@Composable
fun JoinFederationScreen(onBack: () -> Unit, onJoined: (FederationId, recovering: Boolean) -> Unit) {
    val vm = appViewModel { JoinFederationViewModel(it.session) }
    val state by vm.state.collectAsStateWithLifecycle()
    var scanning by remember { mutableStateOf(false) }
    val preview = state.preview

    LaunchedEffect(state.joined) { state.joined?.let { onJoined(it.id, it.recovering) } }
    // System Back waits too, like the header's: leaving would cancel a join or recover mid-call.
    BackHandler(enabled = state.joining) {}
    LaunchedEffect(Unit) {
        ScanHandoff.take<Scanned.Invite>()?.let {
            vm.onInviteChange(it.text)
            vm.preview()
        }
    }

    WalletScreen(
        onBack = onBack,
        backEnabled = !state.joining,
        spacing = 12.dp,
        bottom = {
            when {
                preview == null -> WalletButton(
                    "Preview",
                    vm::preview,
                    enabled = !state.busy && state.invite.isNotBlank(),
                    working = state.previewing,
                )
                vm.recoverByDefault -> {
                    MutedText(
                        "Used it before with this recovery phrase? Recovering finds those funds.",
                        Modifier.fillMaxWidth(),
                        TextAlign.Center,
                    )
                    WalletButton("Join and recover funds", vm::recover, enabled = !state.busy, working = state.joining)
                    WalletButton("Join without recovering", vm::join, style = ButtonStyle.Secondary, enabled = !state.busy)
                }
                else -> WalletButton("Join ${preview.name ?: "federation"}", vm::join, enabled = !state.busy, working = state.joining)
            }
        },
    ) {
        ScreenTitle("Join a federation")
        MutedText("Paste an invite code from a guardian, or scan their QR.")
        Spacer(Modifier.height(4.dp))
        WalletTextField(
            state.invite,
            vm::onInviteChange,
            label = "Invite code",
            placeholder = "fed1…",
            enabled = !state.busy,
            minLines = 2,
            maxLines = 3,
            mono = true,
            keyboardOptions = KeyboardOptions(autoCorrectEnabled = false, keyboardType = KeyboardType.Uri),
        )
        WalletButton("Scan invite QR", { scanning = true }, style = ButtonStyle.Secondary, enabled = !state.busy, icon = R.drawable.ic_scan)
        TextButton(onClick = vm::useTestFederation, enabled = !state.busy) {
            Text("Use the Mutinynet test federation", color = MaterialTheme.colorScheme.primary)
        }
        ErrorMessage(state.error)
        if (preview != null) PreviewCard(preview)
    }

    if (scanning) {
        QrScanner(
            onResult = {
                scanning = false
                vm.onInviteChange(it.trim())
                vm.preview()
            },
            onDismiss = { scanning = false },
        )
    }
}

@Composable
private fun PreviewCard(preview: FederationPreview) {
    Overline("Preview", Modifier.padding(top = 8.dp))
    WalletCard(padding = PaddingValues(horizontal = 16.dp)) {
        DetailRow("Name", preview.name ?: "Unnamed federation")
        CardDivider()
        DetailRow("Guardians", preview.guardians.toString())
        CardDivider()
        DetailRow("Network", preview.network.label())
        CardDivider()
        DetailRow("Modules", preview.modules.joinToString(" · "))
        CardDivider()
        // In full: this is what to compare against the federation's published id
        // before joining, and a shortened form would hide a look-alike.
        Column(Modifier.padding(vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text("Federation ID", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            SelectionContainer { Text(preview.id, style = WalletText.mono) }
        }
    }
    preview.meta["welcome_message"]?.let { MutedText(it) }
    if (preview.network != Network.BITCOIN) {
        Text(
            "This is a test network. Its coins have no real value.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.walletColors.warning,
        )
    }
}
