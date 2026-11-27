package org.fedimint.demo.ui.payments

import androidx.lifecycle.SavedStateHandle
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.fedimint.sdk.ActivityItem
import org.fedimint.sdk.ActivityPage
import org.fedimint.sdk.ActivityStatus
import org.fedimint.sdk.Cursor
import org.fedimint.sdk.Direction
import org.fedimint.sdk.OperationKind
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * The two rules that keep money from moving twice (PaymentViewModel, SECURITY.md
 * rule 7): an operation is recorded as soon as it exists, whatever its
 * observation does; and a failed send drops the quote it consumed.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class PaymentViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @Before fun setUp() = Dispatchers.setMain(dispatcher)

    @After fun tearDown() = Dispatchers.resetMain()

    /** Exposes the base class's protected steps with fake SDK calls. */
    private class Vm(
        savedState: SavedStateHandle = SavedStateHandle(),
        reconcile: (suspend (Long) -> Reconciled)? = null,
    ) : PaymentViewModel(savedState, reconcile, now = { 1_000L }) {
        var sends = 0

        fun quoted() = mutableState.update { it.copy(review = Review(emptyList(), "100 sats", null, amount = "100")) }

        /** What Back on the review does. */
        fun back() = clearReview()

        fun send(states: Flow<String>, id: String, before: suspend () -> Unit = {}) = execute {
            before()
            sends++
            follow(states) { OpProgress(it) }
            id
        }

        fun failSend(message: String) = execute {
            sends++
            throw IllegalStateException(message)
        }

        companion object {
            const val KEY = KEY_OPERATION
            const val SUBMITTING = KEY_SUBMITTING
        }
    }

    private fun test(block: suspend TestScope.(Vm) -> Unit) = runTest(dispatcher) { block(Vm()) }

    @Test fun `a sent operation locks the screen before its first update arrives`() = test { vm ->
        vm.quoted()
        vm.send(flow { awaitCancellation() }, id = "op-1")
        advanceUntilIdle()

        val s = vm.state.value
        assertEquals("op-1", s.operationId)
        assertTrue("inputs stay locked", s.started)
        assertNull("no review to approve again", s.review)
        assertNull(s.progress)
        assertTrue("points to the existing operation", s.needsActivityLink)
        assertFalse(s.working)
    }

    @Test fun `a failed observation keeps the operation and never reopens sending`() = test { vm ->
        vm.quoted()
        vm.send(flow { throw IllegalStateException("updates broke") }, id = "op-1")
        advanceUntilIdle()

        val s = vm.state.value
        assertEquals("op-1", s.operationId)
        assertTrue(s.followFailed)
        assertTrue(s.started)
        assertNull(s.review)
        assertTrue(s.needsActivityLink)
        assertEquals(1, vm.sends)
    }

    @Test fun `a failed send drops the consumed review`() = test { vm ->
        vm.quoted()
        vm.failSend("That quote expired. Get a new one.")
        advanceUntilIdle()

        val s = vm.state.value
        assertNull("the spent quote's review is gone, so Review fetches a new one", s.review)
        assertNull(s.operationId)
        assertFalse("inputs unlock for a fresh quote", s.started)
        assertEquals("That quote expired. Get a new one.", s.error)
    }

    @Test fun `failure then a fresh quote then success, without editing anything`() = test { vm ->
        vm.quoted()
        vm.failSend("Not enough balance for that.")
        advanceUntilIdle()

        vm.quoted() // what tapping Review again does: a new quote for the same inputs
        vm.send(flow { emit("Paid") }, id = "op-2")
        advanceUntilIdle()

        val s = vm.state.value
        assertEquals("op-2", s.operationId)
        assertEquals("Paid", s.progress?.label)
        assertNull(s.error)
        assertEquals(2, vm.sends)
    }

    @Test fun `Back during a send keeps the review being paid`() = test { vm ->
        val sdkReturns = CompletableDeferred<Unit>()
        vm.quoted()
        vm.send(flow { awaitCancellation() }, id = "op-1", before = { sdkReturns.await() })
        advanceUntilIdle()

        vm.back()
        assertNotNull("still on screen while it is being paid", vm.state.value.review)

        sdkReturns.complete(Unit)
        advanceUntilIdle()
        assertEquals("op-1", vm.state.value.operationId)
        assertEquals("You sent shows the amount", "100", vm.state.value.sent?.amount)
    }

    @Test fun `Back on an idle review drops it`() = test { vm ->
        vm.quoted()
        vm.back()
        assertNull(vm.state.value.review)
    }

    @Test fun `a second tap while sending does not send again`() = test { vm ->
        vm.quoted()
        vm.send(flow { emit("Paid") }, id = "op-1", before = { delay(1_000) })
        vm.send(flow { emit("Paid") }, id = "op-2")
        advanceUntilIdle()

        assertEquals(1, vm.sends)
        assertEquals("op-1", vm.state.value.operationId)
        assertNotNull(vm.state.value.progress)
    }

    @Test fun `a submitted operation survives process recreation, locked`() = test { _ ->
        val saved = SavedStateHandle()
        val before = Vm(saved)
        before.quoted()
        before.send(flow { awaitCancellation() }, id = "op-1")
        advanceUntilIdle()

        // Android kills the process and later restores the screen: a new ViewModel,
        // built only from what was saved. The route and the inputs come back too.
        val after = Vm(SavedStateHandle(mapOf(Vm.KEY to saved.get<String>(Vm.KEY))))

        val s = after.state.value
        assertEquals("op-1", s.operationId)
        assertTrue("inputs stay locked", s.started)
        assertTrue(s.restored)
        assertNull("no review, so no Send for the same payment", s.review)
        assertTrue("points to the existing operation", s.needsActivityLink)
        assertEquals(0, after.sends)
    }

    @Test fun `a screen that submitted nothing restores as an editable draft`() = test { _ ->
        val s = Vm(SavedStateHandle()).state.value
        assertNull(s.operationId)
        assertFalse(s.started)
        assertFalse(s.restored)
    }

    @Test fun `a failed send saves nothing to restore`() = test { _ ->
        val saved = SavedStateHandle()
        val vm = Vm(saved)
        vm.quoted()
        vm.failSend("Not enough balance for that.")
        advanceUntilIdle()
        assertNull(saved.get<String>(Vm.KEY))
        assertNull("no submission left in progress", saved.get<Long>(Vm.SUBMITTING))
    }

    /** What Android keeps when the screen stops: the handle's contents at that moment. */
    private fun SavedStateHandle.snapshot() = SavedStateHandle(keys().associateWith { get<Any?>(it) })

    @Test fun `state saved while the send is still running restores locked on the operation`() = test { _ ->
        val saved = SavedStateHandle()
        val sending = Vm(saved)
        val sdkReturns = CompletableDeferred<Unit>()
        sending.quoted()
        sending.send(flow { awaitCancellation() }, id = "op-1", before = { sdkReturns.await() })
        advanceUntilIdle()

        // The app goes to the background mid-call and Android saves the screen now...
        val snapshot = saved.snapshot()
        // ...then the send completes in the background, too late for that snapshot.
        sdkReturns.complete(Unit)
        advanceUntilIdle()
        assertNull("the snapshot predates the operation id", snapshot.get<String>(Vm.KEY))

        // The process is killed and the screen restored from the snapshot.
        val restored = Vm(snapshot, reconcile = { since -> if (since == 1_000L) Reconciled.Found("op-1") else Reconciled.NotSubmitted })
        assertTrue("locked before the check even returns", restored.state.value.started)
        assertNull(restored.state.value.review)
        advanceUntilIdle()

        val s = restored.state.value
        assertEquals("found in the SDK's history", "op-1", s.operationId)
        assertTrue(s.started)
        assertFalse(s.checkingSubmission)
        assertNull(s.review)
        assertEquals(0, restored.sends)
    }

    @Test fun `restored mid-send with nothing in history unlocks and says nothing was sent`() = test { _ ->
        val restored = Vm(SavedStateHandle(mapOf(Vm.SUBMITTING to 1_000L)), reconcile = { Reconciled.NotSubmitted })
        advanceUntilIdle()

        val s = restored.state.value
        assertNull(s.operationId)
        assertTrue(s.notSubmitted)
        assertFalse("the draft can be submitted again", s.started)
    }

    @Test fun `a failed check stays locked until a retry finds the operation`() = test { _ ->
        var historyReachable = false
        val restored = Vm(
            SavedStateHandle(mapOf(Vm.SUBMITTING to 1_000L)),
            reconcile = { if (historyReachable) Reconciled.Found("op-1") else throw IllegalStateException("This federation isn't open.") },
        )
        advanceUntilIdle()
        assertTrue(restored.state.value.started)
        assertTrue(restored.state.value.submissionCheckFailed)

        historyReachable = true
        restored.checkSubmission()
        advanceUntilIdle()
        assertEquals("op-1", restored.state.value.operationId)
        assertFalse(restored.state.value.submissionCheckFailed)
    }

    @Test fun `an unclear history keeps the screen locked and claims nothing`() = test { _ ->
        val saved = SavedStateHandle(mapOf(Vm.SUBMITTING to 1_000L))
        val restored = Vm(saved, reconcile = { Reconciled.Unclear })
        advanceUntilIdle()

        val s = restored.state.value
        assertTrue("still locked", s.started)
        assertTrue(s.submissionUnclear)
        assertFalse("never says nothing was sent", s.notSubmitted)
        assertNull(s.operationId)
        assertNotNull("a later restore is locked too", saved.get<Long>(Vm.SUBMITTING))
    }

    // findSubmission: the history search behind reconcileWith.

    private fun op(id: String, time: Long, kind: OperationKind = OperationKind.ONCHAIN_SEND) = ActivityItem(
        operationId = id,
        kind = kind,
        time = time.toULong(),
        amount = 1_000uL,
        fee = 0uL,
        direction = Direction.OUTGOING,
        status = ActivityStatus.PENDING,
        isFinal = false,
    )

    /** Pages newest first, [size] at a time, cursors "1", "2", …; records each cursor asked for. */
    private class Pages(private val items: List<ActivityItem>, private val size: Int = 20) {
        val asked = mutableListOf<Cursor?>()

        suspend fun load(cursor: Cursor?): ActivityPage {
            asked += cursor
            val n = cursor?.toInt() ?: 0
            val chunk = items.drop(n * size).take(size)
            return ActivityPage(chunk, (n + 1).toString().takeIf { (n + 1) * size < items.size })
        }
    }

    private val since = 10_000_000L

    @Test fun `the attempt on page two is found`() = runTest(dispatcher) {
        // Twenty newer deposits push the withdrawal onto the second page.
        val deposits = (1..20).map { op("dep-$it", since + 5_000 + it, OperationKind.ONCHAIN_RECEIVE) }
        val pages = Pages(deposits + op("send", since + 1_000) + op("old", since - 3_600_000))

        assertEquals(Reconciled.Found("send"), findSubmission(OperationKind.ONCHAIN_SEND, since, pages::load))
        assertEquals(listOf(null, "1"), pages.asked)
    }

    @Test fun `nothing near the attempt in the whole history means nothing was sent`() = runTest(dispatcher) {
        val pages = Pages((1..30).map { op("dep-$it", since + it, OperationKind.ONCHAIN_RECEIVE) } + op("old", since - 3_600_000))
        assertEquals(Reconciled.NotSubmitted, findSubmission(OperationKind.ONCHAIN_SEND, since, pages::load))
    }

    @Test fun `an earlier payment of the same kind is not taken for the attempt`() = runTest(dispatcher) {
        // Sent 30 s before the attempt started: inside the clock window, but not this attempt.
        val pages = Pages(listOf(op("earlier", since - 30_000)))
        assertEquals(Reconciled.Unclear, findSubmission(OperationKind.ONCHAIN_SEND, since, pages::load))
    }

    @Test fun `a lone payment stamped just before the attempt started is not its receipt`() = runTest(dispatcher) {
        // Activity times are true epoch milliseconds from the clock the start was read
        // from: the attempt's own operation can't predate it.
        for (offset in listOf(1L, 1_000L, 2_000L, 2_001L)) {
            val pages = Pages(listOf(op("earlier", since - offset)))
            assertEquals("at since - $offset", Reconciled.Unclear, findSubmission(OperationKind.ONCHAIN_SEND, since, pages::load))
        }
    }

    @Test fun `a lone payment stamped at the start is the attempt`() = runTest(dispatcher) {
        val pages = Pages(listOf(op("send", since)))
        assertEquals(Reconciled.Found("send"), findSubmission(OperationKind.ONCHAIN_SEND, since, pages::load))
    }

    @Test fun `two candidates are unclear`() = runTest(dispatcher) {
        val pages = Pages(listOf(op("b", since + 2_000), op("a", since + 1_000)))
        assertEquals(Reconciled.Unclear, findSubmission(OperationKind.ONCHAIN_SEND, since, pages::load))
    }

    @Test fun `the search stops once a page reaches back past the window`() = runTest(dispatcher) {
        val pages = Pages(listOf(op("send", since + 1_000)) + (1..40).map { op("old-$it", since - 120_000 - it) })
        findSubmission(OperationKind.ONCHAIN_SEND, since, pages::load)
        assertEquals(listOf<Cursor?>(null), pages.asked)
    }
}
