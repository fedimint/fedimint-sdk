package org.fedimint.demo.ui.onboarding

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Button
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
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
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

/**
 * Asks for a few words at random positions. Enough to show the user wrote the
 * phrase down in order, without making them retype all of it.
 */
class VerifyBackupViewModel(private val session: WalletSession) : ViewModel() {
    data class UiState(
        /** Zero-based positions being asked for, in ascending order. */
        val positions: List<Int> = emptyList(),
        val answers: Map<Int, String> = emptyMap(),
        val error: String? = null,
        val verified: Boolean = false,
    )

    private val _state = MutableStateFlow(UiState())
    val state = _state.asStateFlow()

    /** Held here rather than in [UiState], so the answer key is never part of what the UI renders. */
    private var words: List<String> = emptyList()

    init {
        viewModelScope.launch {
            attempt { session.recoveryWords() }
                .onSuccess { loaded ->
                    words = loaded
                    val positions = loaded.indices.shuffled().take(CHALLENGE_SIZE).sorted()
                    _state.update { it.copy(positions = positions) }
                }
                .onFailure { e -> _state.update { it.copy(error = userMessage(e)) } }
        }
    }

    fun onAnswer(position: Int, text: String) {
        _state.update { it.copy(answers = it.answers + (position to text), error = null) }
    }

    fun verify() {
        val s = _state.value
        val wrong = s.positions.firstOrNull { p ->
            s.answers[p].orEmpty().trim().lowercase() != words.getOrNull(p)
        }
        if (wrong != null) {
            _state.update { it.copy(error = "Word #${wrong + 1} doesn't match. Check your written copy.") }
            return
        }
        session.markBackedUp()
        _state.update { it.copy(verified = true) }
    }

    private companion object {
        const val CHALLENGE_SIZE = 3
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun VerifyBackupScreen(onBack: () -> Unit, onVerified: () -> Unit) {
    SecureScreen()
    val vm = appViewModel { VerifyBackupViewModel(it.session) }
    val state by vm.state.collectAsStateWithLifecycle()

    LaunchedEffect(state.verified) { if (state.verified) onVerified() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Confirm your backup") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
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
                "Enter these words from your written copy to show it's complete and in order.",
                style = MaterialTheme.typography.bodyLarge,
            )
            Spacer(Modifier.height(20.dp))
            state.positions.forEachIndexed { i, position ->
                OutlinedTextField(
                    value = state.answers[position].orEmpty(),
                    onValueChange = { vm.onAnswer(position, it) },
                    label = { Text("Word #${position + 1}") },
                    singleLine = true,
                    keyboardOptions = seedKeyboard(
                        if (i == state.positions.lastIndex) ImeAction.Done else ImeAction.Next,
                    ),
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.height(12.dp))
            }
            state.error?.let {
                Text(it, color = MaterialTheme.colorScheme.error)
                Spacer(Modifier.height(12.dp))
            }
            Button(
                onClick = vm::verify,
                enabled = state.positions.isNotEmpty() && state.positions.all { !state.answers[it].isNullOrBlank() },
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text("Verify")
            }
        }
    }
}

/**
 * Keyboard settings for typing seed words. The password type keeps most
 * keyboards from learning or suggesting the words; nothing is masked, because
 * the user needs to see what they typed.
 */
fun seedKeyboard(imeAction: ImeAction = ImeAction.Default) = KeyboardOptions(
    capitalization = KeyboardCapitalization.None,
    autoCorrectEnabled = false,
    keyboardType = KeyboardType.Password,
    imeAction = imeAction,
)
