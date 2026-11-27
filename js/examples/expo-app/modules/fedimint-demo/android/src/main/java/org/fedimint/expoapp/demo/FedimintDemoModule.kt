package org.fedimint.expoapp.demo

import android.app.KeyguardManager
import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.PersistableBundle
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_WEAK
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import com.facebook.react.bridge.ReactContext
import com.facebook.react.bridge.WindowFocusChangeListener
import expo.modules.kotlin.Promise
import expo.modules.kotlin.functions.Queues
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition

/**
 * `confirmOwner`: a biometric first, with the device's PIN, pattern or password as the fallback.
 * Android 11 and later pair that fallback with a strong biometric; Android 10 and earlier allow it
 * beside a weak one only. A failed prompt rejects with the system's own message.
 *
 * `copyBearer`: ecash notes, marked sensitive so the clipboard preview hides them, and cleared once
 * their lifetime is up if nothing else was copied since.
 *
 * Android 10 and later let only the app whose window has focus read the clipboard, and reading it
 * is how "nothing else was copied since" is told. A lifetime that ends while the app is in the
 * background is therefore acted on when its window has focus again; a process killed before then
 * leaves the copy where it is.
 */
class FedimintDemoModule : Module(), WindowFocusChangeListener {
  private val main = Handler(Looper.getMainLooper())

  private val context: Context
    get() = requireNotNull(appContext.reactContext)

  private val clipboard: ClipboardManager
    get() = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager

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

  override fun definition() = ModuleDefinition {
    Name("FedimintDemo")

    OnCreate {
      (appContext.reactContext as? ReactContext)?.addWindowFocusChangeListener(this@FedimintDemoModule)
    }

    OnDestroy {
      main.removeCallbacks(expire)
      (appContext.reactContext as? ReactContext)?.removeWindowFocusChangeListener(this@FedimintDemoModule)
    }

    AsyncFunction("confirmOwner") { reason: String, promise: Promise ->
      val keyguard = context.getSystemService(Context.KEYGUARD_SERVICE) as KeyguardManager
      // With the screen lock allowed as a fallback, a lock is all a prompt needs.
      if (!keyguard.isDeviceSecure) {
        promise.resolve("unavailable")
        return@AsyncFunction
      }
      val activity = appContext.currentActivity as? FragmentActivity
      if (activity == null) {
        promise.reject("declined", "the app is not in front", null)
        return@AsyncFunction
      }
      val prompt = BiometricPrompt(
        activity,
        ContextCompat.getMainExecutor(activity),
        object : BiometricPrompt.AuthenticationCallback() {
          override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
            promise.resolve("ok")
          }

          override fun onAuthenticationError(code: Int, message: CharSequence) {
            promise.reject("declined", message.toString(), null)
          }
        },
      )
      val biometric = if (Build.VERSION.SDK_INT >= 30) BIOMETRIC_STRONG else BIOMETRIC_WEAK
      prompt.authenticate(
        BiometricPrompt.PromptInfo.Builder()
          .setTitle(reason)
          .setAllowedAuthenticators(biometric or DEVICE_CREDENTIAL)
          .build(),
      )
    }.runOnQueue(Queues.MAIN)

    AsyncFunction("copyBearer") { value: String, lifetime: Double ->
      val clip = ClipData.newPlainText(LABEL, value)
      clip.description.extras = PersistableBundle().apply {
        putBoolean(
          if (Build.VERSION.SDK_INT >= 33) ClipDescription.EXTRA_IS_SENSITIVE else "android.content.extra.IS_SENSITIVE",
          true,
        )
      }
      clipboard.setPrimaryClip(clip)
      copied = value
      copiedAt = if (Build.VERSION.SDK_INT >= 26) clipboard.primaryClipDescription?.timestamp else null
      expired = false
      focused = true
      main.removeCallbacks(expire)
      main.postDelayed(expire, (lifetime * 1000).toLong())
    }.runOnQueue(Queues.MAIN)
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
