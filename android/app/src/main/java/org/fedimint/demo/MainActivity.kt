package org.fedimint.demo

import android.content.ClipData
import android.content.ClipboardManager
import android.os.Bundle
import android.view.View
import android.widget.Button
import android.widget.EditText
import android.widget.TextView
import android.widget.Toast
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import java.time.Instant
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.fedimint.sdk.EcashReceiveState
import org.fedimint.sdk.ErrorCode
import org.fedimint.sdk.Federation
import org.fedimint.sdk.InviteCode
import org.fedimint.sdk.LnQuote
import org.fedimint.sdk.Mnemonic
import org.fedimint.sdk.Notes
import org.fedimint.sdk.OnchainQuote
import org.fedimint.sdk.Sdk
import org.fedimint.sdk.createFedimintSdk
// The generated error class is `org.fedimint.sdk.Exception` (UniFFI's Kotlin
// backend maps every `*Error` to `*Exception`). Imported aliased so it does not
// shadow `kotlin.Exception`.
import org.fedimint.sdk.Exception as SdkException

/** The mutinynet federation js/examples/vite-core pre-fills, so both demos start from the same place. */
private const val TESTNET_FEDERATION_CODE =
    "fed11qgqrgvnhwden5te0v9k8q6rp9ekh2arfdeukuet595cr2ttpd3jhq6rzve6zuer9wchxvetyd938gcewvdhk6tcqqysptkuvknc7erjgf4em3zfh90kffqf9srujn6q53d6r056e4apze5cw27h75"

/**
 * Every SDK action on one scrolling screen, section by section, mirroring
 * js/examples/vite-core: wallet status, join, generate an invoice, redeem and
 * send ecash, pay lightning, parse an invite code, deposit, send on-chain, and
 * recent activity.
 *
 * The point of this app is to *run* the native library against a real
 * federation, not just link it. Every call here is one this SDK's `uniffi`
 * feature exports; none of it is hand-written glue over the native library.
 * That includes loading it: the generated bindings load the library
 * themselves, the same way for this app as for any other, so nothing here
 * has to happen first for the SDK to work.
 *
 * Threading: each section reads its inputs on the main thread, runs the SDK
 * call on `Dispatchers.IO`, and writes its result back on the main thread.
 */
class MainActivity : AppCompatActivity() {

    private var sdk: Sdk? = null
    private var federation: Federation? = null
    private var balanceJob: Job? = null

    // Quotes are single use: held between the Quote and Send/Pay taps so the
    // user sees the fee before committing, then dropped once submitted.
    private var lnQuote: LnQuote? = null
    private var onchainQuote: OnchainQuote? = null

    private var lastInvoice: String? = null
    private var lastNotes: Notes? = null
    private var lastAddress: String? = null

    private lateinit var walletStatus: TextView
    private lateinit var balance: TextView
    private lateinit var restoreWords: EditText
    private lateinit var toggleSeed: Button
    private lateinit var refreshBalance: Button
    private lateinit var seed: TextView
    private lateinit var invite: EditText
    private lateinit var lnReceiveCopy: Button
    private lateinit var ecashSendCopy: Button
    private lateinit var lnPay: Button
    private lateinit var depositCopy: Button
    private lateinit var onchainSend: Button

    /** Buttons that need an open SDK, and those that also need a joined federation. */
    private lateinit var needsSdk: List<Button>
    private lateinit var needsFederation: List<Button>

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        walletStatus = findViewById(R.id.walletStatus)
        balance = findViewById(R.id.balance)
        restoreWords = findViewById(R.id.restoreWords)
        toggleSeed = findViewById(R.id.toggleSeed)
        refreshBalance = findViewById(R.id.refreshBalance)
        seed = findViewById(R.id.seed)
        invite = findViewById(R.id.invite)
        lnReceiveCopy = findViewById(R.id.lnReceiveCopy)
        ecashSendCopy = findViewById(R.id.ecashSendCopy)
        lnPay = findViewById(R.id.lnPay)
        depositCopy = findViewById(R.id.depositCopy)
        onchainSend = findViewById(R.id.onchainSend)

        invite.setText(TESTNET_FEDERATION_CODE)

