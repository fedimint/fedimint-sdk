package org.fedimint.demo.wallet

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.withContext
import org.fedimint.sdk.Amount
import org.fedimint.sdk.Ecash
import org.fedimint.sdk.EcashQuote
import org.fedimint.sdk.EcashReceiveOperation
import org.fedimint.sdk.EcashSendHandle
import org.fedimint.sdk.Federation
import org.fedimint.sdk.FederationId
import org.fedimint.sdk.Lightning
import org.fedimint.sdk.LnQuote
import org.fedimint.sdk.LnReceiveHandle
import org.fedimint.sdk.LnSendOperation
import org.fedimint.sdk.Notes
import org.fedimint.sdk.Onchain
import org.fedimint.sdk.OnchainQuote
import org.fedimint.sdk.OnchainReceiveHandle
import org.fedimint.sdk.OnchainSendOperation
import org.fedimint.sdk.Sats

/**
 * Sending and receiving, over the three payment modules a federation may
 * offer. Every send is two calls, `quote` then `send(quote)`, so the user can
 * see the fee and approve it before anything moves; a quote is single use and
 * expires. Every receive returns an operation to follow with [states].
 *
 * The returned quotes, handles and operations are the caller's to close.
 */
class Payments(private val session: WalletSession) {

    // ── Lightning ────────────────────────────────────────────────────────

    suspend fun lightningReceive(id: FederationId, amount: Amount, description: String): LnReceiveHandle =
        lightning(id) { it.receive(amount, description) }

    suspend fun lightningQuote(id: FederationId, invoice: String): LnQuote =
        lightning(id) { it.quote(invoice.trim().removePrefix("lightning:")) }

    suspend fun lightningSend(id: FederationId, quote: LnQuote): LnSendOperation =
        lightning(id) { it.send(quote) }

    // ── Ecash ────────────────────────────────────────────────────────────

    suspend fun ecashQuote(id: FederationId, amount: Amount): EcashQuote = ecash(id) { it.quote(amount) }

    suspend fun ecashSend(id: FederationId, quote: EcashQuote): EcashSendHandle = ecash(id) { it.send(quote) }

    /** Parses notes someone handed over. Throws INVALID_INPUT for anything that isn't notes. */
    suspend fun parseNotes(text: String): Notes = withContext(Dispatchers.IO) { Notes.parse(text.trim()) }

    suspend fun ecashReceive(id: FederationId, notes: Notes): EcashReceiveOperation = ecash(id) { it.receive(notes) }

    // ── On-chain ─────────────────────────────────────────────────────────

    suspend fun onchainReceive(id: FederationId): OnchainReceiveHandle = onchain(id) { it.receive() }

    suspend fun onchainQuote(id: FederationId, address: String, amount: Sats): OnchainQuote =
        onchain(id) { it.quote(address.trim().removePrefix("bitcoin:"), amount) }

    suspend fun onchainSend(id: FederationId, quote: OnchainQuote): OnchainSendOperation =
        onchain(id) { it.send(quote) }

    // ── Plumbing ─────────────────────────────────────────────────────────

    private suspend fun <T> lightning(id: FederationId, block: suspend (Lightning) -> T): T =
        module(id, "Lightning", Federation::lightning, block)

    private suspend fun <T> ecash(id: FederationId, block: suspend (Ecash) -> T): T =
        module(id, "ecash", Federation::ecash, block)

    private suspend fun <T> onchain(id: FederationId, block: suspend (Onchain) -> T): T =
        module(id, "on-chain", Federation::onchain, block)

    /** A module facade is null when the federation doesn't run that module. */
    private suspend fun <M : AutoCloseable, T> module(
        id: FederationId,
        name: String,
        facade: (Federation) -> M?,
        block: suspend (M) -> T,
    ): T = session.withFederation(id) { federation ->
        val module = facade(federation) ?: throw IllegalStateException("This federation doesn't offer $name.")
        module.use { block(it) }
    }

    companion object {
        /**
         * An operation's states as a flow: the current state first, then each
         * change, completing once the state is final (`next()` returns null).
         *
         * Takes the subscription factory, not a subscription, and opens it once:
         * one subscription is one cursor, and re-creating it per value would
         * replay the current state forever. Closed when collection ends.
         */
        fun <S : Any, U : AutoCloseable> states(subscribe: () -> U, next: suspend (U) -> S?): Flow<S> = flow {
            subscribe().use { updates ->
                while (true) emit(next(updates) ?: break)
            }
        }.flowOn(Dispatchers.IO)
    }
}
