package org.fedimint.demo.ui.payments

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * Deposits are followed one at a time: a newer deposit takes over the screen,
 * and the older one's later states never write to it.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class OnchainReceiveViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @Before fun setUp() = Dispatchers.setMain(dispatcher)

    @After fun tearDown() = Dispatchers.resetMain()

    /** Deposits the test hands over by hand; records every subscription and whether it was closed. */
    private class FakeSource : OnchainReceiveViewModel.Source {
        inner class Subscription : OnchainReceiveViewModel.Deposits {
            val incoming = Channel<OnchainReceiveViewModel.Deposit>(Channel.UNLIMITED)
            var closed = false

            override suspend fun next() = incoming.receive()

            override fun close() {
                closed = true
            }
        }

        val subscriptions = mutableListOf<Subscription>()
        var addresses = 0
        var failAddress = false

        override suspend fun deposits() = Subscription().also { subscriptions += it }

        override suspend fun address(): String {
            if (failAddress) throw IllegalStateException("federation unreachable")
            return "tb1qaddress${++addresses}"
        }
    }

    private fun deposit(id: String) = MutableSharedFlow<OpProgress>(replay = 1).let { it to OnchainReceiveViewModel.Deposit(id, it) }

    private val confirmed = OpProgress("Deposit confirmed, claiming it…")
    private val claimed = OpProgress("Received", settled = true, ok = true)

    @Test fun `an older deposit settling doesn't show under the newer one`() = runTest(dispatcher) {
        val source = FakeSource()
        val vm = OnchainReceiveViewModel(source)
        advanceUntilIdle()
        val subscription = source.subscriptions.single()

        val (a, depositA) = deposit("op-a")
        subscription.incoming.send(depositA)
        a.emit(confirmed)
        advanceUntilIdle()
        assertEquals("op-a", vm.state.value.operationId)

        val (b, depositB) = deposit("op-b")
        subscription.incoming.send(depositB)
        b.emit(confirmed)
        advanceUntilIdle()

        // A is claimed after B took over the screen.
        a.emit(claimed)
        advanceUntilIdle()

        val s = vm.state.value
        assertEquals("op-b", s.operationId)
        assertEquals(confirmed, s.progress)
        assertFalse("B is still pending", s.progress!!.settled)
        assertEquals("B's timeline only", listOf(confirmed), s.steps.map { it.progress })

        b.emit(claimed)
        advanceUntilIdle()
        assertEquals(claimed, vm.state.value.progress)
    }

    @Test fun `a new address replaces the subscription instead of adding one`() = runTest(dispatcher) {
        val source = FakeSource()
        val vm = OnchainReceiveViewModel(source)
        advanceUntilIdle()
        val (a, depositA) = deposit("op-a")
        source.subscriptions[0].incoming.send(depositA)
        a.emit(confirmed)
        advanceUntilIdle()

        vm.newAddress()
        advanceUntilIdle()
        vm.newAddress()
        advanceUntilIdle()

        assertEquals(3, source.subscriptions.size)
        assertEquals("only the newest is open", listOf(true, true, false), source.subscriptions.map { it.closed })
        val s = vm.state.value
        assertEquals("tb1qaddress3", s.output?.text)
        assertEquals(null, s.operationId)
        assertTrue("back to waiting", s.steps.isEmpty())

        // The superseded deposit settling later writes nothing.
        a.emit(claimed)
        advanceUntilIdle()
        assertEquals("Waiting for a deposit", vm.state.value.progress?.label)
    }

    @Test fun `a new deposit never shows the previous one's outcome`() = runTest(dispatcher) {
        val source = FakeSource()
        val vm = OnchainReceiveViewModel(source)
        advanceUntilIdle()
        val subscription = source.subscriptions.single()

        val (a, depositA) = deposit("op-a")
        subscription.incoming.send(depositA)
        a.emit(claimed)
        advanceUntilIdle()
        assertEquals(claimed, vm.state.value.progress)

        // B arrives but its first state hasn't come yet.
        val (_, depositB) = deposit("op-b")
        subscription.incoming.send(depositB)
        advanceUntilIdle()

        val s = vm.state.value
        assertEquals("op-b", s.operationId)
        assertFalse("pending, not A's Received", s.progress!!.settled)
        assertTrue(s.steps.isEmpty())
    }

    @Test fun `a new deposit whose updates fail shows no stale success`() = runTest(dispatcher) {
        val source = FakeSource()
        val vm = OnchainReceiveViewModel(source)
        advanceUntilIdle()
        val subscription = source.subscriptions.single()

        val (a, depositA) = deposit("op-a")
        subscription.incoming.send(depositA)
        a.emit(claimed)
        advanceUntilIdle()

        subscription.incoming.send(OnchainReceiveViewModel.Deposit("op-b", flow { throw IllegalStateException("storage") }))
        advanceUntilIdle()

        val s = vm.state.value
        assertEquals("op-b", s.operationId)
        assertTrue(s.followFailed)
        assertFalse("never A's Received beside B's error", s.progress!!.settled)
    }

    @Test fun `a failed new address takes the old one off screen`() = runTest(dispatcher) {
        val source = FakeSource()
        val vm = OnchainReceiveViewModel(source)
        advanceUntilIdle()
        assertEquals("tb1qaddress1", vm.state.value.output?.text)

        source.failAddress = true
        vm.newAddress()
        advanceUntilIdle()

        val s = vm.state.value
        assertEquals("nothing watches the old address, so it isn't shown", null, s.output)
        assertEquals(null, s.progress)
        assertFalse("back to Try again", s.started)
        assertEquals("federation unreachable", s.error)
        assertTrue("the old subscription is closed", source.subscriptions.first().closed)
    }
}
