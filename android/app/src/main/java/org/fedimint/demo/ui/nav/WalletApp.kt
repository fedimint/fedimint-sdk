package org.fedimint.demo.ui.nav

import android.content.Intent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import androidx.navigation.toRoute
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import org.fedimint.demo.harness.HarnessActivity
import org.fedimint.demo.ui.activity.ActivityScreen
import org.fedimint.demo.ui.activity.OperationDetailScreen
import org.fedimint.demo.ui.common.appViewModel
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.demo.ui.federations.FederationDetailScreen
import org.fedimint.demo.ui.federations.FederationsScreen
import org.fedimint.demo.ui.federations.JoinFederationScreen
import org.fedimint.demo.ui.home.HomeScreen
import org.fedimint.demo.ui.onboarding.BackupScreen
import org.fedimint.demo.ui.payments.EcashReceiveScreen
import org.fedimint.demo.ui.payments.EcashSendScreen
import org.fedimint.demo.ui.payments.LightningReceiveScreen
import org.fedimint.demo.ui.payments.LightningSendScreen
import org.fedimint.demo.ui.payments.OnchainReceiveScreen
import org.fedimint.demo.ui.payments.OnchainSendScreen
import org.fedimint.demo.ui.payments.PaymentDirection
import org.fedimint.demo.ui.payments.Rail
import org.fedimint.demo.ui.onboarding.RestoreScreen
import org.fedimint.demo.ui.onboarding.VerifyBackupScreen
import org.fedimint.demo.ui.onboarding.WelcomeScreen
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

    Surface(Modifier.fillMaxSize()) {
        when (val s = state) {
            LaunchViewModel.UiState.Loading -> Centered { CircularProgressIndicator() }
            is LaunchViewModel.UiState.Failed -> Centered {
                Text("Couldn't open your wallet", style = MaterialTheme.typography.titleLarge)
                Spacer(Modifier.height(8.dp))
                Text(s.message, color = MaterialTheme.colorScheme.error)
                Spacer(Modifier.height(16.dp))
                Button(onClick = vm::load) { Text("Try again") }
            }
            is LaunchViewModel.UiState.Ready -> WalletNavHost(start = s.start)
        }
    }
}

@Composable
private fun WalletNavHost(start: Any, nav: NavHostController = rememberNavController()) {
    val context = LocalContext.current

    NavHost(navController = nav, startDestination = start) {
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
        composable<VerifyBackup> {
            VerifyBackupScreen(
                onBack = { nav.popBackStack() },
                onVerified = { nav.clearTo(Home) },
            )
        }
        composable<Home> {
            HomeScreen(
                onJoinFederation = { nav.navigate(JoinFederation) },
                onOpenFederations = { nav.navigate(Federations) },
                onOpenDeveloperTools = { context.startActivity(Intent(context, HarnessActivity::class.java)) },
                onPay = { direction, rail, id -> nav.navigate(paymentRoute(direction, rail, id)) },
                onOpenActivity = { id -> nav.navigate(Activity(id)) },
                onOpenOperation = { id, op -> nav.navigate(OperationDetail(id, op)) },
            )
        }
        composable<Activity> { entry ->
            val id = entry.toRoute<Activity>().federationId
            ActivityScreen(id, nav::popBackStack, onOpen = { op -> nav.navigate(OperationDetail(id, op)) })
        }
        composable<OperationDetail> { entry ->
            val route = entry.toRoute<OperationDetail>()
            OperationDetailScreen(route.federationId, route.operationId, nav::popBackStack)
        }
        composable<LightningReceive> { LightningReceiveScreen(it.toRoute<LightningReceive>().federationId, nav::popBackStack) }
        composable<LightningSend> { LightningSendScreen(it.toRoute<LightningSend>().federationId, nav::popBackStack) }
        composable<EcashReceive> { EcashReceiveScreen(it.toRoute<EcashReceive>().federationId, nav::popBackStack) }
        composable<EcashSend> { EcashSendScreen(it.toRoute<EcashSend>().federationId, nav::popBackStack) }
        composable<OnchainReceive> { OnchainReceiveScreen(it.toRoute<OnchainReceive>().federationId, nav::popBackStack) }
        composable<OnchainSend> { OnchainSendScreen(it.toRoute<OnchainSend>().federationId, nav::popBackStack) }
        composable<JoinFederation> {
            JoinFederationScreen(
                onBack = { nav.popBackStack() },
                // Home follows the session's selected federation, which join just set.
                onJoined = { nav.popBackStack() },
            )
        }
        composable<Federations> {
            FederationsScreen(
                onBack = { nav.popBackStack() },
                onOpen = { id -> nav.navigate(FederationDetail(id)) },
                onJoin = { nav.navigate(JoinFederation) },
            )
        }
        composable<FederationDetail> { entry ->
            FederationDetailScreen(
                id = entry.toRoute<FederationDetail>().id,
                onBack = { nav.popBackStack() },
                onShownOnHome = { nav.popBackStack<Home>(inclusive = false) },
            )
        }
    }
}

private fun paymentRoute(direction: PaymentDirection, rail: Rail, id: String): Any = when (direction) {
    PaymentDirection.Receive -> when (rail) {
        Rail.Lightning -> LightningReceive(id)
        Rail.Ecash -> EcashReceive(id)
        Rail.Onchain -> OnchainReceive(id)
    }
    PaymentDirection.Send -> when (rail) {
        Rail.Lightning -> LightningSend(id)
        Rail.Ecash -> EcashSend(id)
        Rail.Onchain -> OnchainSend(id)
    }
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
