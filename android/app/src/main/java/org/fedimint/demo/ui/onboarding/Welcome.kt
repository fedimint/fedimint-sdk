package org.fedimint.demo.ui.onboarding

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.core.view.WindowCompat
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.fedimint.demo.R
import org.fedimint.demo.ui.common.ButtonStyle
import org.fedimint.demo.ui.common.Overline
import org.fedimint.demo.ui.common.WalletButton
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.findActivity
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.demo.ui.theme.WalletText
import org.fedimint.demo.ui.theme.walletColors
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
            // Records the origin only if no seed exists yet; see WalletSetup.
            attempt { session.create() }
                .onSuccess { _state.update { it.copy(working = false, created = true) } }
                .onFailure { e -> _state.update { it.copy(working = false, error = userMessage(e)) } }
        }
    }
}

/** First launch: create a new wallet or restore an existing one. The design's one dark screen. */
@Composable
fun WelcomeScreen(onCreated: () -> Unit, onRestore: () -> Unit) {
    val vm = appViewModel { WelcomeViewModel(it.session) }
    val state by vm.state.collectAsStateWithLifecycle()
    val colors = MaterialTheme.walletColors

    LaunchedEffect(state.created) { if (state.created) onCreated() }
    LightSystemBars()

    Box(Modifier.fillMaxSize().background(colors.hero).clipToBounds()) {
        // The brand mark, large and cropped by the right edge: decoration only.
        Image(
            painterResource(R.drawable.brand_mark),
            contentDescription = null,
            modifier = Modifier.offset(x = 160.dp, y = 110.dp).size(260.dp),
        )
        Column(
            Modifier.fillMaxSize().safeDrawingPadding().padding(start = 28.dp, end = 28.dp, top = 8.dp, bottom = 24.dp),
        ) {
            Image(painterResource(R.drawable.brand_logo_dark), contentDescription = "Fedimint", modifier = Modifier.width(132.dp))
            Spacer(Modifier.weight(1f))
            Overline("Fedimint SDK · demo wallet", color = colors.accent)
            Spacer(Modifier.height(12.dp))
            Text("Bitcoin custody, shared by people you trust.", style = MaterialTheme.typography.headlineLarge, color = colors.onHero)
            Spacer(Modifier.height(12.dp))
            Text(
                "A reference wallet showing every SDK flow — identical on iOS, Android and the web.",
                style = MaterialTheme.typography.bodyLarge,
                color = colors.onHeroMuted,
            )
            Spacer(Modifier.height(36.dp))
            WalletButton("Create new wallet", vm::create, style = ButtonStyle.Inverse, working = state.working)
            Spacer(Modifier.height(12.dp))
            WalletButton("Restore from recovery phrase", onRestore, style = ButtonStyle.OutlineOnDark, enabled = !state.working)
            state.error?.let {
                Spacer(Modifier.height(12.dp))
                Text(it, style = MaterialTheme.typography.bodyMedium, color = Color(0xFFFF8A80))
            }
            Spacer(Modifier.height(20.dp))
            Text(
                "Use test funds only",
                style = WalletText.mono,
                color = colors.onHeroMuted,
                textAlign = TextAlign.Center,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

/** Light status and navigation bar icons while a dark screen is shown; the previous look after. */
@Composable
private fun LightSystemBars() {
    val view = LocalView.current
    DisposableEffect(view) {
        val window = view.context.findActivity()?.window ?: return@DisposableEffect onDispose {}
        val controller = WindowCompat.getInsetsController(window, view)
        val status = controller.isAppearanceLightStatusBars
        val navigation = controller.isAppearanceLightNavigationBars
        controller.isAppearanceLightStatusBars = false
        controller.isAppearanceLightNavigationBars = false
        onDispose {
            controller.isAppearanceLightStatusBars = status
            controller.isAppearanceLightNavigationBars = navigation
        }
    }
}
