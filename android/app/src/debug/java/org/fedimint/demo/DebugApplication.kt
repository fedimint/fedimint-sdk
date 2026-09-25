package org.fedimint.demo

import android.app.Application
import android.content.Context
import org.fedimint.sdk.Mnemonic

/**
 * Loads the SDK before this process's `Application` is in place, so every
 * debug build (the one the Appium suite drives) starts in the hardest order
 * for the SDK's Android setup.
 *
 * The SDK publishes the Android context its DNS resolver needs by looking up
 * the process's `Application`, which Android assigns only after
 * `attachBaseContext` returns. Calling any binding here loads the native
 * library, and runs its `JNI_OnLoad`, before that lookup can succeed. The SDK
 * is meant to recover by publishing when `createFedimintSdk` runs instead; the
 * Lightning test then dials an iroh gateway, and run-android-e2e.sh fails the
 * run unless the resolver read the device's DNS servers. So an SDK that only
 * works when loaded late fails CI instead of passing it.
 *
 * Debug only, through src/debug's manifest: the release build of the demo, and
 * any app using the SDK, never touches this class.
 */
class DebugApplication : Application() {
    override fun attachBaseContext(base: Context) {
        super.attachBaseContext(base)
        // Any synchronous binding would do; this one needs no input.
        Mnemonic.generate().close()
    }
}
