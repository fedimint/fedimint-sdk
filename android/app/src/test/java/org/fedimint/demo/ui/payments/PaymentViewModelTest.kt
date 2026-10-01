package org.fedimint.demo.ui.payments

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
    private class Vm : PaymentViewModel() {
        var sends = 0

        fun quoted() = mutableState.update { it.copy(review = Review(emptyList(), "100 sats", null)) }

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

    @Test fun `a second tap while sending does not send again`() = test { vm ->
        vm.quoted()
        vm.send(flow { emit("Paid") }, id = "op-1", before = { delay(1_000) })
        vm.send(flow { emit("Paid") }, id = "op-2")
        advanceUntilIdle()

        assertEquals(1, vm.sends)
        assertEquals("op-1", vm.state.value.operationId)
        assertNotNull(vm.state.value.progress)
    }
}
