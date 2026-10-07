package org.fedimint.demo.ui.common

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.view.WindowManager
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewmodel.compose.viewModel
import org.fedimint.demo.AppContainer
import org.fedimint.demo.FedimintApp

/**
 * A screen-scoped ViewModel built from the [AppContainer]: the manual-injection
 * counterpart of Hilt's `hiltViewModel()`.
 */
@Composable
inline fun <reified VM : ViewModel> appViewModel(crossinline create: (AppContainer) -> VM): VM {
    val container = (LocalContext.current.applicationContext as FedimintApp).container
    return viewModel { create(container) }
}

/**
 * Keeps this screen out of screenshots, screen recordings and the recent-apps
 * thumbnail while it is shown. For screens that display or take a recovery
 * phrase: anyone who sees those words owns the wallet. Required reading before
 * changing it or adding a sensitive screen: android/app/SECURITY.md.
 *
 * `FLAG_SECURE` belongs to the window, which every screen shares, and screens
 * overlap: going from Backup to Verify, Verify is shown before Backup is
 * disposed. So the flag is counted per window ([SecureFlagCounter]) and cleared
 * only when the last secure screen leaves, never by one leaving while another
 * is still on screen.
 */
@Composable
fun SecureScreen() {
    val activity = LocalContext.current.findActivity()
    DisposableEffect(activity) {
        val window = activity?.window ?: return@DisposableEffect onDispose {}
        val counter = secureCounters.getOrPut(activity) {
            SecureFlagCounter { secure ->
                if (secure) {
                    window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
                } else {
                    window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
                }
            }
        }
        counter.acquire()
        onDispose {
            counter.release()
            // The counter's callback holds the window, and the window its activity, so a
            // lingering entry would keep the activity alive (the weak key never clears).
            // Drop it with the last secure screen; a destroyed activity disposes its
            // screens, so this always runs.
            if (counter.idle) secureCounters.remove(activity)
        }
    }
}

/** One counter per activity (window), only while it shows a secure screen. Main thread only. */
private val secureCounters = HashMap<Activity, SecureFlagCounter>()

/**
 * Counts the secure screens currently shown in one window and turns the flag
 * on with the first and off with the last.
 */
class SecureFlagCounter(private val setSecure: (Boolean) -> Unit) {
    private var shown = 0

    /** No secure screen is shown: the counter can be discarded. */
    val idle: Boolean get() = shown == 0

    fun acquire() {
        if (shown++ == 0) setSecure(true)
    }

    fun release() {
        check(shown > 0) { "released more secure screens than were shown" }
        if (--shown == 0) setSecure(false)
    }
}

tailrec fun Context.findActivity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.findActivity()
    else -> null
}
