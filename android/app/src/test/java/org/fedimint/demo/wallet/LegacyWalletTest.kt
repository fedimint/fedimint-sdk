package org.fedimint.demo.wallet

import java.io.File
import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** An update from the old demo keeps its wallet: its store moves to where the wallet looks. */
class LegacyWalletTest {
    private val filesDir: File = Files.createTempDirectory("files").toFile()
    private val walletDir = File(filesDir, "wallet")

    private fun store(dir: File, marker: String) {
        File(dir, "db").mkdirs()
        File(dir, "db/CURRENT").writeText(marker)
    }

    @Test fun `an upgraded install's store moves into the wallet directory`() {
        store(filesDir, "old seed")
        File(filesDir, "LOCK").writeText("")
        File(filesDir, "profileInstalled").writeText("unrelated")

        assertTrue(LegacyWallet.adopt(filesDir, walletDir))

        assertEquals("old seed", File(walletDir, "db/CURRENT").readText())
        assertFalse("nothing left in the old place", File(filesDir, "db").exists())
        assertFalse("the stale claim goes", File(filesDir, "LOCK").exists())
        assertEquals("nothing else in filesDir is touched", "unrelated", File(filesDir, "profileInstalled").readText())
    }

    @Test fun `it runs once`() {
        store(filesDir, "old seed")
        assertTrue(LegacyWallet.adopt(filesDir, walletDir))
        assertFalse(LegacyWallet.adopt(filesDir, walletDir))
        assertEquals("old seed", File(walletDir, "db/CURRENT").readText())
    }

    @Test fun `a fresh install has nothing to move`() {
        assertFalse(LegacyWallet.adopt(filesDir, walletDir))
        assertFalse(walletDir.exists())
    }

    @Test fun `the wallet's own store wins and the old one is left alone`() {
        store(filesDir, "old seed")
        store(walletDir, "current seed")

        assertFalse(LegacyWallet.adopt(filesDir, walletDir))
        assertEquals("current seed", File(walletDir, "db/CURRENT").readText())
        assertEquals("old seed", File(filesDir, "db/CURRENT").readText())
    }

    @Test fun `the debug harness's directory is not mistaken for a wallet`() {
        store(File(filesDir, "harness"), "harness seed")
        assertFalse(LegacyWallet.adopt(filesDir, walletDir))
    }
}
