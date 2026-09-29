package org.fedimint.demo.ui.common

import java.text.NumberFormat
import org.fedimint.sdk.Amount
import org.fedimint.sdk.Network

/** Whole sats with grouping, e.g. 12,345. The SDK counts in msats; sub-sat remainders are dropped. */
fun formatSats(msats: Amount): String = NumberFormat.getIntegerInstance().format((msats / 1_000uL).toLong())

/** Msats left over after whole sats, or null when there are none worth showing. */
fun msatRemainder(msats: Amount): ULong? = (msats % 1_000uL).takeIf { it != 0uL }

fun Network.label(): String = when (this) {
    Network.BITCOIN -> "Bitcoin"
    Network.TESTNET -> "Testnet"
    Network.TESTNET4 -> "Testnet4"
    Network.SIGNET -> "Signet"
    Network.REGTEST -> "Regtest"
}
