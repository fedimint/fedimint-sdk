package org.fedimint.demo.ui.nav

import androidx.annotation.DrawableRes
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import androidx.navigation.NavDestination.Companion.hasRoute
import androidx.navigation.NavDestination.Companion.hierarchy
import androidx.navigation.NavGraphBuilder
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import androidx.navigation.toRoute
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import org.fedimint.demo.R
import org.fedimint.demo.ui.activity.ActivityTabScreen
import org.fedimint.demo.ui.activity.OperationDetailScreen
import org.fedimint.demo.ui.common.WalletButton
import org.fedimint.demo.ui.common.WalletIcon
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.demo.ui.federations.FederationDetailScreen
import org.fedimint.demo.ui.federations.FederationMetaScreen
import org.fedimint.demo.ui.federations.FederationsScreen
import org.fedimint.demo.ui.federations.JoinFederationScreen
import org.fedimint.demo.ui.federations.RecoveryScreen
import org.fedimint.demo.ui.home.HomeScreen
import org.fedimint.demo.ui.onboarding.BackupScreen
import org.fedimint.demo.ui.onboarding.RestoreScreen
import org.fedimint.demo.ui.onboarding.VerifyBackupScreen
import org.fedimint.demo.ui.onboarding.WelcomeScreen
import org.fedimint.demo.ui.payments.ReceiveScreen
import org.fedimint.demo.ui.payments.SendScreen
import org.fedimint.demo.wallet.WalletSession

/**
 * Decides where the app starts:
 * - no wallet on this device → [Welcome]
 * - a wallet whose phrase was never confirmed → [Backup] (e.g. the app was
 *   closed mid-onboarding; the seed exists, so start from showing it)
 * - otherwise → [Home]
 */
class LaunchViewModel(private val session: WalletSession) : ViewModel() {
    sealed interface UiState {
        data object Loading : UiState
        data class Ready(val start: Any) : UiState
        data class Failed(val message: String) : UiState
    }

    private val _state = MutableStateFlow<UiState>(UiState.Loading)
    val state = _state.asStateFlow()

    init {
        load()
    }

    fun load() {
        _state.value = UiState.Loading
        viewModelScope.launch {
            // Moves an old demo's wallet into place and undoes an interrupted setup first.
            attempt { session.prepare() }.onFailure {
                _state.value = UiState.Failed(userMessage(it))
                return@launch
            }
            _state.value = if (!session.hasWallet()) {
                UiState.Ready(Welcome)
            } else {
                attempt { session.open() }.fold(
                    { UiState.Ready(if (session.isBackedUp) Home else Backup) },
                    { UiState.Failed(userMessage(it)) },
                )
            }
        }
    }
}

/** The app's root: opens the wallet if there is one, then hands over to the navigation graph. */
@Composable
fun WalletApp() {
    val vm = appViewModel { LaunchViewModel(it.session) }
    val state by vm.state.collectAsStateWithLifecycle()

    Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
        when (val s = state) {
            LaunchViewModel.UiState.Loading -> Centered { CircularProgressIndicator(color = MaterialTheme.colorScheme.primary) }
            is LaunchViewModel.UiState.Failed -> Centered {
                Text("Couldn't open your wallet", style = MaterialTheme.typography.titleLarge)
                Spacer(Modifier.height(8.dp))
                Text(s.message, color = MaterialTheme.colorScheme.error, textAlign = TextAlign.Center)
                Spacer(Modifier.height(16.dp))
                WalletButton("Try again", vm::load)
            }
            is LaunchViewModel.UiState.Ready -> WalletNavHost(start = s.start)
        }
    }
}

/** The three tabs, in the design's order. Send and Receive open as full screens over them. */
private enum class Tab(val route: Any, val label: String, @DrawableRes val icon: Int) {
    Home(org.fedimint.demo.ui.nav.Home, "Home", R.drawable.ic_home),
    Activity(ActivityTab, "Activity", R.drawable.ic_activity),
    Federations(org.fedimint.demo.ui.nav.Federations, "Federations", R.drawable.ic_landmark),
}

@Composable
private fun WalletNavHost(start: Any, nav: NavHostController = rememberNavController()) {
    val entry by nav.currentBackStackEntryAsState()
    val current = entry?.destination
    val tab = Tab.entries.firstOrNull { t -> current?.hierarchy?.any { it.hasRoute(t.route::class) } == true }

    Scaffold(
        containerColor = MaterialTheme.colorScheme.background,
        // Screens handle the status bar and keyboard themselves; the bar takes the navigation inset.
        contentWindowInsets = WindowInsets(0),
        bottomBar = { if (tab != null) TabBar(tab) { nav.switchTo(it.route) } },
    ) { padding ->
        NavHost(
            navController = nav,
            startDestination = start,
            modifier = Modifier.padding(padding).consumeWindowInsets(padding),
        ) {
            graph(nav)
        }
    }
}

