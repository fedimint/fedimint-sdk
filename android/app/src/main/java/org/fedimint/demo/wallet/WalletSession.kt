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
import org.fedimint.sdk.RecoveryOperation
import org.fedimint.sdk.RecoveryState
import org.fedimint.sdk.Sdk
import org.fedimint.sdk.createFedimintSdk
import org.fedimint.sdk.Exception as SdkException

/**
 * The one owner of the app's [Sdk].
 *
 * The SDK claims its data directory, so there is one instance per storage
 * location, and the wallet's lives here, application-scoped, and every
 * screen reaches it through its ViewModel. Screens come and go; the SDK stays
 * open for the life of the process.
 *
 * Every method is main-safe: blocking and suspending SDK calls are moved to
 * [Dispatchers.IO] here, so callers never have to think about it.
 *
 * It holds the seed's storage and its origin record: read android/app/SECURITY.md
 * before changing how either is written.
 */
class WalletSession(
    private val dataDir: File,
    private val prefs: SharedPreferences,
    /** Where the old one-screen demo kept its store; see [LegacyWallet]. */
    private val legacyDir: File? = null,
) {
    /** Work that belongs to the session rather than a screen, like the status subscription. */
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    private val openLock = Mutex()

    /** The open SDK, or null before [open]. Never handed out: screens go through this class. */
    private val _sdk = MutableStateFlow<Sdk?>(null)

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
     * Calling it again when it is already open does nothing.
     *
     * Opening over an empty directory persists a seed, so a new wallet goes
     * through [create] or [restore], which record its origin first.
     */
    suspend fun open(mnemonic: Mnemonic? = null) {
        openLock.withLock {
            if (_sdk.value != null) return@withLock
            val sdk = withContext(Dispatchers.IO) {
                dataDir.mkdirs()
                createFedimintSdk(dataDir.path, mnemonic)
            }
            _sdk.value = sdk
            watchStatuses(sdk)
        }
    }

    /** The recovery phrase of the open wallet, as plain words for display. */
    suspend fun recoveryWords(): List<String> = withContext(Dispatchers.IO) {
        requireOpen().exportMnemonic().words()
    }

    fun markBackedUp() {
        isBackedUp = true
    }

    /**
     * Where this wallet's seed came from; see [SeedOrigin] for why an unknown
     * origin is treated like a restore when joining.
     */
    val seedOrigin: SeedOrigin
        get() = SeedOrigin.fromStored(prefs.getString(KEY_SEED_ORIGIN, null), prefs.getBoolean(KEY_RESTORED_LEGACY, false))

    private val setup = WalletSetup<Mnemonic>(
        scope = scope,
        hasSeed = ::hasWallet,
        isOpen = { _sdk.value != null },
        recordOrigin = ::recordSeedOrigin,
        open = ::open,
        finish = ::finishSetup,
        isPending = { prefs.getBoolean(KEY_SETUP_PENDING, false) },
        discardFresh = ::discardFreshSetup,
    )

    /**
     * Run once at launch, before anything opens the SDK: moves an old demo's
     * store into place ([LegacyWallet]) and discards a setup the process died
     * in ([WalletSetup.discardInterrupted]). Off the main thread; a failure is
     * reported, not a crash, and launch can try again.
     */
    suspend fun prepare() {
        withContext(Dispatchers.IO) { legacyDir?.let { LegacyWallet.adopt(it, dataDir) } }
        setup.discardInterrupted()
    }

    /** Opens the wallet, creating a seed if this device holds none; see [WalletSetup]. */
    suspend fun create() = setup.create()

    /** Opens the wallet over a restored [mnemonic]; see [WalletSetup]. */
    suspend fun restore(mnemonic: Mnemonic) = setup.restore(mnemonic)

    /**
     * Records where the seed [open] is about to persist comes from, for
     * [WalletSetup], which calls it only over an empty directory. Written
     * synchronously (`commit`, not `apply`): a process death between the two
     * then leaves the record ahead of the seed, never behind it. A record with
     * no seed is harmless; the next create or restore overwrites it.
     */
    private suspend fun recordSeedOrigin(origin: SeedOrigin) {
        require(origin != SeedOrigin.UNKNOWN) { "record a known origin" }
        withContext(Dispatchers.IO) {
            val saved = prefs.edit().putString(KEY_SEED_ORIGIN, origin.name).putBoolean(KEY_SETUP_PENDING, true).commit()
            check(saved) { "Couldn't save the wallet's setup." }
        }
    }

    /**
     * Clears the "setup pending" mark. Checked: a mark left by a failed write
     * would make the next launch discard a wallet in use, so failing here
     * fails the step instead, and a retry clears it.
     */
    private suspend fun finishSetup() {
        withContext(Dispatchers.IO) {
            check(prefs.edit().remove(KEY_SETUP_PENDING).commit()) { "Couldn't finish the wallet's setup." }
        }
    }

    /**
     * Undoes a failed create or restore over an empty directory, for
     * [WalletSetup]: the store files the SDK made before failing, and the
     * origin record, so the directory reads as empty and the origin as unknown.
     */
    private suspend fun discardFreshSetup() {
        withContext(Dispatchers.IO) {
            // Files first, and every one of them: the marks are cleared only once the
            // directory really is empty, so a partial delete is retried at the next
            // launch rather than opened as a wallet (and given a seed nobody saw).
            val emptied = dataDir.listFiles()?.all { it.deleteRecursively() } ?: true
            check(emptied && dataDir.list().isNullOrEmpty()) { "Couldn't clear an unfinished wallet setup." }
            check(prefs.edit().remove(KEY_SEED_ORIGIN).remove(KEY_SETUP_PENDING).commit()) {
                "Couldn't clear an unfinished wallet setup."
            }
        }
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

    /**
     * Joins the federation and rescans its history for this seed's funds: the
     * way to join after restoring a wallet. The federation stays locked (no
     * sends or receives) until the rescan finishes; follow it with
     * [resumeRecovery]. Selected once joined.
     *
     * If it was already joined, returns its id: the recovery screen then
     * reattaches through [resumeRecovery]. A failure may still have joined it,
     * in which case it shows up quarantined and Reopen resumes the recovery.
     */
    suspend fun recover(invite: String): FederationId = withContext(Dispatchers.IO) {
        InviteCode.parse(invite.trim()).use { code ->
            val id = try {
                requireOpen().recover(code).let { handle ->
                    handle.progress.close()
                    handle.federation.use { it.id() }
                }
            } catch (e: SdkException) {
                if (e.code() != ErrorCode.ALREADY_JOINED) throw e
                code.federationId()
            }
            select(id)
            id
        }
    }

    /**
     * Where this federation's recovery stands: null if it was joined plainly
     * and never recovered; otherwise Running, Failed (still locked), or Done.
     */
    suspend fun recoveryStatus(id: FederationId): RecoveryState? = withContext(Dispatchers.IO) {
        requireOpen().recoveryStatus(id)
    }

    /**
     * The recovery operation to follow. On a running recovery this reattaches
     * without starting anything; on a failed one it starts a new attempt; on
     * a finished one it returns that one, reading Done.
     */
    suspend fun resumeRecovery(id: FederationId): RecoveryOperation = withContext(Dispatchers.IO) {
        requireOpen().resumeRecovery(id).let { handle ->
            handle.federation.close()
            handle.progress
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
                    _federations.update { current -> applyStatus(current, info) }
                }
            }
        }
    }

    private fun requireOpen(): Sdk = checkNotNull(_sdk.value) { "wallet is not open" }

    companion object {
        /**
         * The list after one status update: replaced in place, so a status
         * change never reorders it (Home falls back to the first open
         * federation when none is selected); added at the end if new; removed
         * once forgotten.
         */
        internal fun applyStatus(current: List<FederationInfo>?, info: FederationInfo): List<FederationInfo> {
            val list = current.orEmpty()
            return when {
                info.status is FederationStatus.Forgotten -> list.filterNot { it.id == info.id }
                list.any { it.id == info.id } -> list.map { if (it.id == info.id) info else it }
                else -> list + info
            }
        }

        private const val TAG = "WalletSession"
        private const val KEY_BACKED_UP = "backed_up"
        private const val KEY_SEED_ORIGIN = "seed_origin"
        private const val KEY_SETUP_PENDING = "setup_pending"

        /** Written by an earlier build, only for restores, and only after the seed. Read, never written. */
        private const val KEY_RESTORED_LEGACY = "restored"
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
