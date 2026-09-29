package org.fedimint.demo

import android.app.Application
import java.io.File
import org.fedimint.demo.wallet.WalletSession

/**
 * Process-wide objects, created once and handed to ViewModels by hand.
 *
 * Manual injection rather than Hilt/Koin: the graph is one object today and a
 * handful at most, and a reference app should show the SDK, not a DI framework.
 * If the graph grows, this class is the only place that changes.
 */
class AppContainer(app: Application) {
    val session = WalletSession(
        dataDir = File(app.filesDir, "wallet"),
        prefs = app.getSharedPreferences("wallet", Application.MODE_PRIVATE),
    )
}

class FedimintApp : Application() {
    lateinit var container: AppContainer
        private set

    override fun onCreate() {
        super.onCreate()
        container = AppContainer(this)
    }
}
