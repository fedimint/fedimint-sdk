package org.fedimint.demo.wallet

import android.util.Log
import java.io.File

/**
 * Brings the wallet of an install that predates the reference wallet into its
 * directory.
 *
 * The one-screen demo this app replaced opened the SDK over `filesDir`
 * itself; the wallet opens `filesDir/wallet` (the debug harness has
 * `filesDir/harness`). An APK update keeps `filesDir`, so without this the
 * old seed, federations and history would sit where the wallet never looks,
 * and Welcome would offer to create another.
 *
 * The SDK keeps everything in `<dir>/db` (its `<dir>/LOCK` is only a claim,
 * recreated on open). So the move is one rename of that directory: atomic
 * within one filesystem, so a crash leaves either the old layout, adopted on
 * the next start, or the new one. It runs at launch before anything opens the
 * SDK ([WalletSession.prepare]), off the main thread, and never touches
 * anything else in `filesDir`. If it fails, launch says so and offers to try
 * again, rather than crash or offer a new wallet beside the old one.
 */
object LegacyWallet {
    private const val TAG = "LegacyWallet"
    private const val STORE = "db"

    /** Moves `filesDir/db` to `walletDir/db` if only the old one exists. Returns whether it moved. */
    fun adopt(filesDir: File, walletDir: File): Boolean {
        val legacy = File(filesDir, STORE)
        val current = File(walletDir, STORE)
        if (!legacy.isDirectory) return false
        if (current.exists()) {
            // Both exist: the wallet's own store wins, and the old one is left untouched.
            Log.w(TAG, "a store exists in both the old and the new location; keeping the new one")
            return false
        }
        check(walletDir.isDirectory || walletDir.mkdirs()) { "Couldn't make room for your existing wallet." }
        check(legacy.renameTo(current)) { "Couldn't move your existing wallet into place." }
        File(filesDir, "LOCK").delete()
        return true
    }
}
