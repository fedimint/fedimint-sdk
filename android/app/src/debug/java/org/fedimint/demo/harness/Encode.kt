package org.fedimint.demo.harness

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonObjectBuilder
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put
import org.fedimint.sdk.EcashReceiveState
import org.fedimint.sdk.EcashSendState
import org.fedimint.sdk.LightningRoute
import org.fedimint.sdk.LnReceiveState
import org.fedimint.sdk.LnSendState
import org.fedimint.sdk.OnchainReceiveState
import org.fedimint.sdk.OnchainSendState
import org.fedimint.sdk.RecoveryState
import org.fedimint.sdk.Exception as SdkException

// The value encodings of js/e2e/PROTOCOL.md. Every `when` here is exhaustive,
// so a state variant added to the SDK fails this build until it is encoded.

/** A failure of the harness itself, answered with a HARNESS_* code. */
internal class HarnessError(val code: String, message: String) : RuntimeException(message)

internal fun badParams(message: String) = HarnessError("HARNESS_BAD_PARAMS", message)

/** The largest integer every platform's JSON reader holds exactly. */
private const val MAX_SAFE_INTEGER = 9_007_199_254_740_991uL

internal fun num(value: ULong): JsonPrimitive {
    if (value > MAX_SAFE_INTEGER) {
        throw HarnessError("HARNESS_INTERNAL", "$value is above 2^53-1")
    }
    return JsonPrimitive(value.toLong())
}

internal fun num(value: UInt) = JsonPrimitive(value.toLong())

internal fun num(value: UShort) = JsonPrimitive(value.toInt())

internal fun JsonObjectBuilder.putOrNull(key: String, value: JsonElement?) =
    put(key, value ?: JsonNull)

internal object Encode {
    fun error(e: SdkException) = error(e.code().name, e.reason())

    fun error(code: String, reason: String) = buildJsonObject {
        put("code", code)
        put("reason", reason)
    }

    private fun type(name: String, fields: JsonObjectBuilder.() -> Unit = {}) =
        buildJsonObject {
            put("type", name)
            fields()
        }

    fun route(route: LightningRoute): JsonObject = when (route) {
        is LightningRoute.Internal -> type("INTERNAL")
        is LightningRoute.Gateway -> type("GATEWAY") { put("gatewayId", route.gatewayId) }
    }

    // A fieldless Kotlin enum already carries the SCREAMING_SNAKE name.
    fun state(state: EcashSendState): JsonObject = type(state.name)

    fun state(state: EcashReceiveState): JsonObject = when (state) {
        is EcashReceiveState.Created -> type("CREATED")
        is EcashReceiveState.Issuing -> type("ISSUING")
        is EcashReceiveState.Done -> type("DONE")
        is EcashReceiveState.Failed -> type("FAILED") { put("reason", state.reason) }
    }

    fun state(state: LnSendState): JsonObject = when (state) {
        is LnSendState.Created -> type("CREATED")
        is LnSendState.Funded -> type("FUNDED")
        is LnSendState.Success -> type("SUCCESS") {
            put("preimage", state.preimage)
            put("feeMsats", num(state.fee))
            put("route", route(state.route))
        }
        is LnSendState.Refunded -> type("REFUNDED")
        is LnSendState.Failed -> type("FAILED") { put("reason", state.reason) }
    }

    fun state(state: LnReceiveState): JsonObject = when (state) {
        is LnReceiveState.Created -> type("CREATED")
        is LnReceiveState.WaitingForPayment -> type("WAITING_FOR_PAYMENT")
        is LnReceiveState.Funded -> type("FUNDED")
        is LnReceiveState.Claimed -> type("CLAIMED")
        is LnReceiveState.Canceled -> type("CANCELED") { put("reason", state.reason) }
        is LnReceiveState.Expired -> type("EXPIRED")
        is LnReceiveState.Failed -> type("FAILED")
    }

    fun state(state: OnchainSendState): JsonObject = when (state) {
        is OnchainSendState.Created -> type("CREATED")
        is OnchainSendState.Succeeded -> type("SUCCEEDED") { put("txid", state.txid) }
        is OnchainSendState.Refunded -> type("REFUNDED") { put("reason", state.reason) }
        is OnchainSendState.Failed -> type("FAILED") { put("reason", state.reason) }
    }

    fun state(state: OnchainReceiveState): JsonObject = when (state) {
        is OnchainReceiveState.Confirmed -> type("CONFIRMED") {
            put("txid", state.txid)
            put("grossDepositedSats", num(state.grossDeposited))
        }
        is OnchainReceiveState.Claimed -> type("CLAIMED") {
            put("txid", state.txid)
            put("grossDepositedSats", num(state.grossDeposited))
            put("netCreditMsats", num(state.netCredit))
        }
        is OnchainReceiveState.Failed -> type("FAILED") { put("reason", state.reason) }
    }

    fun state(state: RecoveryState): JsonObject = when (state) {
        is RecoveryState.Running -> type("RUNNING") {
            putOrNull(
                "progress",
                state.progress?.let {
                    buildJsonObject {
                        put("complete", num(it.complete))
                        put("total", num(it.total))
                    }
                },
            )
        }
        is RecoveryState.Done -> type("DONE")
        is RecoveryState.Failed -> type("FAILED") { put("reason", state.reason) }
    }
}

// ── Params ───────────────────────────────────────────────────────────────

internal fun JsonObject.str(key: String): String {
    val value = this[key] ?: throw badParams("missing \"$key\"")
    if (value !is JsonPrimitive || !value.isString) throw badParams("\"$key\" must be a string")
    return value.content
}

internal fun JsonObject.optStr(key: String): String? =
    if (this[key] == null || this[key] is JsonNull) null else str(key)

/** A non-negative integer: an amount, a limit or a timeout. */
internal fun JsonObject.uint(key: String): ULong {
    val value = this[key] ?: throw badParams("missing \"$key\"")
    val long = (value as? JsonPrimitive)?.takeIf { !it.isString }?.longOrNull
    if (long == null || long < 0) throw badParams("\"$key\" must be a non-negative integer")
    return long.toULong()
}

internal fun JsonObject.optUint(key: String): ULong? =
    if (this[key] == null || this[key] is JsonNull) null else uint(key)

internal fun JsonObject.optStrList(key: String): List<String>? {
    val value = this[key]
    if (value == null || value is JsonNull) return null
    val list = value as? kotlinx.serialization.json.JsonArray
        ?: throw badParams("\"$key\" must be an array of strings")
    return list.map {
        (it as? JsonPrimitive)?.takeIf { p -> p.isString }?.content
            ?: throw badParams("\"$key\" must be an array of strings")
    }
}
