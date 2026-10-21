package org.fedimint.demo.ui.onboarding

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.input.ImeAction
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
import org.fedimint.demo.ui.common.ChoicePill
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
import org.fedimint.sdk.Mnemonic

/**
 * Restores a wallet from its recovery phrase, typed a word per box or pasted
 * whole.
 *
 * The words live in this ViewModel, in memory only. They are deliberately not
 * `rememberSaveable`: saved instance state can be written to disk, and a
 * recovery phrase must never be.
 */
class RestoreViewModel(private val session: WalletSession) : ViewModel() {
    data class UiState(
        val words: List<String> = List(12) { "" },
        val working: Boolean = false,
        val error: String? = null,
        val restored: Boolean = false,
    ) {
        val count: Int get() = words.size
        val filled: List<String> get() = words.map { it.trim().lowercase() }.filter { it.isNotEmpty() }
        val complete: Boolean get() = filled.size == count
    }

    private val _state = MutableStateFlow(UiState())
    val state = _state.asStateFlow()

    /** 12 or 24 boxes; words already typed are kept where they fit. */
    fun setCount(count: Int) {
        _state.update { s -> s.copy(words = List(count) { s.words.getOrElse(it) { "" } }, error = null) }
    }

    /** Typing a space (or pasting several words) moves on to the next boxes. */
    fun onWordChange(index: Int, text: String) {
        val parts = text.split(Regex("\\s+"))
        if (parts.size > 1 && parts.count { it.isNotEmpty() } > 1) {
            fill(index, parts.filter { it.isNotEmpty() })
            return
        }
        _state.update { s -> s.copy(words = s.words.toMutableList().also { it[index] = text.trim() }, error = null) }
    }

    /** A whole phrase from the clipboard: switches to 24 boxes if it needs them. */
    fun paste(text: String) {
        val words = pasted(text) ?: return
        _state.update { it.copy(words = words, error = null) }
    }

    private fun fill(from: Int, parts: List<String>) {
        _state.update { s ->
            val words = s.words.toMutableList()
            parts.forEachIndexed { i, word -> if (from + i < words.size) words[from + i] = word.lowercase() }
            s.copy(words = words, error = null)
        }
    }

    companion object {
        /**
         * A whole phrase pasted at once replaces every box: 12 boxes for up to
         * 12 words, 24 otherwise, with nothing left over from before. Null when
         * there is nothing to paste.
         */
        fun pasted(text: String): List<String>? {
            val parts = text.trim().split(Regex("\\s+")).filter { it.isNotEmpty() }.map { it.lowercase() }
            if (parts.isEmpty()) return null
            val count = if (parts.size > 12) 24 else 12
            return List(count) { parts.getOrElse(it) { "" } }
        }
    }

    fun restore() {
        val s = _state.value
        if (s.working) return
        _state.update { it.copy(working = true, error = null) }
        viewModelScope.launch {
            attempt {
                // Validates the words (count, wordlist, checksum) before any storage is touched.
                val mnemonic = Mnemonic.fromWords(s.filled)
                // Records the origin as restored before the seed is persisted, and carries
                // on even if this screen is left; see WalletSetup.
                session.restore(mnemonic)
                // They typed the phrase in, so they demonstrably have it: no backup step.
                session.markBackedUp()
            }
                .onSuccess { _state.update { it.copy(working = false, restored = true) } }
                .onFailure { e -> _state.update { it.copy(working = false, error = userMessage(e)) } }
        }
    }
}

@Composable
fun RestoreScreen(onBack: () -> Unit, onRestored: () -> Unit) {
    SecureScreen()
    val vm = appViewModel { RestoreViewModel(it.session) }
    val state by vm.state.collectAsStateWithLifecycle()
    val clipboard = LocalClipboardManager.current

    LaunchedEffect(state.restored) { if (state.restored) onRestored() }
    // System Back waits too, like the header's: the restore finishes either way (WalletSetup).
    BackHandler(enabled = state.working) {}

    WalletScreen(
        onBack = onBack,
        backEnabled = !state.working,
        spacing = 12.dp,
        bottom = { WalletButton("Restore wallet", vm::restore, enabled = state.complete, working = state.working) },
    ) {
        Overline("Restore")
        ScreenTitle("Enter your recovery phrase")
        MutedText(
            "Type the ${state.count} words in order. Restoring brings back your seed; funds come " +
                "back as you rejoin each federation and its recovery finishes.",
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(12, 24).forEach { n -> ChoicePill("$n words", state.count == n, { vm.setCount(n) }) }
        }
        SeedGrid(state.count) { i -> WordField(state.words[i], { vm.onWordChange(i, it) }, enabled = !state.working, last = i == state.count - 1) }
        WalletButton(
            "Paste phrase",
            { clipboard.getText()?.text?.let(vm::paste) },
            style = ButtonStyle.Secondary,
            enabled = !state.working,
            icon = R.drawable.ic_copy,
        )
        ErrorMessage(state.error)
    }
}

/** One word box: borderless, tinted while focused, with the seed keyboard. */
@Composable
private fun RowScope.WordField(value: String, onValueChange: (String) -> Unit, enabled: Boolean, last: Boolean) {
    var focused by remember { mutableStateOf(false) }
    val focus = LocalFocusManager.current
    BasicTextField(
        value = value,
        // A space ends the word: on to the next box, as on a paper backup card.
        onValueChange = { text ->
            if (text.isNotBlank() && text.last().isWhitespace() && text.trim().none(Char::isWhitespace)) {
                onValueChange(text.trim())
                focus.moveFocus(FocusDirection.Next)
            } else {
                onValueChange(text)
            }
        },
        enabled = enabled,
        singleLine = true,
        textStyle = MaterialTheme.typography.titleMedium.copy(color = MaterialTheme.colorScheme.onSurface),
        cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
        keyboardOptions = seedKeyboard(if (last) ImeAction.Done else ImeAction.Next),
        modifier = Modifier
            .weight(1f)
            .onFocusChanged { focused = it.isFocused }
            .background(if (focused) MaterialTheme.colorScheme.primaryContainer else Color.Transparent, MaterialTheme.shapes.extraSmall)
            .padding(horizontal = 4.dp, vertical = 2.dp),
        decorationBox = { field ->
            if (value.isEmpty() && !focused) Text("·····", style = WalletText.mono, color = MaterialTheme.colorScheme.outline)
            field()
        },
    )
}
