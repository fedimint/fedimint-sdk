package org.fedimint.demo.wallet

import org.fedimint.sdk.FederationInfo
import org.fedimint.sdk.FederationStatus
import org.fedimint.sdk.Network
import org.junit.Assert.assertEquals
import org.junit.Test

/** A status update never reorders the list, so Home's fallback federation doesn't switch by itself. */
class FederationListTest {
    private fun fed(id: String, status: FederationStatus = FederationStatus.Running) =
        FederationInfo(id = id, name = id, network = Network.SIGNET, status = status)

    @Test fun `a status change keeps its place`() {
        val list = listOf(fed("a"), fed("b"))
        val closed = WalletSession.applyStatus(list, fed("a", FederationStatus.Closed))
        assertEquals(listOf("a", "b"), closed.map { it.id })
        assertEquals(FederationStatus.Closed, closed.first().status)

        val reopened = WalletSession.applyStatus(closed, fed("a"))
        assertEquals(listOf("a", "b"), reopened.map { it.id })
        assertEquals("the first open federation is still a", "a", WalletSession.pickActive(reopened, null)?.id)
    }

    @Test fun `a new federation is added at the end and a forgotten one removed`() {
        val added = WalletSession.applyStatus(listOf(fed("a")), fed("b"))
        assertEquals(listOf("a", "b"), added.map { it.id })
        assertEquals(listOf("b"), WalletSession.applyStatus(added, fed("a", FederationStatus.Forgotten)).map { it.id })
    }
}
