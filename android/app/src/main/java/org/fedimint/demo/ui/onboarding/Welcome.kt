package org.fedimint.demo.ui.onboarding

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
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
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.demo.wallet.WalletSession

class WelcomeViewModel(private val session: WalletSession) : ViewModel() {
    data class UiState(
        val working: Boolean = false,
        val error: String? = null,
        val created: Boolean = false,
    )

    private val _state = MutableStateFlow(UiState())
    val state = _state.asStateFlow()

    /** Creates the wallet: opening the SDK over an empty directory generates and stores a seed. */
    fun create() {
        if (_state.value.working) return
        _state.update { it.copy(working = true, error = null) }
        viewModelScope.launch {
            attempt { session.open() }
                .onSuccess { _state.update { it.copy(working = false, created = true) } }
                .onFailure { e -> _state.update { it.copy(working = false, error = userMessage(e)) } }
        }
    }
}

/** First launch: create a new wallet or restore an existing one. */
@Composable
fun WelcomeScreen(onCreated: () -> Unit, onRestore: () -> Unit) {
    val vm = appViewModel { WelcomeViewModel(it.session) }
    val state by vm.state.collectAsStateWithLifecycle()

    LaunchedEffect(state.created) { if (state.created) onCreated() }

    Column(
        modifier = Modifier.fillMaxSize().safeDrawingPadding().padding(24.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text("Fedimint Wallet", style = MaterialTheme.typography.headlineLarge)
        Spacer(Modifier.height(12.dp))
        Text(
            "A reference wallet built on the Fedimint Android SDK. Ecash, Lightning and on-chain " +
                "bitcoin, held with the federations you choose.",
            style = MaterialTheme.typography.bodyLarge,
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(40.dp))
        Button(
            onClick = vm::create,
            enabled = !state.working,
            modifier = Modifier.fillMaxWidth(),
        ) {
            if (state.working) CircularProgressIndicator(Modifier.height(20.dp)) else Text("Create a new wallet")
        }
        Spacer(Modifier.height(12.dp))
        OutlinedButton(
            onClick = onRestore,
            enabled = !state.working,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("Restore from recovery phrase")
        }
        state.error?.let {
            Spacer(Modifier.height(16.dp))
            Text(it, color = MaterialTheme.colorScheme.error)
        }
    }
}
