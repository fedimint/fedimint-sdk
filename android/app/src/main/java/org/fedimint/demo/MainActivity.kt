package org.fedimint.demo

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import org.fedimint.demo.ui.nav.WalletApp
import org.fedimint.demo.ui.theme.FedimintTheme

/**
 * The reference wallet's single activity. Every screen is a Compose
 * destination in one navigation graph (see ui/nav/WalletApp.kt); the SDK lives
 * in the application-scoped WalletSession, not here, so it survives rotation
 * and this activity being recreated.
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            FedimintTheme {
                WalletApp()
            }
        }
    }
}
