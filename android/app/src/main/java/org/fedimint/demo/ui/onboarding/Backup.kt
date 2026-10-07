package org.fedimint.demo.ui.onboarding

import android.widget.Toast
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import org.fedimint.demo.ui.common.SecureScreen
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
 * During onboarding it asks the user to confirm they wrote it down before
 * moving on. Later, from Home's menu ([onBack] given), it is a way to see the
 * phrase again: the words stay hidden until the user taps Reveal, so opening
 * the screen with someone looking over a shoulder doesn't expose them.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun BackupScreen(onContinue: () -> Unit, onBack: (() -> Unit)? = null) {
    SecureScreen()
    val vm = appViewModel { BackupViewModel(it.session) }
    val state by vm.state.collectAsStateWithLifecycle()
    var acknowledged by remember { mutableStateOf(false) }
    val reviewing = onBack != null
    var revealed by remember { mutableStateOf(!reviewing) }
    var confirmCopy by remember { mutableStateOf(false) }
    val context = LocalContext.current

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Your recovery phrase") },
                navigationIcon = {
                    if (onBack != null) {
                        IconButton(onClick = onBack) {
                            Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                        }
                    }
                },
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 24.dp, vertical = 8.dp),
        ) {
            Text(
                "These words are the only way to restore your wallet if you lose this phone. " +
                    "Write them down, in order, and keep them somewhere safe and offline.",
                style = MaterialTheme.typography.bodyLarge,
            )
            Spacer(Modifier.height(12.dp))
            Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.errorContainer)) {
                Text(
                    "Anyone who has these words can take your funds. Never share them, " +
                        "and never type them into a website.",
                    modifier = Modifier.padding(16.dp),
                    color = MaterialTheme.colorScheme.onErrorContainer,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
            Spacer(Modifier.height(20.dp))

            when (val s = state) {
                BackupViewModel.UiState.Loading -> CircularProgressIndicator(Modifier.align(Alignment.CenterHorizontally))
                is BackupViewModel.UiState.Failed -> Text(s.message, color = MaterialTheme.colorScheme.error)
                is BackupViewModel.UiState.Ready ->
                    if (revealed) {
                        WordGrid(s.words)
                        TextButton(onClick = { confirmCopy = true }, modifier = Modifier.align(Alignment.End)) {
                            Text("Copy")
                        }
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
                        OutlinedButton(onClick = { revealed = true }, modifier = Modifier.fillMaxWidth()) {
                            Text("Reveal (make sure no one can see your screen)")
                        }
                    }
            }

            Spacer(Modifier.height(20.dp))
            if (reviewing) {
                Button(onClick = onBack, modifier = Modifier.fillMaxWidth()) { Text("Done") }
                return@Column
            }
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth().clickable { acknowledged = !acknowledged },
            ) {
                Checkbox(checked = acknowledged, onCheckedChange = { acknowledged = it })
                Text("I've written these words down, in order.")
            }
            Spacer(Modifier.height(12.dp))
            Button(
                onClick = onContinue,
                enabled = acknowledged && state is BackupViewModel.UiState.Ready,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text("Continue")
            }
            Spacer(Modifier.height(16.dp))
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

/** The words in two numbered columns, read top to bottom then left to right like a paper backup card. */
@Composable
private fun WordGrid(words: List<String>) {
    val half = (words.size + 1) / 2
    Card(Modifier.fillMaxWidth()) {
        Row(Modifier.padding(16.dp), horizontalArrangement = Arrangement.spacedBy(24.dp)) {
            listOf(words.take(half), words.drop(half)).forEachIndexed { column, chunk ->
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    chunk.forEachIndexed { row, word ->
                        Row {
                            Text(
                                "${column * half + row + 1}.",
                                modifier = Modifier.width(32.dp),
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                fontFamily = FontFamily.Monospace,
                            )
                            Text(word, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodyLarge)
                        }
                    }
                }
            }
        }
    }
}
