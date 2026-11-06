package org.fedimint.demo.ui.common

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * A copied phrase leaves the clipboard after its expiry even when the wallet
 * was in the background at the deadline (Android only lets the focused app
 * read the clipboard), and nothing the user copied since is touched.
 */
class ClipboardExpiryTest {
    private var clock = 0L
    private val expiry = ClipboardExpiry(now = { clock }, ttlMillis = 60_000)

    /** What the clipboard returns: null while the wallet has no focus. */
    private var visible: String? = null
    private val read = { visible }

    @Test fun `cleared at the deadline while the wallet is in front`() {
        expiry.copied("seed words")
        visible = "seed words"
        clock = 60_000
        assertTrue(expiry.check(read, focused = false))
    }

    @Test fun `copy, background past the deadline, come back, cleared then`() {
        expiry.copied("seed words")
        clock = 60_000
        visible = null // in the password manager: the wallet can't read the clipboard
        assertFalse("can't tell yet, so it waits", expiry.check(read, focused = false))

        clock = 300_000
        visible = "seed words" // back in the wallet, clipboard unchanged
        assertTrue(expiry.check(read, focused = true))
    }

    @Test fun `something copied since is left alone`() {
        expiry.copied("seed words")
        clock = 60_000
        visible = null
        expiry.check(read, focused = false)

        visible = "a shopping list"
        assertFalse(expiry.check(read, focused = true))
        visible = "seed words"
        assertFalse("resolved: no later clear", expiry.check(read, focused = true))
    }

    @Test fun `nothing is cleared before the deadline`() {
        expiry.copied("seed words")
        visible = "seed words"
        clock = 59_999
        assertFalse(expiry.check(read, focused = true))
    }
}
