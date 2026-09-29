package org.fedimint.demo.wallet

import android.content.SharedPreferences
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.fedimint.sdk.Mnemonic
import org.fedimint.sdk.Sdk
import org.fedimint.sdk.createFedimintSdk

/**
 * The one owner of the app's [Sdk].
 *
 * The SDK takes an exclusive lock on its data directory, so there must be a
 * single instance per process: it lives here, application-scoped, and every
 * screen reaches it through its ViewModel. Screens come and go; the SDK stays
 * open for the life of the process.
 *
 * Every method is main-safe: blocking and suspending SDK calls are moved to
 * [Dispatchers.IO] here, so callers never have to think about it.
 */
class WalletSession(
    private val dataDir: File,
    private val prefs: SharedPreferences,
) {
    private val openLock = Mutex()
    private val _sdk = MutableStateFlow<Sdk?>(null)

    /** The open SDK, or null before [open]. */
    val sdk: StateFlow<Sdk?> = _sdk.asStateFlow()

    /**
     * Whether this install already holds a wallet. The SDK persists its seed on
     * first open, so a non-empty data directory means one exists; there is no
     * SDK call to ask without opening it.
     */
    fun hasWallet(): Boolean = dataDir.list()?.isNotEmpty() == true

    /**
     * Whether the user has proven they wrote the recovery phrase down. App
     * state, not SDK state: the SDK has no notion of a backup.
     */
    var isBackedUp: Boolean
        get() = prefs.getBoolean(KEY_BACKED_UP, false)
        private set(value) = prefs.edit().putBoolean(KEY_BACKED_UP, value).apply()

    /**
     * Opens the SDK over this app's data directory, once. With a [mnemonic]
     * over an empty directory it restores that seed; with none it loads the
     * stored seed, or creates and persists a fresh one if there is none.
     * Calling it again returns the instance already open.
     */
    suspend fun open(mnemonic: Mnemonic? = null): Sdk = openLock.withLock {
        _sdk.value ?: withContext(Dispatchers.IO) {
            dataDir.mkdirs()
            createFedimintSdk(dataDir.path, mnemonic)
        }.also { _sdk.value = it }
    }

    /** The recovery phrase of the open wallet, as plain words for display. */
    suspend fun recoveryWords(): List<String> = withContext(Dispatchers.IO) {
        requireOpen().exportMnemonic().words()
    }

    fun markBackedUp() {
        isBackedUp = true
    }

    private fun requireOpen(): Sdk = checkNotNull(_sdk.value) { "wallet is not open" }

    private companion object {
        const val KEY_BACKED_UP = "backed_up"
    }
}
