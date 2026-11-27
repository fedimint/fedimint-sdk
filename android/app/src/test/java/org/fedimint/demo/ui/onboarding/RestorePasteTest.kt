package org.fedimint.demo.ui.onboarding

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Pasting a phrase replaces every box and picks the box count, leaving nothing stale. */
class RestorePasteTest {
    private val twelve = (1..12).joinToString(" ") { "word$it" }

    @Test fun `a 12-word paste gives 12 boxes, even after choosing 24`() {
        val words = RestoreViewModel.pasted(twelve)!!
        assertEquals(12, words.size)
        assertEquals("word12", words.last())
    }

    @Test fun `a longer paste gives 24 boxes, the rest left empty`() {
        val words = RestoreViewModel.pasted("$twelve extra1 extra2")!!
        assertEquals(24, words.size)
        assertEquals("extra2", words[13])
        assertEquals("", words[14])
    }

    @Test fun `words are lower-cased and blank pastes ignored`() {
        assertEquals("orbit", RestoreViewModel.pasted("  ORBIT\nmaple ")!!.first())
        assertNull(RestoreViewModel.pasted("   "))
    }
}
