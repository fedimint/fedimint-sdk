package org.fedimint.demo.ui.onboarding

import android.widget.Toast
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import org.fedimint.demo.R
import org.fedimint.demo.ui.common.ButtonStyle
import org.fedimint.demo.ui.common.ErrorMessage
import org.fedimint.demo.ui.common.Loading
import org.fedimint.demo.ui.common.MutedText
import org.fedimint.demo.ui.common.Overline
import org.fedimint.demo.ui.common.ScreenTitle
import org.fedimint.demo.ui.common.SecureScreen
import org.fedimint.demo.ui.common.WalletButton
import org.fedimint.demo.ui.common.WalletIcon
import org.fedimint.demo.ui.common.WalletScreen
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.copySecret
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.demo.wallet.WalletSession

class BackupViewModel(session: WalletSession) : ViewModel() {
    sealed interface UiState {
        data object Loading : UiState
        data class Ready(val words: List<String>) : UiState
        data class Failed(val message: String) : UiState
    }

    private val _state = MutableStateFlow<UiState>(UiState.Loading)
    val state = _state.asStateFlow()

    init {
        viewModelScope.launch {
            _state.value = attempt { session.recoveryWords() }
                .fold({ UiState.Ready(it) }, { UiState.Failed(userMessage(it)) })
        }
    }
}

/**
 * Shows the recovery phrase. Sensitive: see android/app/SECURITY.md.
 *
 * During onboarding it is step 1 of 2, and the user moves on once they have
 * written it down (step 2 checks). Later, from Home ([onBack] given), it is a
 * way to see the phrase again: the words stay hidden until the user taps
 * Reveal, so opening the screen with someone looking over a shoulder doesn't
 * expose them.
 */
@Composable
fun BackupScreen(onContinue: () -> Unit, onBack: (() -> Unit)? = null) {
    SecureScreen()
    val vm = appViewModel { BackupViewModel(it.session) }
    val state by vm.state.collectAsStateWithLifecycle()
    val reviewing = onBack != null
    var revealed by remember { mutableStateOf(!reviewing) }
    var confirmCopy by remember { mutableStateOf(false) }
    val context = LocalContext.current
    val words = (state as? BackupViewModel.UiState.Ready)?.words

    WalletScreen(
        onBack = onBack,
        spacing = 12.dp,
        bottom = {
            if (reviewing) {
                WalletButton("Done", onBack!!)
            } else {
                WalletButton("I've written it down", onContinue, enabled = words != null)
            }
        },
    ) {
        if (!reviewing) Overline("Step 1 of 2")
        ScreenTitle(if (reviewing) "Your recovery phrase" else "Write down your recovery phrase")
        MutedText(
            "These ${words?.size ?: 12} words are the only way to restore your wallet. " +
                "Write them on paper, in order, and keep them somewhere safe and offline.",
        )
        Spacer(Modifier.height(8.dp))
        when (val s = state) {
            BackupViewModel.UiState.Loading -> Loading()
            is BackupViewModel.UiState.Failed -> ErrorMessage(s.message)
            is BackupViewModel.UiState.Ready ->
                if (revealed) {
                    SeedGrid(s.words.size) { SeedWord(s.words[it]) }
                    WalletButton("Copy phrase", { confirmCopy = true }, style = ButtonStyle.Secondary, icon = R.drawable.ic_copy)
                    if (confirmCopy) {
                        CopyPhraseDialog(
                            onConfirm = {
                                confirmCopy = false
                                copySecret(context, "Recovery phrase", s.words.joinToString(" "))
                                Toast.makeText(context, "Copied. Cleared from the clipboard after a minute, once you're back in the wallet.", Toast.LENGTH_LONG).show()
                            },
                            onDismiss = { confirmCopy = false },
                        )
                    }
                } else {
                    WalletButton("Reveal phrase", { revealed = true }, style = ButtonStyle.Soft)
                    MutedText("Make sure no one can see your screen first.")
                }
        }
        Spacer(Modifier.height(4.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            WalletIcon(R.drawable.ic_shield, contentDescription = null, size = 18.dp, tint = MaterialTheme.colorScheme.onSurfaceVariant)
            MutedText("Never share these words. No one from Fedimint will ever ask for them.")
        }
    }
}

/** Copying puts the phrase where other apps can read it; say so before doing it. */
@Composable
private fun CopyPhraseDialog(onConfirm: () -> Unit, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Copy your recovery phrase?") },
        text = {
            Text(
                "While it's on the clipboard, other apps and your keyboard may be able to read " +
                    "it. Paste it only somewhere safe and offline, such as a password manager. " +
                    "The wallet clears it from the clipboard after a minute, or when you come back " +
                    "to the wallet after that.",
            )
        },
        confirmButton = { TextButton(onClick = onConfirm) { Text("Copy") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
