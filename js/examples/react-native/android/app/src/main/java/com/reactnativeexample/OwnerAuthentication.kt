package com.reactnativeexample

import android.app.KeyguardManager
import android.content.Context
import android.os.Build
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_WEAK
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import com.facebook.react.ReactPackage
import com.facebook.react.bridge.NativeModule
import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.bridge.UiThreadUtil
import com.facebook.react.uimanager.ViewManager

/**
 * Device-owner authentication, the iOS demo's `.deviceOwnerAuthentication`: a biometric first,
 * with the device's PIN, pattern or password as the fallback. Android 11 and later pair that
 * fallback with a strong biometric; Android 10 and earlier allow it beside a weak one only, the
 * nearest they come. A failed prompt rejects with the system's own message, as iOS shows
 * `localizedDescription`.
 */
class OwnerAuthenticationModule(context: ReactApplicationContext) : ReactContextBaseJavaModule(context) {
  override fun getName() = "OwnerAuthentication"

  /** Resolves `ok`, or `unavailable` on a device with no screen lock; rejects when declined. */
  @ReactMethod
  fun confirm(reason: String, promise: Promise) {
    val keyguard = reactApplicationContext.getSystemService(Context.KEYGUARD_SERVICE) as KeyguardManager
    // With the screen lock allowed as a fallback, a lock is all a prompt needs.
    if (!keyguard.isDeviceSecure) {
      promise.resolve("unavailable")
      return
    }
    val activity = reactApplicationContext.currentActivity as? FragmentActivity
    if (activity == null) {
      promise.reject("declined", "the app is not in front")
      return
    }
    UiThreadUtil.runOnUiThread {
      val prompt = BiometricPrompt(
        activity,
        ContextCompat.getMainExecutor(activity),
        object : BiometricPrompt.AuthenticationCallback() {
          override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
            promise.resolve("ok")
          }

          override fun onAuthenticationError(code: Int, message: CharSequence) {
            promise.reject("declined", message.toString())
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
    }
  }
}

class OwnerAuthenticationPackage : ReactPackage {
  override fun createNativeModules(context: ReactApplicationContext): List<NativeModule> =
    listOf(OwnerAuthenticationModule(context))

  override fun createViewManagers(context: ReactApplicationContext): List<ViewManager<*, *>> = emptyList()
}
