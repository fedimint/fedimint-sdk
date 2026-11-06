package org.fedimint.demo.harness

import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import java.io.File
import java.util.Collections
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.put
import org.fedimint.sdk.AnyOperation
import org.fedimint.sdk.EcashReceiveOperation
import org.fedimint.sdk.EcashSendOperation
import org.fedimint.sdk.Federation
import org.fedimint.sdk.LnReceiveOperation
import org.fedimint.sdk.LnSendOperation
import org.fedimint.sdk.Notes
import org.fedimint.sdk.OnchainDeposits
import org.fedimint.sdk.OnchainReceiveOperation
import org.fedimint.sdk.OnchainSendOperation
import org.fedimint.sdk.OperationKind
import org.fedimint.sdk.RecoveryOperation
import org.fedimint.sdk.Sdk
import org.fedimint.sdk.Exception as SdkException

/** js/e2e/PROTOCOL.md's version; a harness and the suite must agree on it. */
internal const val PROTOCOL_VERSION = 1

/**
 * One typed operation handle, reduced to what the protocol needs: its current
 * and final state, and a way to follow its `updates()` to the end.
 */
internal class TypedOp(
    val id: String,
    val kind: OperationKind,
    val state: suspend () -> JsonObject,
    val awaitFinal: suspend () -> JsonObject,
    val follow: suspend (emit: (JsonObject) -> Unit) -> Unit,
)

/** Reads `updates` until `next` returns null (the state is final), then closes it. */
private suspend fun <U : AutoCloseable, S : Any> followUpdates(
    updates: U,
    next: suspend (U) -> S?,
    emit: (S) -> Unit,
) {
    try {
        while (true) emit(next(updates) ?: break)
    } finally {
        updates.close()
    }
}

internal fun typed(op: EcashSendOperation) = TypedOp(
    op.id(), OperationKind.ECASH_SEND, { Encode.state(op.state()) }, { Encode.state(op.awaitFinal()) },
) { emit -> followUpdates(op.updates(), { it.next() }) { emit(Encode.state(it)) } }

internal fun typed(op: EcashReceiveOperation) = TypedOp(
    op.id(), OperationKind.ECASH_RECEIVE, { Encode.state(op.state()) }, { Encode.state(op.awaitFinal()) },
) { emit -> followUpdates(op.updates(), { it.next() }) { emit(Encode.state(it)) } }

internal fun typed(op: LnSendOperation) = TypedOp(
    op.id(), OperationKind.LN_SEND, { Encode.state(op.state()) }, { Encode.state(op.awaitFinal()) },
) { emit -> followUpdates(op.updates(), { it.next() }) { emit(Encode.state(it)) } }

internal fun typed(op: LnReceiveOperation) = TypedOp(
    op.id(), OperationKind.LN_RECEIVE, { Encode.state(op.state()) }, { Encode.state(op.awaitFinal()) },
) { emit -> followUpdates(op.updates(), { it.next() }) { emit(Encode.state(it)) } }

internal fun typed(op: OnchainSendOperation) = TypedOp(
    op.id(), OperationKind.ONCHAIN_SEND, { Encode.state(op.state()) }, { Encode.state(op.awaitFinal()) },
) { emit -> followUpdates(op.updates(), { it.next() }) { emit(Encode.state(it)) } }

internal fun typed(op: OnchainReceiveOperation) = TypedOp(
    op.id(), OperationKind.ONCHAIN_RECEIVE, { Encode.state(op.state()) }, { Encode.state(op.awaitFinal()) },
) { emit -> followUpdates(op.updates(), { it.next() }) { emit(Encode.state(it)) } }

internal fun typed(op: RecoveryOperation) = TypedOp(
    op.id(), OperationKind.RECOVERY, { Encode.state(op.state()) }, { Encode.state(op.awaitFinal()) },
) { emit -> followUpdates(op.updates(), { it.next() }) { emit(Encode.state(it)) } }