private fun NavGraphBuilder.graph(nav: NavHostController) {
    composable<Welcome> {
        WelcomeScreen(
            // The seed now exists, so there is no going back to "create or restore".
            onCreated = { nav.navigate(Backup) { popUpTo<Welcome> { inclusive = true } } },
            onRestore = { nav.navigate(Restore) },
        )
    }
    composable<Restore> {
        RestoreScreen(
            onBack = { nav.popBackStack() },
            onRestored = { nav.clearTo(Home) },
        )
    }
    composable<Backup> {
        BackupScreen(onContinue = { nav.navigate(VerifyBackup) })
    }
    composable<RecoveryPhrase> {
        BackupScreen(onContinue = {}, onBack = { nav.popBackStack() })
    }
    composable<VerifyBackup> {
        VerifyBackupScreen(
            onBack = { nav.popBackStack() },
            onVerified = { nav.clearTo(Home) },
        )
    }
    composable<Home> {
        HomeScreen(
            onJoinFederation = { nav.navigate(JoinFederation) },
            onSend = { id -> nav.navigate(Send(id)) },
            onReceive = { id -> nav.navigate(Receive(id)) },
            onOpenActivity = { nav.switchTo(ActivityTab) },
            onOpenOperation = { id, op -> nav.navigate(OperationDetail(id, op)) },
            onOpenRecovery = { id -> nav.navigate(Recovery(id)) },
            onOpenRecoveryPhrase = { nav.navigate(RecoveryPhrase) },
        )
    }
    composable<ActivityTab> {
        ActivityTabScreen(
            onOpen = { id, op -> nav.navigate(OperationDetail(id, op)) },
            onJoinFederation = { nav.navigate(JoinFederation) },
        )
    }
    composable<Federations> {
        FederationsScreen(
            onOpen = { id -> nav.navigate(FederationDetail(id)) },
            onJoin = { nav.navigate(JoinFederation) },
        )
    }
    composable<OperationDetail> { entry ->
        val route = entry.toRoute<OperationDetail>()
        OperationDetailScreen(route.federationId, route.operationId, nav::popBackStack)
    }
    composable<Send> {
        val id = it.toRoute<Send>().federationId
        SendScreen(id, nav::popBackStack, onOpenOperation = { op -> nav.navigate(OperationDetail(id, op)) })
    }
    composable<Receive> {
        val id = it.toRoute<Receive>().federationId
        ReceiveScreen(id, nav::popBackStack, onOpenOperation = { op -> nav.navigate(OperationDetail(id, op)) })
    }
    composable<JoinFederation> {
        JoinFederationScreen(
            onBack = { nav.popBackStack() },
            // Home follows the session's selected federation, which join just set.
            // A recovering join goes on to its progress instead, in place of this screen.
            onJoined = { id, recovering ->
                if (recovering) {
                    nav.navigate(Recovery(id)) { popUpTo<JoinFederation> { inclusive = true } }
                } else {
                    nav.popBackStack()
                }
            },
        )
    }
    composable<FederationDetail> { entry ->
        val id = entry.toRoute<FederationDetail>().id
        FederationDetailScreen(
            id = id,
            onBack = { nav.popBackStack() },
            onShownOnHome = { nav.switchTo(Home) },
            onOpenMeta = { nav.navigate(FederationMeta(id)) },
            onOpenRecovery = { nav.navigate(Recovery(id)) },
        )
    }
    composable<Recovery> {
        RecoveryScreen(it.toRoute<Recovery>().federationId, nav::popBackStack, onHome = { nav.switchTo(Home) })
    }
    composable<FederationMeta> { FederationMetaScreen(it.toRoute<FederationMeta>().federationId, nav::popBackStack) }
}

/** The design's tab bar: white, a hairline on top, the selected tab in the primary colour. */
@Composable
private fun TabBar(selected: Tab, onSelect: (Tab) -> Unit) {
    val scheme = MaterialTheme.colorScheme
    Column(Modifier.background(scheme.surfaceContainerLowest)) {
        HorizontalDivider(thickness = 1.dp, color = scheme.outline)
        Row(Modifier.fillMaxWidth().navigationBarsPadding().padding(horizontal = 8.dp)) {
            Tab.entries.forEach { tab ->
                val color = if (tab == selected) scheme.primary else scheme.onSurfaceVariant
                Column(
                    Modifier
                        .weight(1f)
                        .selectable(selected = tab == selected, role = Role.Tab) { onSelect(tab) }
                        .padding(top = 10.dp, bottom = 12.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    WalletIcon(tab.icon, contentDescription = null, tint = color)
                    Text(tab.label, style = MaterialTheme.typography.labelSmall, color = color)
                }
            }
        }
    }
}

/**
 * Moves to a tab, dropping whatever was open above Home. Home is always the
 * root, so back from Activity or Federations returns there, and "Go to Home"
 * from a pushed screen lands on Home itself rather than a restored stack.
 */
private fun NavHostController.switchTo(route: Any) = navigate(route) {
    popUpTo<Home>()
    launchSingleTop = true
}

/** Navigates to [route] and drops everything behind it: back from there leaves the app. */
private fun NavHostController.clearTo(route: Any) = navigate(route) {
    popUpTo(graph.id) { inclusive = true }
}

@Composable
private fun Centered(content: @Composable () -> Unit) {
    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) { content() }
}
