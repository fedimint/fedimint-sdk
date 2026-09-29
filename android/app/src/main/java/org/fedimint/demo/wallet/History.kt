package org.fedimint.demo.wallet

import org.fedimint.sdk.ActivityPage
import org.fedimint.sdk.AnyOperation
import org.fedimint.sdk.Cursor
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.OperationId

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

    companion object {
        const val PAGE_SIZE = 20
    }
}