/** The typed handle behind a type-erased one, or null when this build has none. */
internal fun typedOf(op: AnyOperation): TypedOp? =
    op.asEcashSend()?.let { typed(it) }
        ?: op.asEcashReceive()?.let { typed(it) }
        ?: op.asLnSend()?.let { typed(it) }
        ?: op.asLnReceive()?.let { typed(it) }
        ?: op.asOnchainSend()?.let { typed(it) }
        ?: op.asOnchainReceive()?.let { typed(it) }
        ?: op.asRecovery()?.let { typed(it) }

/** What one operation's watcher has seen. */
internal class OpRecord(val op: TypedOp) {
    val states: MutableList<JsonObject> = Collections.synchronizedList(mutableListOf())
    @Volatile var final = false
    @Volatile var error: JsonObject? = null
    var job: Job? = null

    fun toJson() = buildJsonObject {
        put("kind", op.kind.name)
        put("states", JsonArray(synchronized(states) { states.toList() }))
        put("final", final)
        putOrNull("error", error)
    }
}

/** The balance as `balanceUpdates()` delivers it, seeded with one `balance()`. */
internal class BalanceRecord {
    @Volatile var amount: ULong? = null
    @Volatile var updates = 0
    @Volatile var error: JsonObject? = null
}

/**
 * The harness's state, for the life of the process: it outlives the activity
 * (a rotation keeps the open SDK) and dies with the process, which is what a
 * restart is.
 *
 * One call runs at a time, on `Dispatchers.IO`; the console itself is touched
 * only on the main thread. Watchers run alongside and only append to records.
 */
internal object HarnessSession {
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val main = Handler(Looper.getMainLooper())

    lateinit var dataDir: File
        private set

    /** The harness's storage, apart from the wallet's. Set once per process. */
    fun useFilesDir(filesDir: File) {
        if (!::dataDir.isInitialized) dataDir = File(filesDir, "console")
    }

    @Volatile var sdk: Sdk? = null
    @Volatile var federation: Federation? = null
    val quotes = ConcurrentHashMap<String, Any>()
    val notes = ConcurrentHashMap<String, Notes>()
    val operations = ConcurrentHashMap<String, OpRecord>()
    @Volatile var balance = BalanceRecord()
    private var balanceJob: Job? = null
    var depositsJob: Job? = null
    val depositIds: MutableList<String> = Collections.synchronizedList(mutableListOf())
    @Volatile var depositsError: JsonObject? = null
    private val refs = AtomicInteger()

    fun nextRef(prefix: String) = "$prefix${refs.incrementAndGet()}"

    // ── The console (main thread) ────────────────────────────────────────

    private var inFlight = false
    private var lastId: String? = null
    var lastResponse: String = buildJsonObject {
        put("v", PROTOCOL_VERSION)
        put("ready", true)
    }.toString()
        private set
    var listener: ((String) -> Unit)? = null

    /**
     * Takes the request text from a tap on `run` and returns what the response
     * view shows now: the pending marker, a cached answer for a repeated id,
     * or an error. The final answer arrives later through [listener].
     */
    fun submit(raw: String): String {
        val request = try {
            parse(raw)
        } catch (e: HarnessError) {
            return publish(failed(null, Encode.error(e.code, e.message ?: ""), null, raw.take(200)))
        }
        // A repeated id is a retried tap: answer it from the cache.
        if (request.id == lastId) return lastResponse
        if (inFlight) {
            return publish(
                failed(request.id, Encode.error("HARNESS_BUSY", "another call is still running"), 0),
            )
        }
        inFlight = true
        lastId = request.id
        publish(buildJsonObject {
            put("v", PROTOCOL_VERSION)
            put("id", request.id)
            put("pending", true)
        }.toString())

        scope.launch {
            val started = SystemClock.elapsedRealtime()
            val text = try {
                val result = Dispatcher.call(request.method, request.params)
                buildJsonObject {
                    put("v", PROTOCOL_VERSION)
                    put("id", request.id)
                    put("ok", true)
                    put("result", result)
                    put("ms", SystemClock.elapsedRealtime() - started)
                }.toString()
            } catch (e: Throwable) {
                failed(request.id, errorOf(e), SystemClock.elapsedRealtime() - started)
            }
            main.post {
                inFlight = false
                publish(text)
            }
        }
        return lastResponse
    }

