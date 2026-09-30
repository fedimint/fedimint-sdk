package org.fedimint.demo.ui.payments

import org.fedimint.demo.ui.common.formatSats
import org.fedimint.sdk.EcashReceiveState
import org.fedimint.sdk.EcashSendState
import org.fedimint.sdk.LightningRoute
import org.fedimint.sdk.LnReceiveState
import org.fedimint.sdk.LnSendState
import org.fedimint.sdk.OnchainReceiveState
import org.fedimint.sdk.OnchainSendState
import org.fedimint.sdk.RecoveryState

/**
 * An operation's state, as the user should read it.
 *
 * @property settled no further change is expected from the user's point of view
 * @property ok for a settled state: whether it went the way the user wanted
 */
data class OpProgress(
    val label: String,
    val detail: String? = null,
    val settled: Boolean = false,
    val ok: Boolean = false,
)

private fun done(label: String, detail: String? = null) = OpProgress(label, detail, settled = true, ok = true)
private fun failed(label: String, detail: String? = null) = OpProgress(label, detail, settled = true, ok = false)

// Each `when` is exhaustive over the generated sealed class, so a state added
// to the SDK fails this build instead of rendering as nothing.

fun LnReceiveState.progress(): OpProgress = when (this) {
    LnReceiveState.Created -> OpProgress("Creating invoice…")
    LnReceiveState.WaitingForPayment -> OpProgress("Waiting for payment", "Share the invoice with the payer.")
    LnReceiveState.Funded -> OpProgress("Payment arrived, claiming it…")
    LnReceiveState.Claimed -> done("Received")
    is LnReceiveState.Canceled -> failed("Canceled", reason)
    LnReceiveState.Expired -> failed("Invoice expired", "Nobody paid it in time. Create a new one.")
    LnReceiveState.Failed -> failed("Receive failed")
}

fun LnSendState.progress(): OpProgress = when (this) {
    LnSendState.Created -> OpProgress("Starting payment…")
    LnSendState.Funded -> OpProgress("Paying…")
    is LnSendState.Success -> done("Paid", "Fee ${formatSats(fee)} sats · ${route.label()}")
    LnSendState.Refunded -> failed("Payment failed", "The funds came back to your balance.")
    is LnSendState.Failed -> failed("Payment failed", reason)
}

fun EcashReceiveState.progress(): OpProgress = when (this) {
    EcashReceiveState.Created, EcashReceiveState.Issuing -> OpProgress("Redeeming…")
    EcashReceiveState.Done -> done("Redeemed", "The notes are now in your balance.")
    is EcashReceiveState.Failed -> failed("Couldn't redeem", reason)
}

/**
 * For the sender, handing the notes over is the finish line, so CREATED counts
 * as done; whether the receiver has redeemed them yet is extra information.
 */
fun EcashSendState.progress(): OpProgress = when (this) {
    EcashSendState.CREATED -> done("Notes ready", "Hand them to the receiver. They can be reclaimed until redeemed.")
    EcashSendState.CANCEL_REQUESTED -> OpProgress("Reclaiming…")
    EcashSendState.CANCELED -> failed("Reclaimed", "The notes came back to your balance.")
    EcashSendState.REDEEMED -> done("Redeemed by the receiver")
}

fun OnchainReceiveState.progress(): OpProgress = when (this) {
    OnchainReceiveState.WaitingForTransaction -> OpProgress("Waiting for a deposit", "Send bitcoin to the address above.")
    is OnchainReceiveState.WaitingForConfirmation ->
        OpProgress("Deposit seen, waiting for confirmations", "${formatSats(grossDeposited * 1_000uL)} sats · tx $txid")
    is OnchainReceiveState.Confirmed -> OpProgress("Confirmed, claiming…", "tx $txid")
    is OnchainReceiveState.Claimed -> done("Received", "${formatSats(netCredit)} sats credited · tx $txid")
    is OnchainReceiveState.Failed -> failed("Deposit failed", reason)
}

fun OnchainSendState.progress(): OpProgress = when (this) {
    OnchainSendState.Created -> OpProgress("Broadcasting…")
    is OnchainSendState.Succeeded -> done("Sent", "tx $txid")
    is OnchainSendState.Refunded -> failed("Send failed", "Refunded to your balance. $reason")
    is OnchainSendState.Failed -> failed("Send failed", reason)
}

fun RecoveryState.progress(): OpProgress = when (this) {
    RecoveryState.Running -> OpProgress("Recovering", "Scanning the federation's history for funds that belong to your recovery phrase.")
    RecoveryState.Done -> done("Recovery complete")
    is RecoveryState.Failed -> failed("Recovery failed", reason)
}

fun LightningRoute.label(): String = when (this) {
    LightningRoute.Internal -> "within the federation, no gateway"
    is LightningRoute.Gateway -> "via a Lightning gateway"
}
