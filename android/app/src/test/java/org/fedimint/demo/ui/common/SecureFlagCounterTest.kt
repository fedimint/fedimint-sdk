package org.fedimint.demo.ui.common

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * FLAG_SECURE stays on while any secure screen is shown, including the moment
 * navigation overlaps two of them (Backup → Verify).
 */
class SecureFlagCounterTest {
    private val changes = mutableListOf<Boolean>()
    private val counter = SecureFlagCounter { changes += it }

    @Test fun `one screen sets and clears the flag`() {
        counter.acquire()
        counter.release()
        assertEquals(listOf(true, false), changes)
    }

    @Test fun `the leaving screen doesn't clear the flag while the next is shown`() {
        counter.acquire() // Backup
        counter.acquire() // Verify enters before Backup is disposed
        counter.release() // Backup disposed
        assertEquals("still secure", listOf(true), changes)

        counter.release() // Verify leaves
        assertEquals(listOf(true, false), changes)
    }

    @Test fun `the counter is idle, so discardable, only after the last screen leaves`() {
        counter.acquire()
        counter.acquire()
        counter.release()
        assertEquals(false, counter.idle)
        counter.release()
        assertEquals(true, counter.idle)
    }

    @Test(expected = IllegalStateException::class)
    fun `releasing more than was shown is a bug`() {
        counter.release()
    }
}
