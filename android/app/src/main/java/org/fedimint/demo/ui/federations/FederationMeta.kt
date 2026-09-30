package org.fedimint.demo.ui.federations

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Card
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.demo.ui.payments.ErrorText
import org.fedimint.demo.ui.payments.PaymentScreen
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

    PaymentScreen("Federation info", onBack) {
        if (state.loading) {
            CircularProgressIndicator(Modifier.align(Alignment.CenterHorizontally))
            return@PaymentScreen
        }
        ErrorText(state.error)
        if (state.error != null) return@PaymentScreen

        Section("Metadata", "What the federation publishes about itself.", state.merged)
        Section("From its configuration", "Exactly as its configuration declares it.", state.config)
        Card(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text("Consensus metadata", style = MaterialTheme.typography.titleMedium)
                val consensus = state.consensus
                if (consensus == null) {
                    Text("This federation doesn't run a meta module.", style = MaterialTheme.typography.bodyMedium)
                } else {
                    Text("Revision ${consensus.revision}", style = MaterialTheme.typography.labelLarge)
                    Text(consensus.document, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                }
            }
        }
    }
}

@Composable
private fun Section(title: String, subtitle: String, entries: Map<String, String>) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(title, style = MaterialTheme.typography.titleMedium)
            Text(subtitle, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (entries.isEmpty()) Text("None", style = MaterialTheme.typography.bodyMedium)
            entries.forEach { (key, value) ->
                Column {
                    Text(key, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Text(value, style = MaterialTheme.typography.bodyMedium)
                }
            }
        }
    }
}
