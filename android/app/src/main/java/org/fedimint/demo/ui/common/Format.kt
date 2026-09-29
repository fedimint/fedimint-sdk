package org.fedimint.demo.ui.common

import android.text.format.DateUtils
import java.text.NumberFormat
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import org.fedimint.sdk.Amount
import org.fedimint.sdk.Network
import org.fedimint.sdk.Timestamp

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

/** "5 minutes ago", "Yesterday": for lists. SDK timestamps are Unix milliseconds. */
fun relativeTime(ts: Timestamp): String =
    DateUtils.getRelativeTimeSpanString(ts.toLong(), System.currentTimeMillis(), DateUtils.MINUTE_IN_MILLIS).toString()

/** A full local date and time: for detail screens. */
fun absoluteTime(ts: Timestamp): String =
    DateTimeFormatter.ofLocalizedDateTime(FormatStyle.MEDIUM, FormatStyle.SHORT)
        .withZone(ZoneId.systemDefault())
        .format(Instant.ofEpochMilli(ts.toLong()))