    private fun publish(text: String): String {
        lastResponse = text
        listener?.invoke(text)
        return text
    }

    private class Request(val id: String, val method: String, val params: JsonObject)

    private fun parse(raw: String): Request {
        fun bad(message: String) = HarnessError("HARNESS_BAD_REQUEST", message)
        val obj = try {
            Json.parseToJsonElement(raw) as? JsonObject
        } catch (e: IllegalArgumentException) {
            null
        } ?: throw bad("not a JSON object")
        if ((obj["v"] as? JsonPrimitive)?.intOrNull != PROTOCOL_VERSION) {
            throw bad("expected protocol version $PROTOCOL_VERSION, got ${obj["v"]}")
        }
        fun string(key: String) = (obj[key] as? JsonPrimitive)?.takeIf { it.isString }?.content
            ?: throw bad("\"$key\" must be a string")
        val params = when (val p = obj["params"]) {
            null, JsonNull -> JsonObject(emptyMap())
            is JsonObject -> p
            else -> throw bad("\"params\" must be an object")
        }
        return Request(string("id"), string("method"), params)
    }

    private fun failed(id: String?, error: JsonObject, ms: Long?, received: String? = null) =
        buildJsonObject {
            put("v", PROTOCOL_VERSION)
            put("id", id)
            put("ok", false)
            put("error", error)
            ms?.let { put("ms", it) }
            received?.let { put("received", it) }
        }.toString()

    fun errorOf(e: Throwable): JsonObject = when (e) {
        is HarnessError -> Encode.error(e.code, e.message ?: "")
        is SdkException -> Encode.error(e)
        else -> Encode.error("HARNESS_INTERNAL", "${e::class.java.simpleName}: ${e.message}")
    }

    // ── SDK state (on the call's thread) ─────────────────────────────────

    /** Starts one watcher for an operation, or returns the one it has. */
    fun track(op: TypedOp): OpRecord {
        operations[op.id]?.let { return it }
        val record = OpRecord(op)
        operations[op.id] = record
        record.job = scope.launch {
            try {
                op.follow { record.states += it }
                record.final = true
            } catch (e: CancellationException) {
                throw e
            } catch (e: Throwable) {
                record.error = errorOf(e)
            }
        }
        return record
    }

    /**
     * Makes `joined` the current federation and subscribes to its balance.
     * Quotes, notes, operations and the deposit subscription belong to the
     * previous one, so they go with it.
     */
    fun attach(joined: Federation) {
        val previous = federation
        if (previous?.id() == joined.id()) return
        balanceJob?.cancel()
        depositsJob?.cancel()
        depositsJob = null
        operations.values.forEach { it.job?.cancel() }
        operations.clear()
        quotes.clear()
        notes.clear()
        depositIds.clear()
        depositsError = null
        federation = joined
        previous?.close()

        val record = BalanceRecord()
        balance = record
        balanceJob = scope.launch {
            try {
                record.amount = joined.balance()
                val updates = joined.balanceUpdates()
                try {
                    while (true) {
                        record.amount = updates.next()
                        record.updates++
                    }
                } finally {
                    updates.close()
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Throwable) {
                record.error = errorOf(e)
            }
        }
    }

    /**
     * Follows `deposits` for the federation's lifetime: each deposit the
     * wallet finds becomes an operation, tracked like any other.
     */
    fun followDeposits(deposits: OnchainDeposits): Job = scope.launch {
        try {
            while (true) {
                val op = deposits.next()
                depositIds += op.id()
                track(typed(op))
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Throwable) {
            depositsError = errorOf(e)
        } finally {
            deposits.close()
        }
    }

    /** Inline, so a method can call the SDK while it builds its result. */
    inline fun result(build: kotlinx.serialization.json.JsonObjectBuilder.() -> Unit): JsonElement =
        buildJsonObject(build)
}
