package org.fedimint.demo

import android.os.Bundle
import android.widget.Button
import android.widget.EditText
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.fedimint.sdk.ErrorCode
import org.fedimint.sdk.Federation
import org.fedimint.sdk.InviteCode
import org.fedimint.sdk.Sdk
import org.fedimint.sdk.createFedimintSdk
// The generated error class is `org.fedimint.sdk.Exception` (UniFFI's Kotlin
// backend maps every `*Error` to `*Exception`). Imported aliased so it does not
// shadow `kotlin.Exception`.
import org.fedimint.sdk.Exception as SdkException

/**
 * Drives the whole SDK surface in onboarding order: open storage (which
 * establishes the seed), show the seed, look at a federation, join it.
 *
 * The point of this app is to *run* the native library, not just link it:
 * opening exercises RocksDB and the storage lock, the seed exercises the
 * entropy source and a persisted write, and joining exercises the
 * root-secret derivation.
 */
class MainActivity : AppCompatActivity() {

    private var sdk: Sdk? = null
    private var federation: Federation? = null

    private lateinit var status: TextView
    private lateinit var mnemonicView: TextView
    private lateinit var output: TextView
    private lateinit var invite: EditText
    private lateinit var previewButton: Button
    private lateinit var joinButton: Button

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        status = findViewById(R.id.status)
        mnemonicView = findViewById(R.id.mnemonic)
        output = findViewById(R.id.output)
        invite = findViewById(R.id.invite)
        previewButton = findViewById(R.id.preview)
        joinButton = findViewById(R.id.join)

        status.text = "loaded libfedimint_sdk.so"

        findViewById<Button>(R.id.open).setOnClickListener { onOpen() }
        previewButton.setOnClickListener { onPreview() }
        joinButton.setOnClickListener { onJoin() }

        refreshButtons()
    }

    /**
     * Opens the SDK over the app-private directory and shows the seed.
     *
     * `null` for the mnemonic: `build()` generates and persists one when the
     * directory is empty, or loads the one already there. Pass
     * `Mnemonic.fromWords(userWords)` here instead to restore a wallet.
     */
    private fun onOpen() = reportAsync {
        val opened = createFedimintSdk(filesDir.path, null)
        sdk = opened
        // `exportMnemonic()` hands back an opaque `Mnemonic`; `words()` is the
        // deliberate step that takes the phrase out as plain strings.
        val words = opened.exportMnemonic().words()
        withContext(Dispatchers.Main) {
            mnemonicView.text = words
                .mapIndexed { i, w -> "${i + 1}. $w" }
                .chunked(3)
                .joinToString("\n") { row -> row.joinToString("  ") { it.padEnd(14) } }
        }
        "SDK open over ${filesDir.path} — seed: ${words.size} words"
    }

    private fun onPreview() = reportAsync {
        val sdk = sdk ?: return@reportAsync "open the SDK first"
        val preview = sdk.preview(InviteCode.parse(invite.text.toString().trim()))
        buildString {
            appendLine("id         ${preview.id}")
            appendLine("name       ${preview.name ?: "(unnamed)"}")
            appendLine("network    ${preview.network}")
            appendLine("guardians  ${preview.guardians}")
            appendLine("modules    ${preview.modules}")
            preview.meta.forEach { (k, v) -> appendLine("meta.$k = $v") }
        }
    }

    private fun onJoin() = reportAsync {
        val sdk = sdk ?: return@reportAsync "open the SDK first"
        // `join` returns a `Federation` handle. It has no methods yet — the
        // facades (balance, ecash, Lightning, ...) land in a later stage — so
        // for now the demo just holds it and reports that the join went through.
        federation?.close()
        federation = sdk.join(InviteCode.parse(invite.text.toString().trim()))
        "joined — Federation handle acquired"
    }

    /**
     * Runs a suspending SDK call off the main thread and renders whatever
     * comes back, including an [SdkException], which is what a real application
     * would branch on.
     *
     * Only `block` runs on `Dispatchers.IO`. Anything touching a view stays
     * on the main dispatcher `lifecycleScope` gives us — Android kills the
     * process for mutating a view from anywhere else.
     */
    private fun reportAsync(block: suspend () -> String) {
        output.text = "working…"
        lifecycleScope.launch {
            output.text = try {
                withContext(Dispatchers.IO) { block() }
            } catch (e: SdkException) {
                describe(e)
            }
            refreshButtons()
        }
    }

    /** Derives button state from what actually exists. Main thread only. */
    private fun refreshButtons() {
        val open = sdk != null
        previewButton.isEnabled = open
        joinButton.isEnabled = open
    }

    /**
     * Renders a failure the way an application should: branch on the stable
     * [ErrorCode], show the message only as a fallback.
     */
    private fun describe(e: SdkException): String = when (e.code()) {
        ErrorCode.INVALID_INPUT -> "That invite code isn't valid."
        ErrorCode.ALREADY_JOINED -> "Already joined this federation."
        ErrorCode.SEED_MISMATCH -> "That's a different seed than this wallet holds."
        ErrorCode.FEDERATION_UNREACHABLE -> "Couldn't reach the federation."
        ErrorCode.TIMEOUT -> "The federation didn't answer in time."
        ErrorCode.UNSUPPORTED_FEDERATION -> "This SDK can't work with that federation."
        ErrorCode.STORAGE_IN_USE -> "Another instance already holds this directory."
        else -> "${e.code()}"
    } + "\n\n[${e.code()}] ${e.reason()}"

    override fun onDestroy() {
        super.onDestroy()
        federation?.close()
        sdk?.close()
    }
}
