package org.fedimint.demo.wallet

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.async
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/**
 * Creating or restoring the wallet's seed, as one step at a time that no
 * screen can cut short.
 *
 * The seed's origin decides whether joining recovers funds (see [SeedOrigin]),
 * so it is written only for a directory that holds no seed yet, just before
 * the SDK persists one. A step that finds a seed already there (a restore that
 * was left mid-way, say) opens it and leaves its origin alone: Create can never
 * relabel an imported seed as fresh.
 *
 * A step that fails over what was an empty directory undoes itself
 * ([discardFresh]): the SDK creates its store before it writes the seed, so a
 * failed open can leave files without a seed. Left there, the next step would
 * take them for a seed and keep this step's label (a later Restore marked
 * CREATED), and the next launch would open them as an existing wallet and
 * generate a seed nobody has seen. Discarding loses nothing: before this step
 * there was no seed, and any seed it wrote came from a phrase the user holds
 * (restore) or was never shown to anyone (create).
 *
 * A step that the process dies in is caught the same way: the origin record
 * is written together with a "setup pending" mark, cleared only once the open
 * succeeds, and [settleAtLaunch] (run at every launch, before anything opens
 * the SDK) discards a directory left with that mark. Otherwise the launch would
 * open a seedless store and the SDK would generate a seed the user never saw.
 *
 * Clearing the mark can fail, and Android's preferences drop it from memory
 * before the disk write that failed, so [isPending] may read false while the
 * disk still says pending. Whether completion is still owed is therefore kept
 * here too, and every later step retries the durable clear before it reports
 * success: a step must never return (and Backup show a phrase) over a wallet
 * the next launch would discard. A launch is such a step too: the activity can
 * be recreated while the process, and the open wallet, survive, and it would
 * otherwise go straight to Backup.
 *
 * Steps run one after another on [scope], the session's, not the caller's:
 * leaving the Restore screen mid-restore doesn't cancel it, and a Create
 * tapped meanwhile waits for it and then finds its seed.
 *
 * Generic over the mnemonic type, with the SDK calls passed in, so the rules
 * are testable without the native library.
 */
class WalletSetup<M : Any>(
    private val scope: CoroutineScope,
    private val hasSeed: () -> Boolean,
    private val isOpen: () -> Boolean,
    /** Writes the origin and marks setup as pending, in one durable write. */
    private val recordOrigin: suspend (SeedOrigin) -> Unit,
    private val open: suspend (M?) -> Unit,
    /** Clears the pending mark once the open has succeeded. */
    private val finish: suspend () -> Unit,
    /** Whether a setup was started and never finished. */
    private val isPending: () -> Boolean,
    /** Empties the data directory and clears the origin record and pending mark. */
    private val discardFresh: suspend () -> Unit,
) {
    private val steps = Mutex()

    /** A fresh step opened its seed but the durable clear of the mark hasn't succeeded yet. */
    private var finishOwed = false

    /** Opens the wallet, creating a seed if there is none. */
    suspend fun create() = run(null)

    /** Opens the wallet over [mnemonic]. Refused once a wallet is open on this device. */
    suspend fun restore(mnemonic: M) = run(mnemonic)

    private suspend fun run(mnemonic: M?) = scope.async {
        steps.withLock {
            if (mnemonic != null && isOpen()) throw IllegalStateException("This device already holds a wallet.")
            val fresh = !isOpen() && !hasSeed()
            if (fresh) {
                recordOrigin(if (mnemonic == null) SeedOrigin.CREATED else SeedOrigin.RESTORED)
                finishOwed = true
            }
            try {
                open(mnemonic)
            } catch (e: Throwable) {
                if (fresh && !isOpen()) runCatching { discardFresh() }.onSuccess { finishOwed = false }
                throw e
            }
            // Also on a retry after clearing the mark failed, whatever isPending() reads
            // now: an open wallet must never stay marked on disk.
            if (finishOwed || isPending()) {
                finish()
                finishOwed = false
            }
        }
    }.await()

    /**
     * At every launch, before the app shows anything: settles what a setup left.
     *
     * - The wallet is already open (the activity was recreated in a process that
     *   survived): a completion still owed is retried, and a failure is thrown,
     *   so launch reports it rather than show the wallet.
     * - Not open, and the mark says pending (the process died mid-setup): the
     *   directory is discarded. Nothing is lost: there was no seed before that
     *   setup, and any it wrote came from a phrase the user holds or was never
     *   shown.
     *
     * Returns whether it discarded.
     */
    suspend fun settleAtLaunch(): Boolean = steps.withLock {
        if (isOpen()) {
            if (finishOwed || isPending()) {
                finish()
                finishOwed = false
            }
            return@withLock false
        }
        if (!isPending()) return@withLock false
        discardFresh()
        finishOwed = false
        true
    }
}
