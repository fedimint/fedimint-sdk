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
 * succeeds, and [discardInterrupted] (run at launch, before anything opens the
 * SDK) discards a directory left with that mark. Otherwise the launch would
 * open a seedless store and the SDK would generate a seed the user never saw.
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

    /** Opens the wallet, creating a seed if there is none. */
    suspend fun create() = run(null)

    /** Opens the wallet over [mnemonic]. Refused once a wallet is open on this device. */
    suspend fun restore(mnemonic: M) = run(mnemonic)

    private suspend fun run(mnemonic: M?) = scope.async {
        steps.withLock {
            if (mnemonic != null && isOpen()) throw IllegalStateException("This device already holds a wallet.")
            val fresh = !isOpen() && !hasSeed()
            if (fresh) recordOrigin(if (mnemonic == null) SeedOrigin.CREATED else SeedOrigin.RESTORED)
            try {
                open(mnemonic)
            } catch (e: Throwable) {
                if (fresh && !isOpen()) runCatching { discardFresh() }
                throw e
            }
            // Also on a retry after clearing the mark failed: an open wallet must never stay marked.
            if (fresh || isPending()) finish()
        }
    }.await()

    /**
     * At launch: discards what a setup left if the process died before it
     * finished. Returns whether it did. Nothing is lost: there was no seed
     * before that setup, and any it wrote came from a phrase the user holds or
     * was never shown.
     */
    suspend fun discardInterrupted(): Boolean = steps.withLock {
        if (isOpen() || !isPending()) return@withLock false
        discardFresh()
        true
    }
}
