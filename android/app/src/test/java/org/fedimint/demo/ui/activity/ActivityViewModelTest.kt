package org.fedimint.demo.ui.activity

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import kotlinx.coroutines.withContext
import org.fedimint.sdk.ActivityItem
import org.fedimint.sdk.ActivityPage
import org.fedimint.sdk.ActivityStatus
import org.fedimint.sdk.Cursor
import org.fedimint.sdk.Direction
import org.fedimint.sdk.OperationKind
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Before
import org.junit.Test

/**
 * Refresh and paging overlap without duplicating rows or keeping a stale
 * cursor, in either completion order.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ActivityViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @Before fun setUp() = Dispatchers.setMain(dispatcher)

    @After fun tearDown() = Dispatchers.resetMain()

    /**
     * A page loader whose calls the test completes by hand. Completions ignore
     * cancellation, like an SDK call already in flight, so stale results really
     * arrive and the ViewModel has to discard them itself.
     */
    private class Loader {
        val calls = mutableListOf<Pair<Cursor?, CompletableDeferred<ActivityPage>>>()

        suspend fun load(cursor: Cursor?): ActivityPage {
            val result = CompletableDeferred<ActivityPage>()
            calls += cursor to result
            return withContext(NonCancellable) { result.await() }
        }

        fun complete(call: Int, page: ActivityPage) = calls[call].second.complete(page)
    }

    private fun item(n: Int) = ActivityItem(
        operationId = "op-$n",
        kind = OperationKind.LN_RECEIVE,
        time = n.toULong(),
        amount = 1_000uL,
        fee = 0uL,
        direction = Direction.INCOMING,
        status = ActivityStatus.SUCCESS,
        isFinal = true,
    )

    private fun page(range: IntRange, next: Cursor?) = ActivityPage(items = range.map(::item), next = next)

    /**
     * The reviewer's sequence: page-two request A, a completed refresh, page-two
     * request B, then A and B complete in [aFirst] order.
     */
    private fun overlap(aFirst: Boolean) = runTest(dispatcher) {
        val loader = Loader()
        val vm = ActivityViewModel(loader::load)
        advanceUntilIdle()
        loader.complete(0, page(1..20, next = "c1"))
        advanceUntilIdle()

        vm.loadMore() // A
        advanceUntilIdle()
        vm.refresh()
        advanceUntilIdle()
        loader.complete(2, page(1..20, next = "c1"))
        advanceUntilIdle()
        vm.loadMore() // B
        advanceUntilIdle()
        assertEquals(listOf(null, "c1", null, "c1"), loader.calls.map { it.first })

        val (a, b) = 1 to 3
        if (aFirst) {
            loader.complete(a, page(21..40, next = "c2"))
            advanceUntilIdle()
            loader.complete(b, page(21..40, next = "c2"))
        } else {
            loader.complete(b, page(21..40, next = "c2"))
            advanceUntilIdle()
            loader.complete(a, page(21..40, next = "c2"))
        }
        advanceUntilIdle()

        val s = vm.state.value
        val ids = s.items.map { it.operationId }
        assertEquals("every row once", ids.distinct(), ids)
        assertEquals(40, ids.size)
        assertEquals("the latest generation's cursor", "c2", s.next)
        assertFalse(s.loadingMore)
    }

    @Test fun `stale page completing first is ignored`() = overlap(aFirst = true)

    @Test fun `stale page completing last is ignored`() = overlap(aFirst = false)

    @Test fun `asking for more twice requests the page once`() = runTest(dispatcher) {
        val loader = Loader()
        val vm = ActivityViewModel(loader::load)
        advanceUntilIdle()
        loader.complete(0, page(1..20, next = "c1"))
        advanceUntilIdle()

        vm.loadMore()
        vm.loadMore()
        advanceUntilIdle()

        assertEquals(2, loader.calls.size)
    }
}
