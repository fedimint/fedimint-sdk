package org.fedimint.demo.wallet

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.FlowCollector
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import org.fedimint.sdk.ActivityPage
import org.fedimint.sdk.AnyOperation
import org.fedimint.sdk.Cursor
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.OperationId
import org.fedimint.sdk.OperationKind
import org.fedimint.sdk.OperationSupport

/**
 * A federation's local activity history, and the operations behind it.
 *
 * History is paged newest first: pass a null cursor for the first page and
 * each page's `next` for the one after; a page with no `next` is the last.
 * The SDK brings in-flight rows up to date before returning a page.
 */
class History(private val session: WalletSession) {

    suspend fun page(id: FederationId, cursor: Cursor? = null, limit: Int = PAGE_SIZE): ActivityPage =
        session.withFederation(id) { it.activity(cursor, limit.toUShort()) }

    /**
     * The operation with this id, untyped, or null if the federation has no
     * such operation (a stale link, say). Check `support()` before narrowing
     * it with `asLnSend()` and the like: an operation written by a newer SDK
     * is still returned, but has no typed handle in this build.
     */
    suspend fun operation(id: FederationId, operationId: OperationId): AnyOperation? =
        session.withFederation(id) { it.operation(operationId) }

    /**
     * Emits once the operation leaves the state it is in when this starts
     * following it, or reaches its final state; the SDK pushes each change, so
     * nothing is read in between. Throws if it can't be followed: missing from
     * this federation, or written by a newer SDK (not `OBSERVABLE`). Closes
     * what it opened when collection ends.
     */
    fun changes(id: FederationId, operationId: OperationId): Flow<Unit> = flow {
        val any = operation(id, operationId) ?: throw IllegalStateException("This payment isn't in this federation's history.")
        any.use { op ->
            check(op.support() == OperationSupport.OBSERVABLE) { "This operation can't be followed by this version of the app." }
            val states: Flow<Any> = flow {
                when (op.kind()) {
                    OperationKind.ECASH_SEND -> follow(op.asEcashSend(), { it.updates() }) { it.next() }
                    OperationKind.ECASH_RECEIVE -> follow(op.asEcashReceive(), { it.updates() }) { it.next() }
                    OperationKind.LN_SEND -> follow(op.asLnSend(), { it.updates() }) { it.next() }
                    OperationKind.LN_RECEIVE -> follow(op.asLnReceive(), { it.updates() }) { it.next() }
                    OperationKind.ONCHAIN_SEND -> follow(op.asOnchainSend(), { it.updates() }) { it.next() }
                    OperationKind.ONCHAIN_RECEIVE -> follow(op.asOnchainReceive(), { it.updates() }) { it.next() }
                    OperationKind.RECOVERY -> follow(op.asRecovery(), { it.updates() }) { it.next() }
                    // Not OBSERVABLE, so refused above; listed so a new kind fails this build.
                    OperationKind.UNKNOWN -> error("This operation can't be followed.")
                }
            }
            var first: Any? = null
            var changed = false
            states.collect { state ->
                if (first == null) {
                    first = state
                } else if (state != first && !changed) {
                    changed = true
                    emit(Unit)
                }
            }
            // The states end once one is final: that counts as a change too.
            if (!changed) emit(Unit)
        }
    }

    private suspend fun <O : AutoCloseable, U : AutoCloseable, S : Any> FlowCollector<Any>.follow(
        operation: O?,
        updates: (O) -> U,
        next: suspend (U) -> S?,
    ) = checkNotNull(operation) { "This operation can't be followed." }.use { o ->
        emitAll(Payments.states({ updates(o) }, next))
    }

    companion object {
        const val PAGE_SIZE = 20
    }
}
