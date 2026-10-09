package org.fedimint.demo.ui.common

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.PersistableBundle

/**
 * Copies a secret (a recovery phrase) with the precautions a wallet owes it
 * (android/app/SECURITY.md, rule 4):
 *
 * - flagged sensitive, so Android 13+ doesn't show it in the clipboard preview
 *   or suggestion strip;
 * - cleared once [ClipboardExpiry.TTL_MILLIS] has passed, but only if the
 *   clipboard still holds it, so something the user copied since is left alone.
 *
 * Android only lets the app with focus read the clipboard, and the expected
 * flow is to copy and switch to a password manager. So the check at the
 * deadline may be unable to read it; it then waits, and runs again when the
 * wallet regains focus ([onFocus], from MainActivity). The phrase can stay on
 * the clipboard past the deadline while the user is elsewhere, and if Android
 * ends the process in the meantime it is not cleared at all.
 */
object SecretClipboard {
    private val expiry = ClipboardExpiry(now = System::currentTimeMillis)

    fun copy(context: Context, label: String, secret: String) {
        val app = context.applicationContext
        val clip = ClipData.newPlainText(label, secret).apply {
            description.extras = PersistableBundle().apply {
                val key = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    ClipDescription.EXTRA_IS_SENSITIVE
                } else {
                    "android.content.extra.IS_SENSITIVE"
                }
                putBoolean(key, true)
            }
        }
        app.getSystemService(ClipboardManager::class.java).setPrimaryClip(clip)
        expiry.copied(secret)
        // On the main looper rather than a screen's scope, so leaving the screen doesn't cancel it.
        Handler(Looper.getMainLooper()).postDelayed({ check(app, focused = false) }, ClipboardExpiry.TTL_MILLIS)
    }

    /** The wallet's window gained focus: a pending expiry can now read the clipboard. */
    fun onFocus(context: Context) = check(context.applicationContext, focused = true)

    private fun check(context: Context, focused: Boolean) {
        val clipboard = context.getSystemService(ClipboardManager::class.java)
        val read = { runCatching { clipboard.primaryClip?.getItemAt(0)?.text?.toString() }.getOrNull() }
        if (expiry.check(read, focused)) clipboard.clearPrimaryClip()
    }
}

/**
 * When a copied secret must leave the clipboard. Pure logic: the clock and the
 * clipboard read are passed in.
 */
class ClipboardExpiry(private val now: () -> Long, private val ttlMillis: Long = TTL_MILLIS) {
    private data class Pending(val secret: String, val deadline: Long)

    private var pending: Pending? = null

    fun copied(secret: String) {
        pending = Pending(secret, now() + ttlMillis)
    }

    /**
     * Whether to clear the clipboard now. [read] returns its text, or null
     * when it is empty or (without focus) unreadable. Without [focused], a null
     * read decides nothing: the expiry stays pending for the next check.
     */
    fun check(read: () -> String?, focused: Boolean): Boolean {
        val p = pending ?: return false
        if (now() < p.deadline) return false
        val current = read()
        if (current == null && !focused) return false
        pending = null
        return current == p.secret
    }

    companion object {
        const val TTL_MILLIS = 60_000L
    }
}

/** Copies a recovery phrase; see [SecretClipboard]. */
fun copySecret(context: Context, label: String, secret: String) = SecretClipboard.copy(context, label, secret)