        findViewById<Button>(R.id.openWallet).setOnClickListener { onOpenWallet() }
        toggleSeed.setOnClickListener { onToggleSeed() }
        refreshBalance.setOnClickListener { onRefreshBalance() }
        findViewById<Button>(R.id.preview).setOnClickListener { onPreview() }
        findViewById<Button>(R.id.join).setOnClickListener { onJoin() }
        findViewById<Button>(R.id.recover).setOnClickListener { onRecover() }
        findViewById<Button>(R.id.lnReceive).setOnClickListener { onLnReceive() }
        lnReceiveCopy.setOnClickListener { lastInvoice?.let { copy("invoice", it) } }
        findViewById<Button>(R.id.ecashReceive).setOnClickListener { onEcashReceive() }
        findViewById<Button>(R.id.ecashSend).setOnClickListener { onEcashSend() }
        ecashSendCopy.setOnClickListener { lastNotes?.let { copy("ecash notes", it.display()) } }
        findViewById<Button>(R.id.lnQuote).setOnClickListener { onLnQuote() }
        lnPay.setOnClickListener { onLnPay() }
        findViewById<Button>(R.id.parseInviteButton).setOnClickListener { onParseInvite() }
        findViewById<Button>(R.id.deposit).setOnClickListener { onDeposit() }
        depositCopy.setOnClickListener { lastAddress?.let { copy("address", it) } }
        findViewById<Button>(R.id.onchainQuote).setOnClickListener { onOnchainQuote() }
        onchainSend.setOnClickListener { onOnchainSend() }
        findViewById<Button>(R.id.activity).setOnClickListener { onActivity() }

        needsSdk = listOf(R.id.preview, R.id.join, R.id.recover).map { findViewById(it) }
        needsFederation = listOf(
            R.id.lnReceive, R.id.ecashReceive, R.id.ecashSend, R.id.lnQuote,
            R.id.deposit, R.id.onchainQuote, R.id.activity,
        ).map { findViewById(it) }

