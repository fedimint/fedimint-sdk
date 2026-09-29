package org.fedimint.demo.ui.onboarding

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.fedimint.demo.ui.common.SecureScreen
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.demo.wallet.WalletSession
import org.fedimint.sdk.Mnemonic

/**
 * Restores a wallet from its recovery phrase.
 *
 * The typed phrase lives in this ViewModel, in memory only. It is deliberately
 * not `rememberSaveable`: saved instance state can be written to disk, and a
 * recovery phrase must never be.
 */
class RestoreViewModel(private val session: WalletSession) : ViewModel() {
    data class UiState(
        val phrase: String = "",
        val working: Boolean = false,
        val error: String? = null,
        val restored: Boolean = false,
    ) {
        val words: List<String> get() = phrase.trim().lowercase().split(Regex("\\s+")).filter { it.isNotEmpty() }
    }

    private val _state = MutableStateFlow(UiState())
    val state = _state.asStateFlow()

    fun onPhraseChange(text: String) {
        _state.update { it.copy(phrase = text, error = null) }
    }

    fun restore() {
        val s = _state.value
        if (s.working) return
        _state.update { it.copy(working = true, error = null) }
        viewModelScope.launch {
            attempt {
                // Validates the words (count, wordlist, checksum) before any storage is touched.
                val mnemonic = Mnemonic.fromWords(s.words)
                session.open(mnemonic)
                // They typed the phrase in, so they demonstrably have it: no backup step.
                session.markBackedUp()
            }
                .onSuccess { _state.update { it.copy(working = false, restored = true) } }
                .onFailure { e -> _state.update { it.copy(working = false, error = userMessage(e)) } }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun RestoreScreen(onBack: () -> Unit, onRestored: () -> Unit) {
    SecureScreen()
    val vm = appViewModel { RestoreViewModel(it.session) }
    val state by vm.state.collectAsStateWithLifecycle()

    LaunchedEffect(state.restored) { if (state.restored) onRestored() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Restore wallet") },
                navigationIcon = {
                    IconButton(onClick = onBack, enabled = !state.working) {
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
                "Enter your 12 or 24 word recovery phrase, in order, separated by spaces.",
                style = MaterialTheme.typography.bodyLarge,
            )
            Spacer(Modifier.height(20.dp))
            OutlinedTextField(
                value = state.phrase,
                onValueChange = vm::onPhraseChange,
                label = { Text("Recovery phrase") },
                supportingText = { Text("${state.words.size} words") },
                minLines = 4,
                enabled = !state.working,
                keyboardOptions = seedKeyboard(ImeAction.Done),
                modifier = Modifier.fillMaxWidth(),
            )
            Spacer(Modifier.height(12.dp))
            Text(
                "Restoring brings back the wallet's seed. Funds held with a federation come back " +
                    "once you rejoin it and its recovery scan finishes.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            state.error?.let {
                Spacer(Modifier.height(12.dp))
                Text(it, color = MaterialTheme.colorScheme.error)
            }
            Spacer(Modifier.height(20.dp))
            Button(
                onClick = vm::restore,
                enabled = !state.working && state.words.size in VALID_WORD_COUNTS,
                modifier = Modifier.fillMaxWidth(),
            ) {
                if (state.working) CircularProgressIndicator(Modifier.height(20.dp)) else Text("Restore")
            }
        }
    }
}

/** The BIP-39 phrase lengths. The SDK does the real validation; this only gates the button. */
private val VALID_WORD_COUNTS = setOf(12, 15, 18, 21, 24)
