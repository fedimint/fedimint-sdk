package org.fedimint.demo.ui.payments

import android.util.Log
import androidx.lifecycle.SavedStateHandle
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
import org.fedimint.demo.wallet.History
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.OperationId
import org.fedimint.sdk.OperationKind
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
 *
 * Two rules keep money from moving twice (see android/app/SECURITY.md):
 *
 * - An operation is recorded ([UiState.operationId]) the moment the SDK call
 *   that creates it returns, not when its first state update arrives. From
 *   then on the inputs stay locked, whatever happens to the observation.
 * - A quote is single use and the SDK consumes it even on a failed send, so a
 *   failed [execute] drops the review: the next tap fetches a fresh quote for
 *   the same inputs instead of resubmitting a spent one.
 *
 * The operation id (not secret) is also kept in [savedState], because Android
 * can kill the process in the background and later restore the screen, inputs
 * and all, with a new ViewModel. That ViewModel starts locked on the restored
 * operation ([UiState.restored]) and points to it in Activity; it can't turn a
 * submitted payment back into a draft. Screens that create nothing (recovery,
 * operation detail) pass no handle.
 *
 * Saved state is captured when the screen stops, and a write made while it is
 * stopped waits for the next stop. So a send that completes in the background
 * would be missing from a snapshot taken mid-call. Before calling the SDK,
 * [execute] therefore records that a submission is in progress (while the
 * screen is still in front, so the snapshot has it). A screen restored with
 * that record but no operation id stays locked and asks [reconcile], which
 * looks in the SDK's persistent history for an operation of this screen's kind
 * created since then: found, it locks on that operation; not found, nothing
 * was sent and the screen unlocks.
 */
