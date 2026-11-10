package com.reactnativeexample

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.PersistableBundle
import com.facebook.react.ReactPackage
import com.facebook.react.bridge.NativeModule
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.bridge.WindowFocusChangeListener
import com.facebook.react.uimanager.ViewManager

/**
 * Copies bearer value, ecash notes. Android has no cross-device clipboard to keep it off, so the
 * copy is marked sensitive, which keeps it out of the clipboard preview, and is cleared once its
 * lifetime is up if nothing else was copied since: the iOS demo's local-only, expiring copy as
 * near as Android allows.
 *
 * Android 10 and later let only the app whose window has focus read the clipboard, and reading it
 * is how "nothing else was copied since" is told. A lifetime that ends while the app is in the
 * background is therefore acted on when its window has focus again; a process killed before then
 * leaves the copy where it is.
 */
class BearerPasteboardModule(context: ReactApplicationContext) :
  ReactContextBaseJavaModule(context), WindowFocusChangeListener {
  private val main = Handler(Looper.getMainLooper())
  private val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
  /** The value copied last, until it is cleared or found replaced. */
  private var copied: String? = null
  /** When the system stamped that copy, Android 8 and later, to tell it from a later one. */
  private var copiedAt: Long? = null
  private var expired = false
  /** Copying takes a tap in the app, so its window has focus then. */
  private var focused = true
  private val expire = Runnable {
    expired = true
    clearIfStillCopied()
  }

  init {
    context.addWindowFocusChangeListener(this)
  }

  override fun getName() = "BearerPasteboard"

  @ReactMethod
  fun copy(value: String, lifetime: Double) {
    val clip = ClipData.newPlainText(LABEL, value)
    clip.description.extras = PersistableBundle().apply {
      putBoolean(
        if (Build.VERSION.SDK_INT >= 33) ClipDescription.EXTRA_IS_SENSITIVE else "android.content.extra.IS_SENSITIVE",
        true,
      )
    }
    main.post {
      clipboard.setPrimaryClip(clip)
      copied = value
      copiedAt = if (Build.VERSION.SDK_INT >= 26) clipboard.primaryClipDescription?.timestamp else null
      expired = false
      focused = true
      main.removeCallbacks(expire)
      main.postDelayed(expire, (lifetime * 1000).toLong())
    }
  }

  private fun clearIfStillCopied() {
    val value = copied ?: return
    if (!expired || !focused) return
    // Null for an empty clipboard too: kept for the next focus, where clearing an empty
    // clipboard again is harmless.
    val description = clipboard.primaryClipDescription ?: return
    copied = null
    // Told by the clip's description, its label and the time it was copied, not its text:
    // from Android 12, reading another app's clip shows "pasted from your clipboard". Before
    // Android 8 there is no time, and before Android 12 no notice, so the text is compared.
    val stillOurs =
      description.label == LABEL &&
        if (Build.VERSION.SDK_INT >= 26) description.timestamp == copiedAt
        else clipboard.primaryClip?.getItemAt(0)?.text?.toString() == value
    if (stillOurs) {
      if (Build.VERSION.SDK_INT >= 28) clipboard.clearPrimaryClip()
      else clipboard.setPrimaryClip(ClipData.newPlainText("", ""))
    }
  }

  override fun onWindowFocusChange(hasFocus: Boolean) {
    focused = hasFocus
    clearIfStillCopied()
  }

  private companion object {
    const val LABEL = "ecash notes"
  }
}

class BearerPasteboardPackage : ReactPackage {
  override fun createNativeModules(context: ReactApplicationContext): List<NativeModule> =
    listOf(BearerPasteboardModule(context))

  override fun createViewManagers(context: ReactApplicationContext): List<ViewManager<*, *>> = emptyList()
}
