package org.fedimint.demo.ui.payments

import android.util.Log
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.fedimint.demo.ui.common.attempt
import org.fedimint.demo.ui.common.userMessage
import org.fedimint.demo.wallet.History
import org.fedimint.sdk.ActivityItem
import org.fedimint.sdk.ActivityPage
import org.fedimint.sdk.Cursor
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
 * searches the SDK's persistent history ([findSubmission]): exactly one
 * operation of this screen's kind created since then locks the screen on it;
 * none at all, with the history searched back past the attempt, unlocks it;
 * anything else (several candidates, or one that may predate the attempt) is
 * unclear, and the screen stays locked and says so rather than claim nothing
 * was sent.
 */
abstract class PaymentViewModel(
    private val savedState: SavedStateHandle? = null,
    private val reconcile: (suspend (sinceMillis: Long) -> Reconciled)? = null,
    private val now: () -> Long = System::currentTimeMillis,
) : ViewModel() {
    /**
     * A quote the user must approve before anything is sent. [amount] is the
     * headline figure (whole sats, no unit), shown large above the rows.
     */
    data class Review(
        val rows: List<Pair<String, String>>,
        val total: String,
        val expiresAtMillis: Long?,
        val amount: String? = null,
    )

    /**
     * A state the operation was seen in, and when this screen saw it. Only
     * what was observed live: the SDK doesn't record when each state began, so
     * a screen opened later starts from the state current then.
     */
    data class Step(val progress: OpProgress, val atMillis: Long)

    /** Something to hand to the other party: an invoice, an address, notes. */
    data class Output(val label: String, val text: String, val qr: String? = text)

    data class UiState(
        val working: Boolean = false,
        /**
         * The running step is a submission ([execute]): an SDK call that may
         * create an operation. Leaving the screen would cancel it mid-call, so
         * screens hold Back while this is set (and only then: a quote or an
         * address that hangs mustn't trap the user).
         */
        val submitting: Boolean = false,
        val error: String? = null,
        val review: Review? = null,
        val output: Output? = null,
        val progress: OpProgress? = null,
        /** The approved quote, kept once it is sent, so the screen can still say what was paid. */
        val sent: Review? = null,
        /** The states [progress] has gone through while this screen followed it, oldest first. */
        val steps: List<Step> = emptyList(),
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
        /** The check couldn't single out this attempt in the history: locked, and the user checks Activity. */
        val submissionUnclear: Boolean = false,
    ) {
        /** Once an operation exists, the inputs are locked: this screen is now following it. */
        val started: Boolean
            get() = operationId != null || progress != null || output != null || checkingSubmission || submissionUnclear

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
        mutableState.update {
            it.copy(checkingSubmission = true, submissionCheckFailed = false, submissionUnclear = false, error = null)
        }
        viewModelScope.launch {
            attempt { reconcile?.invoke(since) ?: Reconciled.Unclear }
                .onSuccess { result ->
                    when (result) {
                        is Reconciled.Found -> {
                            savedState?.remove<Long>(KEY_SUBMITTING)
                            savedState?.set(KEY_OPERATION, result.id)
                            mutableState.update { it.copy(checkingSubmission = false, operationId = result.id, restored = true) }
                        }
                        Reconciled.NotSubmitted -> {
                            savedState?.remove<Long>(KEY_SUBMITTING)
                            mutableState.update { it.copy(checkingSubmission = false, notSubmitted = true) }
                        }
                        // The record stays, so a later restore is locked too.
                        Reconciled.Unclear -> mutableState.update { it.copy(checkingSubmission = false, submissionUnclear = true) }
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
     * The slot for a send's quote, which is single use: the SDK consumes it
     * even on a failed send (SECURITY.md, rule 7). [hold] keeps a new one and
     * closes the one it replaces; [drop] discards it when the input changes;
     * [take] hands it to the send and empties the slot, so nothing can submit
     * it twice.
     */
    protected inner class SingleUseQuote<Q : AutoCloseable> {
        private var held: Q? = null

        fun hold(quote: Q): Q {
            val owned = quote.owned()
            release(held)
            held = owned
            return owned
        }

        fun drop() {
            release(held)
            held = null
        }

        fun take(): Q = checkNotNull(held) { "That quote was already used. Review it again." }.also { held = null }
    }

    /**
     * Closes a handle this screen no longer needs (a replaced quote, notes
     * parsed again) now, rather than when the screen goes. Notes are a bearer
     * instrument, so their native copies shouldn't pile up.
     */
    protected fun release(handle: AutoCloseable?) {
        if (handle == null) return
        synchronized(handles) { handles.remove(handle) }
        runCatching { handle.close() }
    }

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
            mutableState.update { it.copy(working = false, submitting = false, error = error?.let(::userMessage)) }
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
        mutableState.update { it.copy(notSubmitted = false, submitting = true) }
        val id = create()
        savedState?.set(KEY_OPERATION, id)
        savedState?.remove<Long>(KEY_SUBMITTING)
        mutableState.update { it.copy(review = null, sent = it.review, operationId = id) }
    }

    /**
     * Mirrors an operation's states into [UiState.progress] until it is final,
     * or until the returned job is cancelled.
     * If the updates fail, the operation is still recorded; the screen offers
     * the activity entry rather than anything that could send again.
     */
    protected fun <S> follow(states: Flow<S>, toProgress: (S) -> OpProgress): Job =
        viewModelScope.launch {
            states
                .catch { e ->
                    Log.w(TAG, "following an operation failed: ${describe(e)}", e)
                    mutableState.update { it.copy(followFailed = true, error = userMessage(e)) }
                }
                .collect { s ->
                    val progress = toProgress(s)
                    mutableState.update {
                        val repeated = it.steps.lastOrNull()?.progress?.label == progress.label
                        it.copy(progress = progress, steps = if (repeated) it.steps else it.steps + Step(progress, now()))
                    }
                }
        }

    /** Says why a pasted or scanned code wasn't put in the field. */
    fun rejectInput(message: String) = mutableState.update { it.copy(error = message) }

    /**
     * Drops an unapproved quote, e.g. because the input it was made for
     * changed, or Back on the review. Not while a step runs: during a send
     * the review is what is being paid, and Back must not cut it short (the
     * screen's Back handler then swallows Back until the send returns).
     */
    protected fun clearReview() {
        if (mutableState.value.working) return
        mutableState.update { it.copy(review = null, error = null) }
    }

    override fun onCleared() {
        synchronized(handles) { handles.forEach { runCatching { it.close() } } }
    }

    protected companion object {
        private const val TAG = "Payments"

        /** Saved-state key for the created operation's id. */
        const val KEY_OPERATION = "operation_id"

        /** Saved-state key for when a submission started, until it returns. */
        const val KEY_SUBMITTING = "submitting_since"

        /**
         * A [reconcile] that searches [history] for this attempt
         * ([findSubmission] over every page it needs).
         */
        fun reconcileWith(
            history: History,
            federationId: FederationId,
            kind: OperationKind,
        ): suspend (Long) -> Reconciled = { since ->
            findSubmission(kind, since) { cursor -> history.page(federationId, cursor) }
        }

        fun expiry(ts: Timestamp): Long = ts.toLong()

        private fun describe(e: Throwable): String =
            if (e is SdkException) "[${e.code()}] ${e.reason()}" else e.toString()
    }
}

/** What the history says about a submission interrupted by process death. */
sealed interface Reconciled {
    /** This attempt's operation. */
    data class Found(val id: OperationId) : Reconciled

    /** No operation of the kind anywhere near the attempt: nothing was sent. */
    data object NotSubmitted : Reconciled

    /** The history can't single out the attempt; don't claim either way. */
    data object Unclear : Reconciled
}

/** Operations this much older than the recorded start still count as candidates: the clocks may disagree. */
internal const val RECONCILE_WINDOW_MILLIS = 60_000L

/**
 * Finds the operation an interrupted submission of [kind], started at
 * [sinceMillis], created. History is newest first, so pages are read until
 * one reaches back past the window; a match on any page counts.
 *
 * Only a single candidate, stamped at or after the attempt's start, is taken
 * as the attempt: activity times are the client's own epoch milliseconds, the
 * clock the start was read from, so the attempt's operation can't predate it,
 * and one that does is another payment. No candidate at all means nothing was
 * sent. Anything else (several candidates, or a single one stamped before the
 * start) is [Reconciled.Unclear].
 */
internal suspend fun findSubmission(
    kind: OperationKind,
    sinceMillis: Long,
    load: suspend (Cursor?) -> ActivityPage,
): Reconciled {
    val earliest = sinceMillis - RECONCILE_WINDOW_MILLIS
    val candidates = mutableListOf<ActivityItem>()
    var cursor: Cursor? = null
    do {
        val page = load(cursor)
        candidates += page.items.filter { it.kind == kind && it.time.toLong() >= earliest }
        if (page.items.any { it.time.toLong() < earliest }) break
        cursor = page.next
    } while (cursor != null)
    val single = candidates.singleOrNull()
    return when {
        candidates.isEmpty() -> Reconciled.NotSubmitted
        single != null && single.time.toLong() >= sinceMillis -> Reconciled.Found(single.operationId)
        else -> Reconciled.Unclear
    }
}