abstract class PaymentViewModel(
    private val savedState: SavedStateHandle? = null,
    private val reconcile: (suspend (sinceMillis: Long) -> OperationId?)? = null,
    private val now: () -> Long = System::currentTimeMillis,
) : ViewModel() {
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
        /** The operation this screen created, set as soon as the SDK returned it. */
        val operationId: OperationId? = null,
        /** Following the operation's updates failed. The operation itself is unaffected. */
        val followFailed: Boolean = false,
        /** The operation came from saved state after the process was recreated; nothing follows it here. */
        val restored: Boolean = false,
        /**
         * Restored mid-submission: whether anything was sent is being checked
         * against the SDK's history. The screen stays locked until it knows.
         */
        val checkingSubmission: Boolean = false,
        /** That check failed (e.g. the federation isn't open); it can be retried. */
        val submissionCheckFailed: Boolean = false,
        /** The check found nothing: the app closed before anything was submitted. */
        val notSubmitted: Boolean = false,
    ) {
        /** Once an operation exists, the inputs are locked: this screen is now following it. */
        val started: Boolean
            get() = operationId != null || progress != null || output != null || checkingSubmission

        /** The operation exists but this screen can't show its state: point to it instead. */
        val needsActivityLink: Boolean get() = operationId != null && (progress == null || followFailed)
    }

    protected val mutableState = MutableStateFlow(
        when {
            savedState?.get<String>(KEY_OPERATION) != null ->
                UiState(operationId = savedState.get<String>(KEY_OPERATION), restored = true)
            savedState?.get<Long>(KEY_SUBMITTING) != null -> UiState(checkingSubmission = true)
            else -> UiState()
        },
    )
    val state = mutableState.asStateFlow()

    init {
        if (mutableState.value.checkingSubmission) checkSubmission()
    }

    /**
     * Restored mid-submission: find out from the SDK's history whether the
     * operation was created. Until then the screen stays locked; a failed check
     * keeps it locked and can be retried.
     */
    fun checkSubmission() {
        val since = savedState?.get<Long>(KEY_SUBMITTING) ?: return
        mutableState.update { it.copy(checkingSubmission = true, submissionCheckFailed = false, error = null) }
        viewModelScope.launch {
            attempt { reconcile?.invoke(since) }
                .onSuccess { id ->
                    savedState?.remove<Long>(KEY_SUBMITTING)
                    if (id != null) {
                        savedState?.set(KEY_OPERATION, id)
                        mutableState.update { it.copy(checkingSubmission = false, operationId = id, restored = true) }
                    } else {
                        mutableState.update { it.copy(checkingSubmission = false, notSubmitted = true) }
                    }
                }
                .onFailure { e ->
                    Log.w(TAG, "checking a restored submission failed: ${describe(e)}", e)
                    mutableState.update { it.copy(submissionCheckFailed = true, error = userMessage(e)) }
                }
        }
    }

    private val handles = mutableListOf<AutoCloseable>()

    /** Registers an SDK handle to be closed with this ViewModel. */
    protected fun <T : AutoCloseable> T.owned(): T = also { synchronized(handles) { handles += it } }

    /**
     * Runs one step, unless one is already running. Failures land in
     * [UiState.error], after [onFailure] has had a chance to adjust the state.
     */
    protected fun step(onFailure: () -> Unit = {}, block: suspend () -> Unit) {
        if (mutableState.value.working) return
        mutableState.update { it.copy(working = true, error = null) }
        viewModelScope.launch {
            val error = attempt { block() }.exceptionOrNull()
            if (error != null) {
                // The screen shows a message chosen by error code; the SDK's own reason goes to the log.
                Log.w(TAG, "payment step failed: ${describe(error)}", error)
                onFailure()
            }
            mutableState.update { it.copy(working = false, error = error?.let(::userMessage)) }
        }
    }

    /**
     * Creates an operation: [create] makes the SDK call (a send, a receive)
     * and returns the new operation's id, after starting to follow it.
     *
     * The submission is recorded in saved state before the call, while the
     * screen is in front (see the class comment). On success the id replaces
     * that record and locks the screen. On failure the record is dropped (the
     * SDK reports a failed call only when nothing was created) and so is any
     * review: a send has consumed its quote either way, so approving again
     * must start from a fresh one.
     */
    protected fun execute(create: suspend () -> OperationId) = step(
        onFailure = {
            savedState?.remove<Long>(KEY_SUBMITTING)
            mutableState.update { it.copy(review = null) }
        },
    ) {
        savedState?.set(KEY_SUBMITTING, now())
        mutableState.update { it.copy(notSubmitted = false) }
        val id = create()
        savedState?.set(KEY_OPERATION, id)
        savedState?.remove<Long>(KEY_SUBMITTING)
        mutableState.update { it.copy(review = null, operationId = id) }
    }

    /**
     * Mirrors an operation's states into [UiState.progress] until it is final.
     * If the updates fail, the operation is still recorded; the screen offers
     * the activity entry rather than anything that could send again.
     */
    protected fun <S> follow(states: Flow<S>, describe: (S) -> OpProgress) {
        viewModelScope.launch {
            states
                .catch { e ->
                    Log.w(TAG, "following an operation failed: ${describe(e)}", e)
                    mutableState.update { it.copy(followFailed = true, error = userMessage(e)) }
                }
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

        /** Saved-state key for the created operation's id. */
        const val KEY_OPERATION = "operation_id"

        /** Saved-state key for when a submission started, until it returns. */
        const val KEY_SUBMITTING = "submitting_since"

        /** Clock skew allowed between the recorded start and the SDK's operation time. */
        private const val RECONCILE_SLACK_MILLIS = 60_000L

        /**
         * A [reconcile] that looks in [history] for the newest operation of
         * [kind] created since the submission started.
         */
        fun reconcileWith(
            history: History,
            federationId: FederationId,
            kind: OperationKind,
        ): suspend (Long) -> OperationId? = { since ->
            history.page(federationId).items
                .firstOrNull { it.kind == kind && it.time.toLong() >= since - RECONCILE_SLACK_MILLIS }
                ?.operationId
        }

        fun expiry(ts: Timestamp): Long = ts.toLong()

        private fun describe(e: Throwable): String =
            if (e is SdkException) "[${e.code()}] ${e.reason()}" else e.toString()
    }
}
