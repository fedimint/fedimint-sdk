package org.fedimint.demo.ui.common

import kotlinx.coroutines.CancellationException

/**
 * [runCatching] for coroutines: a failure becomes a [Result], but cancellation
 * is rethrown, so leaving a screen mid-call cancels the call instead of
 * surfacing as an error.
 */
inline fun <T> attempt(block: () -> T): Result<T> = try {
    Result.success(block())
} catch (e: CancellationException) {
    throw e
} catch (e: Exception) {
    Result.failure(e)
}
