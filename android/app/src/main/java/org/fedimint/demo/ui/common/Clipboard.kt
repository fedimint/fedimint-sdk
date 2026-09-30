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
 * Copies a secret (a recovery phrase) with the precautions a wallet owes it:
 *
 * - flagged sensitive, so Android 13+ doesn't show it in the clipboard preview
 *   or suggestion strip;
 * - cleared after [clearAfterMillis], but only if the clipboard still holds it,
 *   so something the user copied since is left alone.
 *
 * The timer runs on the main looper rather than a screen's scope, so leaving
 * the screen doesn't leave the secret behind.
 */
fun copySecret(context: Context, label: String, secret: String, clearAfterMillis: Long = 60_000) {
    val clipboard = context.getSystemService(ClipboardManager::class.java)
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
    clipboard.setPrimaryClip(clip)

    Handler(Looper.getMainLooper()).postDelayed({
        val current = runCatching { clipboard.primaryClip?.getItemAt(0)?.text?.toString() }.getOrNull()
        if (current == secret) clipboard.clearPrimaryClip()
    }, clearAfterMillis)
}
