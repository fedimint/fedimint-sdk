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
 * phrase: anyone who sees those words owns the wallet.
 */
@Composable
fun SecureScreen() {
    val activity = LocalContext.current.findActivity()
    DisposableEffect(activity) {
        activity?.window?.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        onDispose { activity?.window?.clearFlags(WindowManager.LayoutParams.FLAG_SECURE) }
    }
}

tailrec fun Context.findActivity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.findActivity()
    else -> null
}
