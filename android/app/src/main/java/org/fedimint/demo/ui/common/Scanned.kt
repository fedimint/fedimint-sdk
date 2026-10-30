package org.fedimint.demo.ui.common

import org.fedimint.demo.wallet.PaymentCodes

/**
 * What a scanned (or pasted) code is, judged by its form only. The SDK still
 * validates it on the screen it is handed to.
 */
sealed interface Scanned {
    val text: String

    data class Invoice(override val text: String) : Scanned
    data class Address(override val text: String) : Scanned
    data class Invite(override val text: String) : Scanned

    /** Anything else is tried as ecash notes. */
    data class Notes(override val text: String) : Scanned

    companion object {
        fun classify(raw: String): Scanned {
            val text = raw.trim()
            val lower = text.lowercase()
            return when {
                lower.startsWith("lightning:") -> Invoice(PaymentCodes.strip(text))
                lower.startsWith("lnbc") || lower.startsWith("lntb") || lower.startsWith("lntbs") || lower.startsWith("lnbcrt") ->
                    Invoice(text)
                lower.startsWith("bitcoin:") -> Address(PaymentCodes.strip(text))
                lower.startsWith("fed1") -> Invite(text)
                BECH32.matches(text) || BASE58.matches(text) -> Address(text)
                else -> Notes(text)
            }
        }

        /** Segwit addresses, in either case: QR codes carry them upper-cased. */
        private val BECH32 = Regex("^(bc1|tb1|bcrt1)[a-z0-9]{8,87}$", RegexOption.IGNORE_CASE)

        /** Legacy and P2SH addresses, mainnet and test networks. */
        private val BASE58 = Regex("^[13mn2][a-km-zA-HJ-NP-Z1-9]{25,34}$")
    }
}

/**
 * Hands a scanned code from Home to the screen that uses it.
 *
 * In memory and taken once, never a navigation argument: ecash notes are a
 * bearer instrument, and navigation arguments end up in saved state (see
 * SECURITY.md, rule 2). Lost if the process dies in between, which only means
 * scanning again.
 *
 * A code expires after [TTL_MILLIS]: one its screen didn't use (an unsupported
 * rail, say) must not fill a Send screen the user opens later by hand.
 */
object ScanHandoff {
    const val TTL_MILLIS = 10_000L

    private var pending: Scanned? = null
    private var putAt = 0L

    fun put(scanned: Scanned, now: Long = System.currentTimeMillis()) {
        pending = scanned
        putAt = now
    }

    /** The pending code, if it is a [T] and still fresh; it is cleared once read as one. */
    inline fun <reified T : Scanned> take(now: Long = System.currentTimeMillis()): T? =
        peek(now).let { it as? T }?.also { clear() }

    /** The pending code if still fresh; an expired one is dropped. */
    fun peek(now: Long = System.currentTimeMillis()): Scanned? {
        if (pending != null && now - putAt > TTL_MILLIS) clear()
        return pending
    }

    fun clear() {
        pending = null
    }
}
