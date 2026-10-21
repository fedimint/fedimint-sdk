package org.fedimint.demo.wallet

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The seed's origin is written only for a directory with no seed, and a
 * restore left mid-way still finishes, so Create can't relabel its seed.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class WalletSetupTest {
    /** The SDK's side: a directory that holds a seed once an open has persisted one. */
    private class Fake(var hasSeed: Boolean = false) {
        var open = false
        val origins = mutableListOf<SeedOrigin>()
        val opens = mutableListOf<String?>()

        /** When set, an open persists the seed and then waits here before publishing. */
        var pauseAfterPersist: CompletableDeferred<Unit>? = null

        /** When set, the next open leaves the SDK's files behind (no seed) and then fails. */
        var failNextOpen = false
        var discards = 0

        /** The durable "setup pending" mark, written with the origin. */
        var pending = false

        /** When set, the next attempt to clear the mark fails (a disk-full commit). */
        var failNextFinish = false

        fun setup(scope: TestScope) = WalletSetup<String>(
            // Like the session's: a supervisor, so a refused step fails only its caller.
            scope = CoroutineScope(SupervisorJob() + StandardTestDispatcher(scope.testScheduler)),
            hasSeed = { hasSeed },
            isOpen = { open },
            recordOrigin = {
                origins += it
                pending = true
            },
            finish = {
                if (failNextFinish) {
                    failNextFinish = false
                    throw IllegalStateException("Couldn't finish the wallet's setup.")
                }
                pending = false
            },
            isPending = { pending },
            discardFresh = {
                discards++
                hasSeed = false
                pending = false
                origins.clear()
            },
            open = { mnemonic ->
                opens += mnemonic
                if (failNextOpen) {
                    failNextOpen = false
                    hasSeed = true // LOCK and db/ exist; the seed was never written
                    throw IllegalStateException("ENTROPY")
                }
                if (!open) {
                    hasSeed = true
                    pauseAfterPersist?.await()
                    open = true
                }
            },
        )
    }

    @Test fun `leaving Restore after the seed is stored, then Create, keeps it restored`() = runTest {
        val fake = Fake().apply { pauseAfterPersist = CompletableDeferred() }
        val setup = fake.setup(this)

        // Restore starts, the SDK stores the imported seed, and the user backs out.
        val restoring = launch { setup.restore("imported words") }
        advanceUntilIdle()
        assertTrue("the seed is stored", fake.hasSeed)
        restoring.cancel()

        // Welcome's Create, while that restore is still finishing.
        val creating = launch { setup.create() }
        advanceUntilIdle()
        fake.pauseAfterPersist!!.complete(Unit)
        advanceUntilIdle()

        assertTrue(creating.isCompleted)
        assertEquals("only the restore recorded an origin", listOf(SeedOrigin.RESTORED), fake.origins)
        assertEquals("the restore finished, then Create opened that seed", listOf("imported words", null), fake.opens)
        assertTrue(fake.open)
    }

    @Test fun `Create over a seed that is already stored records nothing`() = runTest {
        val fake = Fake(hasSeed = true)
        fake.setup(this).create()
        assertEquals(emptyList<SeedOrigin>(), fake.origins)
        assertEquals(listOf<String?>(null), fake.opens)
    }

    @Test fun `a fresh create and a fresh restore record their origin before the seed exists`() = runTest {
        val created = Fake()
        created.setup(this).create()
        assertEquals(listOf(SeedOrigin.CREATED), created.origins)

        val restored = Fake()
        restored.setup(this).restore("words")
        assertEquals(listOf(SeedOrigin.RESTORED), restored.origins)
    }

    @Test fun `restoring once a wallet is open is refused`() = runTest {
        val fake = Fake(hasSeed = true).apply { open = true }
        val result = runCatching { fake.setup(this).restore("other words") }
        assertTrue(result.isFailure)
        assertEquals(emptyList<String?>(), fake.opens)
    }

    @Test fun `a failed create is undone, so a later restore is labelled restored`() = runTest {
        val fake = Fake().apply { failNextOpen = true }
        val setup = fake.setup(this)

        assertTrue(runCatching { setup.create() }.isFailure)
        assertEquals("the half-made store is discarded", 1, fake.discards)
        assertFalse("the directory reads as empty again, so a relaunch shows Welcome", fake.hasSeed)
        assertEquals("no CREATED left behind", emptyList<SeedOrigin>(), fake.origins)

        setup.restore("imported words")
        assertEquals(listOf(SeedOrigin.RESTORED), fake.origins)
        assertTrue(fake.open)
    }

    @Test fun `a failed step over an existing seed discards nothing`() = runTest {
        val fake = Fake(hasSeed = true).apply { failNextOpen = true }
        assertTrue(runCatching { fake.setup(this).create() }.isFailure)
        assertEquals(0, fake.discards)
        assertTrue(fake.hasSeed)
    }

    @Test fun `a setup the process died in is discarded at the next launch`() = runTest {
        // Restore recorded its origin and the SDK made its files, then the process died.
        val fake = Fake().apply {
            origins += SeedOrigin.RESTORED
            pending = true
            hasSeed = true
        }

        assertTrue(fake.setup(this).discardInterrupted())
        assertFalse("Welcome again, not a seed nobody has seen", fake.hasSeed)
        assertEquals(emptyList<SeedOrigin>(), fake.origins)
    }

    @Test fun `a finished setup is never discarded`() = runTest {
        val fake = Fake()
        val setup = fake.setup(this)
        setup.restore("words")
        assertFalse("the pending mark is cleared once open succeeds", fake.pending)

        fake.open = false // a later launch, before opening
        assertFalse(setup.discardInterrupted())
        assertTrue(fake.hasSeed)
        assertEquals(listOf(SeedOrigin.RESTORED), fake.origins)
    }

    @Test fun `a failed clear of the pending mark fails the step, and a retry clears it`() = runTest {
        val fake = Fake().apply { failNextFinish = true }
        val setup = fake.setup(this)

        assertTrue("the user sees the failure", runCatching { setup.create() }.isFailure)
        assertTrue("the mark is still there", fake.pending)

        setup.create() // Try again: the wallet is open now, and the mark is cleared
        assertFalse(fake.pending)
        assertFalse("so the next launch keeps the wallet", setup.discardInterrupted())
        assertTrue(fake.hasSeed)
        assertEquals(listOf(SeedOrigin.CREATED), fake.origins)
    }
}
