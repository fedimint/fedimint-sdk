package org.fedimint.demo.ui.federations

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import org.fedimint.demo.ui.common.CardDivider
import org.fedimint.demo.ui.common.DetailRow
import org.fedimint.demo.ui.common.ErrorMessage
import org.fedimint.demo.ui.common.Loading
import org.fedimint.demo.ui.common.MutedText
import org.fedimint.demo.ui.common.Overline
import org.fedimint.demo.ui.common.WalletCard
import org.fedimint.demo.ui.common.WalletScreen
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.demo.ui.theme.WalletText
import org.fedimint.demo.wallet.WalletSession
import org.fedimint.sdk.FederationId
import org.json.JSONArray
import org.json.JSONObject

/**
 * Read-only federation metadata, in the SDK's three views of it:
 *
 * - `all()`: the merged key/value view, the one meant for rendering;
 * - `configMetadata()`: exactly what the configuration declares, held locally;
 * - `consensusMetadata()`: the meta module's raw document and its revision,
 *   or none if the federation doesn't run a meta module (an ordinary answer).
 */
class FederationMetaViewModel(private val session: WalletSession, private val federationId: FederationId) :
    ViewModel() {

    data class Consensus(val revision: ULong, val document: String)

    data class UiState(
        val loading: Boolean = true,
        val merged: Map<String, String> = emptyMap(),
        val config: Map<String, String> = emptyMap(),
        /** Null when the federation has no meta module. */
        val consensus: Consensus? = null,
        val error: String? = null,
    )

    private val _state = MutableStateFlow(UiState())
    val state = _state.asStateFlow()

    init {
        viewModelScope.launch {
            _state.value = attempt {
                session.withFederation(federationId) { federation ->
                    federation.meta().use { meta ->
                        UiState(
                            loading = false,
                            merged = meta.all(),
                            config = meta.configMetadata(),
                            consensus = meta.consensusMetadata()?.let { Consensus(it.revision, readable(it.value)) },
                        )
                    }
                }
            }.getOrElse { UiState(loading = false, error = userMessage(it)) }
        }
    }

    /** The document as text, indented if it is JSON; the SDK leaves parsing it to the app. */
    private fun readable(bytes: ByteArray): String {
        val text = bytes.toString(Charsets.UTF_8).trim()
        return runCatching {
            when {
                text.startsWith("{") -> JSONObject(text).toString(2)
                text.startsWith("[") -> JSONArray(text).toString(2)
                else -> text
            }
        }.getOrDefault(text)
    }
}

@Composable
fun FederationMetaScreen(federationId: FederationId, onBack: () -> Unit) {
    val vm = appViewModel { FederationMetaViewModel(it.session, federationId) }
    val state by vm.state.collectAsStateWithLifecycle()

    WalletScreen(title = "Federation metadata", onBack = onBack, spacing = 12.dp) {
        if (state.loading) {
            Loading()
            return@WalletScreen
        }
        ErrorMessage(state.error)
        if (state.error != null) return@WalletScreen

        Section("Metadata", "What the federation publishes about itself.", state.merged)
        Section("From its configuration", "Exactly as its configuration declares it.", state.config)
        Overline("Consensus metadata", Modifier.padding(top = 8.dp))
        WalletCard(padding = PaddingValues(16.dp), spacing = 6.dp) {
            val consensus = state.consensus
            if (consensus == null) {
                MutedText("This federation doesn't run a meta module.")
            } else {
                Text("Revision ${consensus.revision}", style = MaterialTheme.typography.labelMedium)
                Text(consensus.document, style = WalletText.mono)
            }
        }
    }
}

@Composable
private fun Section(title: String, subtitle: String, entries: Map<String, String>) {
    Overline(title, Modifier.padding(top = 8.dp))
    MutedText(subtitle)
    if (entries.isEmpty()) {
        MutedText("None")
        return
    }
    WalletCard(padding = PaddingValues(horizontal = 16.dp)) {
        entries.entries.forEachIndexed { i, (key, value) ->
            if (i > 0) CardDivider()
            DetailRow(key, value)
        }
    }
}
