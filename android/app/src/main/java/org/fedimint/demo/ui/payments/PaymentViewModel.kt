package org.fedimint.demo.ui.payments

import android.util.Log
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.sdk.Timestamp
import org.fedimint.sdk.Exception as SdkException

/**
 * What every send and receive screen shares: one [UiState] that moves through
 *
 *     input → (quote → review) → execute → follow the operation
 *
 * Subclasses supply the SDK calls; this class runs them one at a time, turns
 * failures into messages, follows the operation's states, and releases every
 * SDK handle it was given when the screen goes away.
 */
abstract class PaymentViewModel : ViewModel() {
    /** A quote the user must approve before anything is sent. */
    data class Review(val rows: List<Pair<String, String>>, val total: String, val expiresAtMillis: Long?)

    /** Something to hand to the other party: an invoice, an address, notes. */
    data class Output(val label: String, val text: String, val qr: String? = text)

    data class UiState(
        val working: Boolean = false,
        val error: String? = null,
        val review: Review? = null,
        val output: Output? = null,
        val progress: OpProgress? = null,
    ) {
        /** Once an operation exists, the inputs are locked: this screen is now following it. */
        val started: Boolean get() = progress != null || output != null
    }

    protected val mutableState = MutableStateFlow(UiState())
    val state = mutableState.asStateFlow()

    private val handles = mutableListOf<AutoCloseable>()

    /** Registers an SDK handle to be closed with this ViewModel. */
    protected fun <T : AutoCloseable> T.owned(): T = also { synchronized(handles) { handles += it } }

    /** Runs one step, unless one is already running. Failures land in [UiState.error]. */
    protected fun step(block: suspend () -> Unit) {
        if (mutableState.value.working) return
        mutableState.update { it.copy(working = true, error = null) }
        viewModelScope.launch {
            val error = attempt { block() }.exceptionOrNull()
            // The screen shows a message chosen by error code; the SDK's own reason goes to the log.
            error?.let { Log.w(TAG, "payment step failed: ${describe(it)}", it) }
            mutableState.update { it.copy(working = false, error = error?.let(::userMessage)) }
        }
    }

    /** Mirrors an operation's states into [UiState.progress] until it is final. */
    protected fun <S> follow(states: Flow<S>, describe: (S) -> OpProgress) {
        viewModelScope.launch {
            states
                .catch { e -> mutableState.update { it.copy(error = userMessage(e)) } }
                .collect { s -> mutableState.update { it.copy(progress = describe(s)) } }
        }
    }

    /** Drops an unapproved quote, e.g. because the input it was made for changed. */
    protected fun clearReview() = mutableState.update { it.copy(review = null, error = null) }

    override fun onCleared() {
        synchronized(handles) { handles.forEach { runCatching { it.close() } } }
    }

    protected companion object {
        private const val TAG = "Payments"

        fun expiry(ts: Timestamp): Long = ts.toLong()

        private fun describe(e: Throwable): String =
            if (e is SdkException) "[${e.code()}] ${e.reason()}" else e.toString()
    }
}
