package org.fedimint.demo.wallet

/**
 * The one place a pasted or scanned code is turned into what the SDK parses:
 * trimmed, without a `lightning:` or `bitcoin:` scheme (in any case, as QR
 * codes often upper-case it) and without a BIP21 `?query`.
 */
object PaymentCodes {
    private val SCHEMES = setOf("lightning", "bitcoin")

    fun strip(raw: String): String {
        val text = raw.trim()
        val scheme = text.substringBefore(':', missingDelimiterValue = "").lowercase()
        return if (scheme in SCHEMES) text.substringAfter(':').substringBefore('?') else text
    }
}
