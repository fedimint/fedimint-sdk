package org.fedimint.demo.wallet

import android.content.SharedPreferences
import android.util.Log
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.fedimint.sdk.Amount
import org.fedimint.sdk.Capabilities
import org.fedimint.sdk.ErrorCode
import org.fedimint.sdk.Federation
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.FederationInfo
import org.fedimint.sdk.FederationPreview
import org.fedimint.sdk.FederationStatus
import org.fedimint.sdk.InviteCode
import org.fedimint.sdk.Mnemonic
import org.fedimint.sdk.Sdk
import org.fedimint.sdk.createFedimintSdk
import org.fedimint.sdk.Exception as SdkException

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
    /** Work that belongs to the session rather than a screen, like the status subscription. */
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    private val openLock = Mutex()
    private val _sdk = MutableStateFlow<Sdk?>(null)

    /** The open SDK, or null before [open]. */
    val sdk: StateFlow<Sdk?> = _sdk.asStateFlow()

    private val _federations = MutableStateFlow<List<FederationInfo>?>(null)

    /**
     * Every federation this wallet holds, running or not, kept current from
     * the SDK's status subscription. Null until the first statuses arrive.
     * Built from `storedFederations` semantics rather than `federations()`, so
     * a closed or quarantined federation shows as such instead of vanishing.
     */
    val federations: StateFlow<List<FederationInfo>?> = _federations.asStateFlow()

    private val _selectedId = MutableStateFlow(prefs.getString(KEY_SELECTED, null))

    /**
     * The federation the user last picked to look at. It may name one that
     * has since been forgotten; see [pickActive] for the one to actually show.
     */
    val selectedFederationId: StateFlow<FederationId?> = _selectedId.asStateFlow()

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
        }.also {
            _sdk.value = it
            watchStatuses(it)
        }
    }

    /** The recovery phrase of the open wallet, as plain words for display. */
    suspend fun recoveryWords(): List<String> = withContext(Dispatchers.IO) {
        requireOpen().exportMnemonic().words()
    }

    fun markBackedUp() {
        isBackedUp = true
    }

    fun select(id: FederationId) {
        _selectedId.value = id
        prefs.edit().putString(KEY_SELECTED, id).apply()
    }

    /** Reads a federation's configuration from its guardians without joining. */
    suspend fun preview(invite: String): FederationPreview = withContext(Dispatchers.IO) {
        InviteCode.parse(invite.trim()).use { requireOpen().preview(it) }
    }

    /**
     * Joins the federation and makes it the selected one. Joining one this
     * wallet already holds is not an error for the user: it just selects it.
     */
    suspend fun join(invite: String): FederationId = withContext(Dispatchers.IO) {
        InviteCode.parse(invite.trim()).use { code ->
            val id = try {
                requireOpen().join(code).use { it.id() }
            } catch (e: SdkException) {
                if (e.code() != ErrorCode.ALREADY_JOINED) throw e
                code.federationId()
            }
            select(id)
            id
        }
    }

    /** What the federation supports, or null if it isn't open (closed, quarantined, being removed). */
    suspend fun capabilities(id: FederationId): Capabilities? = withContext(Dispatchers.IO) {
        federationHandle(id)?.use { it.capabilities() }
    }

    /**
     * The federation's balance, live: the current amount first, then each
     * change. Emits nothing for a federation that isn't open. Ends with
     * FEDERATION_CLOSED if the federation closes while collected.
     */
    fun balance(id: FederationId): Flow<Amount> = flow {
        val federation = federationHandle(id) ?: return@flow
        federation.use {
            // One subscription is one cursor: it must be created once and
            // polled, never re-created per value, or it replays forever.
            it.balanceUpdates().use { updates ->
                while (true) emit(updates.next())
            }
        }
    }.flowOn(Dispatchers.IO)

    /** The federation's invite code as text, for sharing, or null if it isn't open. */
    suspend fun inviteCode(id: FederationId): String? = withContext(Dispatchers.IO) {
        federationHandle(id)?.use { federation -> federation.inviteCode().use { it.display() } }
    }

    /**
     * Stops a federation without deleting anything: its balance and history
     * stay on the device, and [reopen] brings it back. Closing a quarantined
     * federation marks it deliberately closed, so it stops being retried.
     */
    suspend fun close(id: FederationId) = withContext(Dispatchers.IO) {
        requireOpen().closeFederation(id)
    }

    /** Starts a closed or quarantined federation again, from the state already on the device. */
    suspend fun reopen(id: FederationId) = withContext(Dispatchers.IO) {
        requireOpen().reopenFederation(id).close()
    }

    /**
     * Erases the federation from this device. The SDK refuses with
     * BALANCE_NOT_EMPTY or PENDING_OPERATIONS while it still holds value
     * (unless it is recovering), and a refusal still leaves it closed.
     */
    suspend fun forget(id: FederationId) = withContext(Dispatchers.IO) {
        requireOpen().forgetFederation(id)
    }

    /**
     * Runs [block] with a live handle to the federation on [Dispatchers.IO],
     * then releases the handle. Anything the block returns (a quote, an
     * operation) holds its own reference and outlives the federation handle.
     */
    suspend fun <T> withFederation(id: FederationId, block: suspend (Federation) -> T): T =
        withContext(Dispatchers.IO) {
            val federation = federationHandle(id) ?: throw IllegalStateException("This federation isn't open.")
            federation.use { block(it) }
        }

    private fun federationHandle(id: FederationId): Federation? = requireOpen().federation(id)

    /**
     * Mirrors every federation's status into [federations]. The subscription
     * delivers the current status of each stored federation first, then every
     * change after, so it is the only source the list needs.
     */
    private fun watchStatuses(sdk: Sdk) {
        _federations.value = sdk.storedFederations()
        scope.launch {
            sdk.federationStatusUpdates().use { updates ->
                while (true) {
                    val info = try {
                        updates.next()
                    } catch (e: SdkException) {
                        // FEDERATION_CLOSED is the SDK shutting down, the stream's normal end.
                        // Anything else is an infrastructure fault; the list keeps its last
                        // known state rather than taking the app down from a background job.
                        if (e.code() != ErrorCode.FEDERATION_CLOSED) {
                            Log.e(TAG, "federation status updates stopped", e)
                        }
                        break
                    }
                    _federations.update { current ->
                        val others = current.orEmpty().filterNot { it.id == info.id }
                        if (info.status is FederationStatus.Forgotten) others else others + info
                    }
                }
            }
        }
    }

    private fun requireOpen(): Sdk = checkNotNull(_sdk.value) { "wallet is not open" }

    companion object {
        private const val TAG = "WalletSession"
        private const val KEY_BACKED_UP = "backed_up"
        private const val KEY_SELECTED = "selected_federation"

        /**
         * The federation to show: the selected one if the wallet still holds
         * it, otherwise the first usable one, otherwise any. Null only when the
         * wallet holds none.
         */
        fun pickActive(federations: List<FederationInfo>, selected: FederationId?): FederationInfo? =
            federations.firstOrNull { it.id == selected }
                ?: federations.firstOrNull { it.status is FederationStatus.Running }
                ?: federations.firstOrNull()
    }
}
