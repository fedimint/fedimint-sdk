package org.fedimint.demo.ui.home

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.fedimint.sdk.ActivityItem
import org.fedimint.sdk.ActivityStatus
import org.fedimint.sdk.Amount
import org.fedimint.sdk.Capabilities
import org.fedimint.sdk.Direction
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.FederationInfo
import org.fedimint.sdk.FederationStatus
import org.fedimint.sdk.Network
import org.fedimint.sdk.OperationKind
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * Switching federation never shows one federation's balance, activity or
 * capabilities under another's name, however slow the new reads are.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class HomeViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @Before fun setUp() = Dispatchers.setMain(dispatcher)

    @After fun tearDown() = Dispatchers.resetMain()

    private val capsA = Capabilities(ecash = true, lightning = true, onchain = true)
    private val capsB = Capabilities(ecash = true, lightning = false, onchain = false)

    /** A reads instantly; B's reads wait for [releaseB]. */
    private inner class Source : HomeViewModel.Source {
        val releaseB = CompletableDeferred<Unit>()
        val selectedId = MutableStateFlow<FederationId?>("A")

        override val federations = MutableStateFlow<List<FederationInfo>?>(listOf(fed("A"), fed("B")))
        override val selected: Flow<FederationId?> = selectedId

        override fun balance(id: FederationId): Flow<Amount> = flow {
            if (id == "B") releaseB.await()
            emit(if (id == "A") 100_000uL else 5_000uL)
            awaitCancellation()
        }

        override suspend fun capabilities(id: FederationId): Capabilities {
            if (id == "B") releaseB.await()
            return if (id == "A") capsA else capsB
        }

        override suspend fun recent(id: FederationId): List<ActivityItem> {
            if (id == "B") releaseB.await()
            return listOf(item("op-$id"))
        }

        override fun select(id: FederationId) {
            selectedId.value = id
        }
    }

    private fun fed(id: String) =
        FederationInfo(id = id, name = "Federation $id", network = Network.SIGNET, status = FederationStatus.Running)

    private fun item(id: String, final: Boolean = true) = ActivityItem(
        operationId = id,
        kind = OperationKind.ECASH_SEND,
        time = 1uL,
        amount = 1_000uL,
        fee = 0uL,
        direction = Direction.OUTGOING,
        status = if (final) ActivityStatus.SUCCESS else ActivityStatus.PENDING,
        isFinal = final,
    )

    /** One federation whose balance never changes, and scripted history reads. */
    private inner class QuietSource(vararg pages: List<ActivityItem>) : HomeViewModel.Source {
        private val pages = ArrayDeque(pages.toList())
        var reads = 0

        override val federations = MutableStateFlow<List<FederationInfo>?>(listOf(fed("A")))
        override val selected: Flow<FederationId?> = MutableStateFlow("A")

        override fun balance(id: FederationId): Flow<Amount> = flow {
            emit(1_000uL) // once: the balance doesn't move in these tests
            awaitCancellation()
        }

        override suspend fun capabilities(id: FederationId) = capsA

        override suspend fun recent(id: FederationId): List<ActivityItem> {
            reads++
            return if (pages.size > 1) pages.removeFirst() else pages.first()
        }

        override fun select(id: FederationId) = Unit
    }

    @Test fun `switching federation never shows the previous one's data`() = runTest(dispatcher) {
        val source = Source()
        val vm = HomeViewModel(source)
        val states = mutableListOf<HomeViewModel.UiState>()
        backgroundScope.launch { vm.state.collect { states += it } }
        advanceUntilIdle()

        val onA = vm.state.value
        assertEquals("A", onA.active?.id)
        assertEquals(HomeViewModel.Balance.Value(100_000uL), onA.balance)
        assertEquals(listOf("op-A"), onA.recent.map { it.operationId })

        source.select("B") // B's reads are held back
        advanceUntilIdle()
        val whileLoading = vm.state.value
        assertEquals("B", whileLoading.active?.id)
        assertEquals(HomeViewModel.Balance.Loading, whileLoading.balance)
        assertTrue(whileLoading.recent.isEmpty())

        source.releaseB.complete(Unit)
        advanceUntilIdle()
        val onB = vm.state.value
        assertEquals(HomeViewModel.Balance.Value(5_000uL), onB.balance)
        assertEquals(listOf("op-B"), onB.recent.map { it.operationId })
        assertEquals(capsB, onB.capabilities)

        // Not just the settled states: no state ever emitted mixes the two.
        for (s in states.filter { it.active?.id == "B" }) {
            assertNotEquals(HomeViewModel.Balance.Value(100_000uL), s.balance)
            assertTrue(s.recent.none { it.operationId == "op-A" })
            assertNotEquals(capsA, s.capabilities)
        }
    }

    @Test fun `a pending row settles on Home without the balance moving`() = runTest(dispatcher) {
        // An ecash send: debited when sent, so redemption leaves the balance unchanged.
        val source = QuietSource(listOf(item("op-1", final = false)), listOf(item("op-1", final = true)))
        val vm = HomeViewModel(source)
        backgroundScope.launch { vm.state.collect {} }
        advanceUntilIdle() // includes the pending-row refresh timer

        val row = vm.state.value.recent.single()
        assertTrue("settled", row.isFinal)
        assertEquals(ActivityStatus.SUCCESS, row.status)
        assertEquals("reloaded once more, then stopped", 2, source.reads)
    }

    @Test fun `an operation created elsewhere shows when Home is back in view`() = runTest(dispatcher) {
        // e.g. opening On-chain Receive creates a deposit, then the user goes back.
        val source = QuietSource(emptyList(), listOf(item("op-new")))
        val vm = HomeViewModel(source)
        backgroundScope.launch { vm.state.collect {} }
        advanceUntilIdle()
        assertTrue(vm.state.value.recent.isEmpty())

        vm.refresh() // what Home resuming does
        advanceUntilIdle()
        assertEquals(listOf("op-new"), vm.state.value.recent.map { it.operationId })
    }
}
