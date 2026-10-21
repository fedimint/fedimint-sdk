package org.fedimint.demo.ui.common

import org.fedimint.sdk.ErrorCode
import org.fedimint.sdk.Exception as SdkException

/**
 * What to tell the user when a call fails. Branches on the stable [ErrorCode],
 * never on the message text, which is for logs and may change between releases.
 */
fun userMessage(e: Throwable): String = when (e) {
    is SdkException -> when (e.code()) {
        ErrorCode.INVALID_INPUT -> "That input isn't valid."
        ErrorCode.ALREADY_JOINED -> "You've already joined this federation."
        ErrorCode.SEED_MISMATCH -> "This wallet already holds a different recovery phrase."
        ErrorCode.FEDERATION_UNREACHABLE -> "Couldn't reach the federation."
        ErrorCode.TIMEOUT -> "The federation didn't answer in time."
        ErrorCode.UNSUPPORTED_FEDERATION -> "This app can't work with that federation."
        ErrorCode.STORAGE_IN_USE -> "The wallet is already open elsewhere."
        ErrorCode.NOT_SUPPORTED -> "This federation doesn't offer that."
        ErrorCode.QUOTE_EXPIRED -> "That quote expired. Get a new one."
        ErrorCode.QUOTE_CHANGED -> "The terms changed since the quote. Get a new one."
        ErrorCode.INSUFFICIENT_BALANCE -> "Not enough balance for that."
        ErrorCode.RECOVERING -> "Recovery is still running for this federation."
        ErrorCode.BALANCE_NOT_EMPTY -> "This federation still holds funds. Spend or move them first."
        ErrorCode.PENDING_OPERATIONS -> "This federation has payments still in progress. Wait for them to finish."
        ErrorCode.FEDERATION_CLOSED -> "This federation is closed. Reopen it first."
        else -> "Something went wrong (${e.code()})."
    }
    else -> e.message ?: "Something went wrong."
}
