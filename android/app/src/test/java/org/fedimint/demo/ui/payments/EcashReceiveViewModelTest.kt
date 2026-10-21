package org.fedimint.demo.ui.payments

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test

/** A failed redeem can be retried with the same text, by checking again; never by itself. */
@OptIn(ExperimentalCoroutinesApi::class)
class EcashReceiveViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @Before fun setUp() = Dispatchers.setMain(dispatcher)

    @After fun tearDown() = Dispatchers.resetMain()

    /** Parses anything as 800 sats; the first redeem fails, later ones succeed. */
    private class FakeSource : EcashReceiveViewModel.Source {
        var parses = 0
        var redeems = 0

        override suspend fun parse(text: String): EcashReceiveViewModel.Parsed {
            parses++
            return EcashReceiveViewModel.Parsed(800_000uL, handle = null) {
                redeems++
                if (redeems == 1) throw IllegalStateException("storage busy")
                EcashReceiveViewModel.Redemption("op-$redeems", flowOf(OpProgress("Redeemed", settled = true, ok = true)), handle = null)
            }
        }
    }

    @Test fun `failure, check again, then success with identical text`() = runTest(dispatcher) {
        val source = FakeSource()
        val vm = EcashReceiveViewModel(source)
        vm.onNotesChanged("cashuNotes")
        vm.check()
        advanceUntilIdle()
        assertEquals("800 sats", vm.state.value.review?.total)

        vm.redeem()
        advanceUntilIdle()
        assertNull("the failed redeem consumed the review", vm.state.value.review)
        assertEquals("storage busy", vm.state.value.error)
        assertNull(vm.state.value.operationId)

        // Redeem without checking again does nothing: the parsed notes were used up.
        vm.redeem()
        advanceUntilIdle()
        assertEquals(1, source.redeems)

        vm.check() // the screen's "Check notes again", same text
        advanceUntilIdle()
        assertEquals(2, source.parses)
        assertNotNull("ready to redeem again", vm.state.value.review)
        assertEquals("checking doesn't redeem", 1, source.redeems)

        vm.redeem()
        advanceUntilIdle()
        assertEquals("op-2", vm.state.value.operationId)
        assertEquals("Redeemed", vm.state.value.progress?.label)
        assertEquals("cashuNotes", vm.notesText.value)
    }

    @Test fun `notes parsed again release the replaced copy at once`() = runTest(dispatcher) {
        class Handle : AutoCloseable {
            var closed = false

            override fun close() {
                closed = true
            }
        }
        val handles = mutableListOf<Handle>()
        val source = object : EcashReceiveViewModel.Source {
            override suspend fun parse(text: String) = Handle().let { h ->
                handles += h
                EcashReceiveViewModel.Parsed(1_000uL, h) { error("not redeemed here") }
            }
        }
        val vm = EcashReceiveViewModel(source)
        vm.onNotesChanged("notes")
        vm.check()
        advanceUntilIdle()
        vm.check()
        advanceUntilIdle()
        assertEquals("the first parse is closed once replaced", listOf(true, false), handles.map { it.closed })

        vm.onNotesChanged("other notes")
        assertEquals(listOf(true, true), handles.map { it.closed })
    }
}
