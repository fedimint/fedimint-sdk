package org.fedimint.demo.ui.onboarding

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
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
import org.fedimint.demo.ui.common.ChoiceTile
import org.fedimint.demo.ui.common.ErrorMessage
import org.fedimint.demo.ui.common.MutedText
import org.fedimint.demo.ui.common.Overline
import org.fedimint.demo.ui.common.ScreenTitle
import org.fedimint.demo.ui.common.SecureScreen
import org.fedimint.demo.ui.common.WalletButton
import org.fedimint.demo.ui.common.WalletScreen
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.demo.ui.theme.WalletText
import org.fedimint.demo.wallet.WalletSession

/**
 * Asks for the words at a few random positions, each picked from three: the
 * right one and two others from the same phrase. Enough to show the user wrote
 * the phrase down in order, without making them retype it.
 */
class VerifyBackupViewModel(private val session: WalletSession) : ViewModel() {
    data class UiState(
        /** Zero-based positions being asked for, in ascending order. */
        val positions: List<Int> = emptyList(),
        /** For each position, three words to pick from, in random order. */
        val options: Map<Int, List<String>> = emptyMap(),
        val picks: Map<Int, String> = emptyMap(),
        val error: String? = null,
        val verified: Boolean = false,
    ) {
        val complete: Boolean get() = positions.isNotEmpty() && positions.all { it in picks }
    }

    private val _state = MutableStateFlow(UiState())
    val state = _state.asStateFlow()

    /** The phrase, to check picks against. The options necessarily contain each answer, but not which it is. */
    private var words: List<String> = emptyList()

    init {
        viewModelScope.launch {
            attempt { session.recoveryWords() }
                .onSuccess { loaded ->
                    words = loaded
                    val positions = loaded.indices.shuffled().take(CHALLENGE_SIZE).sorted()
                    val options = positions.associateWith { p ->
                        val others = loaded.filterIndexed { i, w -> i != p && w != loaded[p] }.distinct().shuffled().take(2)
                        (others + loaded[p]).shuffled()
                    }
                    _state.update { it.copy(positions = positions, options = options) }
                }
                .onFailure { e -> _state.update { it.copy(error = userMessage(e)) } }
        }
    }

    fun pick(position: Int, word: String) {
        _state.update { it.copy(picks = it.picks + (position to word), error = null) }
    }

    fun verify() {
        val s = _state.value
        val wrong = s.positions.firstOrNull { p -> s.picks[p] != words.getOrNull(p) }
        if (wrong != null) {
            _state.update {
                it.copy(
                    picks = it.picks - wrong,
                    error = "Word %02d doesn't match. Check your written copy.".format(wrong + 1),
                )
            }
            return
        }
        session.markBackedUp()
        _state.update { it.copy(verified = true) }
    }

    private companion object {
        const val CHALLENGE_SIZE = 3
    }
}

@Composable
fun VerifyBackupScreen(onBack: () -> Unit, onVerified: () -> Unit) {
    SecureScreen()
    val vm = appViewModel { VerifyBackupViewModel(it.session) }
    val state by vm.state.collectAsStateWithLifecycle()

    LaunchedEffect(state.verified) { if (state.verified) onVerified() }

    WalletScreen(
        onBack = onBack,
        spacing = 12.dp,
        bottom = {
            Text(
                "${state.picks.size} / ${state.positions.size.coerceAtLeast(1)} confirmed",
                style = WalletText.mono,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
                modifier = Modifier.fillMaxWidth(),
            )
            WalletButton("Continue", vm::verify, enabled = state.complete)
        },
    ) {
        Overline("Step 2 of 2")
        ScreenTitle("Confirm three words")
        MutedText("Pick the right word for each position, from your written copy.")
        state.positions.forEach { position ->
            Spacer(Modifier.height(8.dp))
            Overline("Word %02d".format(position + 1))
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                state.options[position].orEmpty().forEach { word ->
                    ChoiceTile(word, selected = state.picks[position] == word, onClick = { vm.pick(position, word) }, modifier = Modifier.weight(1f))
                }
            }
        }
        ErrorMessage(state.error)
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
