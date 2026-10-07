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

    private fun item(id: String) = ActivityItem(
        operationId = id,
        kind = OperationKind.LN_RECEIVE,
        time = 1uL,
        amount = 1_000uL,
        fee = 0uL,
        direction = Direction.INCOMING,
        status = ActivityStatus.SUCCESS,
        isFinal = true,
    )

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
}
