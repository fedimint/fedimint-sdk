package org.fedimint.demo.wallet

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Only a seed known to have been created here may be joined plainly by
 * default: a missing record, from an older build or an interrupted setup, must
 * lead with recovery.
 */
class SeedOriginTest {
    @Test fun `created seeds join plainly`() {
        val origin = SeedOrigin.fromStored("CREATED", legacyRestored = false)
        assertEquals(SeedOrigin.CREATED, origin)
        assertFalse(origin.recoverByDefault)
    }

    @Test fun `restored seeds recover`() {
        assertTrue(SeedOrigin.fromStored("RESTORED", legacyRestored = false).recoverByDefault)
    }

    @Test fun `the earlier build's restored flag still means restored`() {
        assertEquals(SeedOrigin.RESTORED, SeedOrigin.fromStored(null, legacyRestored = true))
    }

    @Test fun `a seed with no record is not assumed fresh`() {
        // A wallet from before the record existed, or one whose setup was cut
        // short: the seed is on disk, its origin isn't.
        val origin = SeedOrigin.fromStored(null, legacyRestored = false)
        assertEquals(SeedOrigin.UNKNOWN, origin)
        assertTrue(origin.recoverByDefault)
    }

    @Test fun `an unreadable record is not assumed fresh`() {
        assertTrue(SeedOrigin.fromStored("garbage", legacyRestored = false).recoverByDefault)
    }
}