        refreshState()
    }

    // ── Wallet ───────────────────────────────────────────────────────────

    /**
     * Opens the SDK over the app-private directory. With no words, `null`
     * loads the seed already there or generates and persists one over an
     * empty directory; with words, `Mnemonic.fromWords` restores that seed
     * (and fails with SEED_MISMATCH if the directory already holds another).
     *
     * Once open, it reattaches to the first federation this storage has
     * joined, so a joined federation persists across app restarts.
     */
    private fun onOpenWallet() {
        val words = restoreWords.text.toString().trim().split(Regex("\\s+")).filter { it.isNotEmpty() }
        section(R.id.walletResult) {
            if (sdk != null) return@section "already open"
            val mnemonic = if (words.isEmpty()) null else Mnemonic.fromWords(words)
            val opened = createFedimintSdk(filesDir.path, mnemonic)
            val existing = opened.federations().firstOrNull()
            withContext(Dispatchers.Main) {
                sdk = opened
                existing?.let { attach(it) }
            }
            if (existing != null) {
                "opened — reattached to ${existing.name() ?: existing.id()}"
            } else {
                "opened — join a federation below"
            }
        }
    }

    private fun onToggleSeed() {
        if (seed.visibility == View.VISIBLE) {
            seed.visibility = View.GONE
            toggleSeed.text = "Show seed"
            return
        }
        val sdk = sdk ?: return
        // `exportMnemonic()` hands back an opaque `Mnemonic`; `words()` is the
        // deliberate step that takes the phrase out as plain strings.
        seed.text = sdk.exportMnemonic().words()
            .mapIndexed { i, w -> "${i + 1}. $w" }
            .chunked(3)
            .joinToString("\n") { row -> row.joinToString("  ") { it.padEnd(14) } }
        seed.visibility = View.VISIBLE
        toggleSeed.text = "Hide seed"
    }

    private fun onRefreshBalance() {
        section(R.id.walletResult) {
            val federation = federation ?: return@section "join a federation first"
            val amount = federation.balance()
            withContext(Dispatchers.Main) { balance.text = "Balance: ${formatMsats(amount)}" }
            "balance refreshed"
        }
    }

    /**
     * Makes `joined` the current federation and keeps the balance line live
     * from its `balanceUpdates()` subscription. The previous handle is
     * released only now, once there is a new one to replace it. Main thread.
     */
    private fun attach(joined: Federation) {
        val previous = federation
        federation = joined
        balanceJob?.cancel()
        if (previous !== joined) previous?.close()
        lnQuote = null
        onchainQuote = null
        balance.text = "Balance: …"
        balanceJob = lifecycleScope.launch {
            try {
                val initial = withContext(Dispatchers.IO) { joined.balance() }
                balance.text = "Balance: ${formatMsats(initial)}"
                val updates = joined.balanceUpdates()
                while (true) {
                    val next = withContext(Dispatchers.IO) { updates.next() }
                    balance.text = "Balance: ${formatMsats(next)}"
                }
            } catch (e: SdkException) {
                balance.text = "Balance: unavailable (${e.code()})"
            }
        }
        refreshState()
    }

    // ── Join ─────────────────────────────────────────────────────────────

    private fun onPreview() {
        val inviteText = invite.text.toString().trim()
        section(R.id.joinResult) {
            val sdk = sdk ?: return@section "open the wallet first"
            val preview = sdk.preview(InviteCode.parse(inviteText))
            buildString {
                appendLine("Federation ID  ${preview.id}")
                appendLine("Name           ${preview.name ?: "(unnamed)"}")
                appendLine("Network        ${preview.network}")
                appendLine("Guardians      ${preview.guardians}")
                appendLine("Modules        ${preview.modules.joinToString()}")
                preview.meta.forEach { (k, v) -> appendLine("meta.$k = $v") }
            }
        }
    }

    /**
     * Joins the pasted federation, or reattaches to it if this storage already
     * joined it: ALREADY_JOINED is the cue to look the running federation up by
     * the id the invite code carries.
     */
    private fun onJoin() {
        val inviteText = invite.text.toString().trim()
        section(R.id.joinResult) {
            val sdk = sdk ?: return@section "open the wallet first"
            val code = InviteCode.parse(inviteText)
            val (joined, verb) = try {
                sdk.join(code) to "Joined"
            } catch (e: SdkException) {
                if (e.code() != ErrorCode.ALREADY_JOINED) throw e
                val existing = sdk.federation(code.federationId()) ?: throw e
                existing to "Already joined — reattached to"
            }
            withContext(Dispatchers.Main) { attach(joined) }
            describeFederation(verb, joined)
        }
    }

    /**
     * Joins with a seed-recovery rescan instead of a plain join: the path for a
     * restored seed that already holds funds in this federation. Spends and
     * receives are refused until the rescan completes.
     */
    private fun onRecover() {
        val inviteText = invite.text.toString().trim()
        val result = findViewById<TextView>(R.id.joinResult)
        section(R.id.joinResult) {
            val sdk = sdk ?: return@section "open the wallet first"
            val recovery = sdk.recover(InviteCode.parse(inviteText))
            val header = "Recovering ${recovery.federation.id()}"
            withContext(Dispatchers.Main) {
                attach(recovery.federation)
                watch(result, header, recovery.progress.updates()) { it.next() }
            }
            "$header\n\nstate: ${describeState(recovery.progress.state())}"
        }
    }

    // ── Lightning in ─────────────────────────────────────────────────────

    private fun onLnReceive() {
        val amountText = findViewById<EditText>(R.id.lnReceiveAmount).text.toString().trim()
        val description = findViewById<EditText>(R.id.lnReceiveDescription).text.toString()
        val result = findViewById<TextView>(R.id.lnReceiveResult)
        section(R.id.lnReceiveResult) {
            val lightning = federation?.lightning()
                ?: return@section "this federation has no lightning module"
            val msats = amountText.toULongOrNull() ?: return@section "enter an amount in msats"

            val receive = lightning.receive(msats, description)
            val header = "Invoice:\n${receive.invoice}\n\noperation ${receive.operation.id()}"
            withContext(Dispatchers.Main) {
                lastInvoice = receive.invoice
                lnReceiveCopy.isEnabled = true
                watch(result, header, receive.operation.updates()) { it.next() }
            }
            header
        }
    }

    // ── Ecash ────────────────────────────────────────────────────────────

    private fun onEcashReceive() {
        val notesText = findViewById<EditText>(R.id.ecashNotes).text.toString().trim()
        section(R.id.ecashReceiveResult) {
            val ecash = federation?.ecash() ?: return@section "this federation has no mint module"
            if (notesText.isEmpty()) return@section "paste ecash notes first"

            val notes = Notes.parse(notesText)
            val operation = ecash.receive(notes)
            // `awaitFinal` returns the terminal state, and `Failed` is one of
            // them: an operation failing is a state, while an exception means
            // the operation could not be observed. Only `Done` redeemed
            // anything.
            val outcome = when (val state = operation.awaitFinal()) {
                is EcashReceiveState.Done -> "Redeemed ${formatMsats(notes.value())}"
                is EcashReceiveState.Failed -> "Not redeemed: ${state.reason}"
                else -> "Ended in ${describeState(state)}"
            }
            "$outcome\n\noperation ${operation.id()}"
        }
    }

    /**
     * Ecash out: quote the amount, then send it. The notes are a bearer
     * instrument — hand them to a receiver out of band — and the operation
     * keeps tracking whether they were redeemed or reclaimed.
     */
    private fun onEcashSend() {
        val amountText = findViewById<EditText>(R.id.ecashSendAmount).text.toString().trim()
        section(R.id.ecashSendResult) {
            val ecash = federation?.ecash() ?: return@section "this federation has no mint module"
            val msats = amountText.toULongOrNull() ?: return@section "enter an amount in msats"

            val quote = ecash.quote(msats)
            val sent = ecash.send(quote)
            val state = sent.operation.state()
            withContext(Dispatchers.Main) {
                lastNotes = sent.notes
                ecashSendCopy.isEnabled = true
            }
            buildString {
                appendLine("notes ${formatMsats(quote.notesValue())} + fee ${formatMsats(quote.fee())}")
                appendLine()
                appendLine("hand these notes to the receiver:")
                // `notes` is an opaque handle, so logging `sent` never prints the
                // token; `display()` is the deliberate way to take it out.
                appendLine(sent.notes.display())
                appendLine()
                append("operation ${sent.operation.id()} — ${describeState(state)}")
            }
        }
    }

    // ── Lightning out ────────────────────────────────────────────────────

    /** Quotes a pasted invoice: amount, fee, route and expiry, before anything is paid. */
    private fun onLnQuote() {
        val invoiceText = findViewById<EditText>(R.id.lnInvoice).text.toString().trim()
        section(R.id.lnPayResult) {
            val lightning = federation?.lightning()
                ?: return@section "this federation has no lightning module"
            if (invoiceText.isEmpty()) return@section "paste an invoice first"

            val quote = lightning.quote(invoiceText)
            withContext(Dispatchers.Main) {
                lnQuote = quote
                lnPay.isEnabled = true
            }
            buildString {
                appendLine("Amount   ${formatMsats(quote.invoiceAmount())}")
                appendLine("Fee      ${formatMsats(quote.fee())}")
                appendLine("Total    ${formatMsats(quote.total())}")
                appendLine("Route    ${describeState(quote.route())}")
                append("Expires  ${Instant.ofEpochMilli(quote.expiresAt().toLong())}")
            }
        }
    }

    private fun onLnPay() {
        val quote = lnQuote ?: return
        val lightning = federation?.lightning() ?: return
        lnQuote = null
        lnPay.isEnabled = false
        section(R.id.lnPayResult) {
            val operation = lightning.send(quote)
            val state = operation.awaitFinal()
            "operation ${operation.id()}\nstate: ${describeState(state)}"
        }
    }

    // ── Parse invite code ────────────────────────────────────────────────

    private fun onParseInvite() {
        val inviteText = findViewById<EditText>(R.id.parseInvite).text.toString().trim()
        section(R.id.parseInviteResult) {
            val code = InviteCode.parse(inviteText)
            "Fed Id: ${code.federationId()}"
        }
    }

    // ── On-chain ─────────────────────────────────────────────────────────

    private fun onDeposit() {
        val result = findViewById<TextView>(R.id.depositResult)
        section(R.id.depositResult) {
            val onchain = federation?.onchain() ?: return@section "this federation has no wallet module"

            val receive = onchain.receive()
            val header = "Send bitcoin to:\n${receive.address}\n\noperation ${receive.operation.id()}"
            withContext(Dispatchers.Main) {
                lastAddress = receive.address
                depositCopy.isEnabled = true
                watch(result, header, receive.operation.updates()) { it.next() }
            }
            header
        }
    }

    private fun onOnchainQuote() {
        val amountText = findViewById<EditText>(R.id.onchainAmount).text.toString().trim()
        val addressText = findViewById<EditText>(R.id.onchainAddress).text.toString().trim()
        section(R.id.onchainSendResult) {
            val onchain = federation?.onchain() ?: return@section "this federation has no wallet module"
            val sats = amountText.toULongOrNull() ?: return@section "enter an amount in sats"
            if (addressText.isEmpty()) return@section "enter a destination address"

            val quote = onchain.quote(addressText, sats)
            withContext(Dispatchers.Main) {
                onchainQuote = quote
                onchainSend.isEnabled = true
            }
            buildString {
                appendLine("Amount   ${quote.amount()} sat")
                appendLine("Fee      ${formatMsats(quote.fee())}")
                appendLine("Total    ${formatMsats(quote.total())}")
                append("Expires  ${Instant.ofEpochMilli(quote.expiresAt().toLong())}")
            }
        }
    }

    private fun onOnchainSend() {
        val quote = onchainQuote ?: return
        val onchain = federation?.onchain() ?: return
        onchainQuote = null
        onchainSend.isEnabled = false
        val result = findViewById<TextView>(R.id.onchainSendResult)
        section(R.id.onchainSendResult) {
            val operation = onchain.send(quote)
            val header = "operation ${operation.id()}"
            withContext(Dispatchers.Main) {
                watch(result, header, operation.updates()) { it.next() }
            }
            "$header\nstate: ${describeState(operation.state())}"
        }
    }

    // ── Activity ─────────────────────────────────────────────────────────

    private fun onActivity() {
        section(R.id.activityResult) {
            val federation = federation ?: return@section "join a federation first"
            val page = federation.activity(null, 20u)
            if (page.items.isEmpty()) return@section "no activity yet"
            page.items.joinToString("\n\n") { item ->
                buildString {
                    appendLine("${Instant.ofEpochMilli(item.time.toLong())}")
                    append("${item.kind} ${item.direction ?: ""} ${item.status}")
                    item.amount?.let { append("  ${formatMsats(it)}") }
                    item.fee?.let { append("  fee ${formatMsats(it)}") }
                }
            }
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    /**
     * Runs a suspending SDK call off the main thread and renders what comes
     * back into one section's result line, including an [SdkException], which
     * is what a real application would branch on.
     *
     * Only `block` runs on `Dispatchers.IO`. Anything touching a view stays on
     * the main dispatcher — which is why every caller reads its input fields
     * before handing `block` over, and switches back to Main to update state.
     */
    private fun section(resultId: Int, block: suspend () -> String) {
        val result = findViewById<TextView>(resultId)
        result.text = "working…"
        lifecycleScope.launch {
            result.text = try {
                withContext(Dispatchers.IO) { block() }
            } catch (e: SdkException) {
                describe(e)
            }
            refreshState()
        }
    }

    /**
     * Follows an operation's state changes into `view` under `header`, until
     * the subscription ends (`next()` returns null once the state is final).
     *
     * Takes the subscriber rather than a factory, and takes it already
     * created: one subscription has one cursor, so calling `updates()` per
     * iteration would hand back a fresh subscriber that replays the current
     * state forever and never reaches the terminal `null`. It is closed once
     * the loop ends, including when the scope is cancelled.
     */
    private fun <T : AutoCloseable> watch(
        view: TextView,
        header: String,
        subscriber: T,
        next: suspend (T) -> Any?,
    ) {
        lifecycleScope.launch {
            try {
                while (true) {
                    val state = withContext(Dispatchers.IO) { next(subscriber) } ?: break
                    view.text = "$header\n\nstate: ${describeState(state)}"
                }
            } catch (e: SdkException) {
                view.text = "$header\n\n${describe(e)}"
            } finally {
                subscriber.close()
            }
        }
    }

    /** Enables each button only once what it needs exists. Main thread only. */
    private fun refreshState() {
        val sdk = sdk
        val federation = federation
        needsSdk.forEach { it.isEnabled = sdk != null }
        needsFederation.forEach { it.isEnabled = federation != null }
        toggleSeed.isEnabled = sdk != null
        refreshBalance.isEnabled = federation != null
        walletStatus.text = when {
            sdk == null -> "SDK not open"
            federation == null -> "SDK open — no federation joined"
            else -> describeFederation("Federation", federation) + "\nid ${federation.id()}"
        }
    }

    private fun describeFederation(verb: String, federation: Federation): String {
        val caps = federation.capabilities()
        return "$verb ${federation.name() ?: "(unnamed)"} on ${federation.network()}\n" +
            "ecash ${caps.ecash} · lightning ${caps.lightning} · onchain ${caps.onchain}"
    }

    private fun copy(label: String, text: String) {
        getSystemService(ClipboardManager::class.java)
            .setPrimaryClip(ClipData.newPlainText(label, text))
        Toast.makeText(this, "Copied $label", Toast.LENGTH_SHORT).show()
    }

    private fun formatMsats(msats: ULong): String {
        val sats = msats / 1_000uL
        return if (msats % 1_000uL == 0uL) "$sats sat" else "$sats sat ($msats msat)"
    }

    /**
     * Renders any operation state or route readably.
     *
     * A state that carries no data (`Created`, `Internal`, ...) generates as a
     * plain Kotlin `object`, whose default `toString()` is a bare class hash
     * rather than its name; a true Kotlin `enum` already prints its own name.
     * `::class.simpleName` gives the clean label either way, and the explicit
     * branches below add the fields worth showing.
     */
    private fun describeState(state: Any?): String = when (state) {
        null -> "null"
        is Enum<*> -> state.name
        is org.fedimint.sdk.EcashReceiveState.Failed -> "Failed(${state.reason})"
        is org.fedimint.sdk.LnSendState.Success -> "Success(fee=${state.fee})"
        is org.fedimint.sdk.LnSendState.Failed -> "Failed(${state.reason})"
        is org.fedimint.sdk.LnReceiveState.Canceled -> "Canceled(${state.reason})"
        is org.fedimint.sdk.OnchainSendState.Succeeded -> "Succeeded(txid=${state.txid})"
        is org.fedimint.sdk.OnchainSendState.Refunded -> "Refunded(${state.reason})"
        is org.fedimint.sdk.OnchainSendState.Failed -> "Failed(${state.reason})"
        is org.fedimint.sdk.OnchainReceiveState.Failed -> "Failed(${state.reason})"
        is org.fedimint.sdk.RecoveryState.Failed -> "Failed(${state.reason})"
        is org.fedimint.sdk.LightningRoute.Gateway -> "Gateway(${state.gatewayId})"
        else -> state::class.simpleName ?: state.toString()
    }

    /**
     * Renders a failure the way an application should: branch on the stable
     * [ErrorCode], show the message only as detail.
     */
    private fun describe(e: SdkException): String = when (e.code()) {
        ErrorCode.INVALID_INPUT -> "That input isn't valid."
        ErrorCode.ALREADY_JOINED -> "Already joined this federation."
        ErrorCode.SEED_MISMATCH -> "That's a different seed than this wallet holds."
        ErrorCode.FEDERATION_UNREACHABLE -> "Couldn't reach the federation."
        ErrorCode.TIMEOUT -> "The federation didn't answer in time."
        ErrorCode.UNSUPPORTED_FEDERATION -> "This SDK can't work with that federation."
        ErrorCode.STORAGE_IN_USE -> "Another instance already holds this directory."
        ErrorCode.NOT_SUPPORTED -> "This federation doesn't offer that."
        ErrorCode.QUOTE_EXPIRED -> "That quote is no longer valid — quote again."
        ErrorCode.QUOTE_CHANGED -> "The terms changed since the quote — quote again."
        ErrorCode.INSUFFICIENT_BALANCE -> "Not enough balance for that."
        ErrorCode.RECOVERING -> "Recovery is still running for this federation."
        else -> "${e.code()}"
    } + "\n\n[${e.code()}] ${e.reason()}"

    override fun onDestroy() {
        super.onDestroy()
        balanceJob?.cancel()
        federation?.close()
        sdk?.close()
    }
}
