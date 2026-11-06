package org.fedimint.demo.harness

import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.fedimint.demo.harness.HarnessSession.result
import org.fedimint.sdk.Ecash
import org.fedimint.sdk.EcashQuote
import org.fedimint.sdk.Federation
import org.fedimint.sdk.InviteCode
import org.fedimint.sdk.Lightning
import org.fedimint.sdk.LnQuote
import org.fedimint.sdk.Mnemonic
import org.fedimint.sdk.Notes
import org.fedimint.sdk.Onchain
import org.fedimint.sdk.OnchainQuote
import org.fedimint.sdk.Sdk
import org.fedimint.sdk.createFedimintSdk

/**
 * The protocol's methods (js/e2e/PROTOCOL.md), each a direct call into the
 * generated bindings. Parameters are checked before any state, so a mistyped
 * call answers HARNESS_BAD_PARAMS whatever the harness holds.
 */
internal object Dispatcher {
    private val s = HarnessSession

    private val methods = LinkedHashMap<String, suspend (JsonObject) -> JsonElement>()

    private fun method(name: String, handler: suspend (params: JsonObject) -> JsonElement) {
        methods[name] = handler
    }

    init {
        method("harness.describe") { _ -> describe() }
        method("harness.state") { _ ->
            result {
                put("sdkOpen", s.sdk != null)
                put("federationId", s.federation?.id())
                put("operations", s.operations.size)
                put("quotes", s.quotes.size)
            }
        }
        method("harness.echo") { p -> result { put("payload", p.str("payload")) } }
        method("sdk.open") { p -> open(p.optStrList("mnemonic")) }
        method("sdk.mnemonic") { _ ->
            sdk().exportMnemonic().use { m -> result { put("words", strings(m.words())) } }
        }
        method("invite.parse") { p ->
            val code = p.str("inviteCode")
            sdk()
            InviteCode.parse(code).use { result { put("federationId", it.federationId()) } }
        }
        method("sdk.preview") { p ->
            val code = p.str("inviteCode")
            val sdk = sdk()
            val preview = InviteCode.parse(code).use { sdk.preview(it) }
            result {
                put("id", preview.id)
                put("name", preview.name)
                put("network", preview.network.name)
                put("guardians", num(preview.guardians))
                put("modules", strings(preview.modules))
                put("meta", JsonObject(preview.meta.mapValues { JsonPrimitive(it.value) }))
            }
        }
        method("sdk.join") { p ->
            val code = p.str("inviteCode")
            val sdk = sdk()
            val joined = InviteCode.parse(code).use { sdk.join(it) }
            s.attach(joined)
            result { put("federationId", joined.id()) }
        }

        method("federation.info") { _ ->
            val f = federation()
            val caps = f.capabilities()
            result {
                put("id", f.id())
                put("name", f.name())
                put("network", f.network().name)
                put("capabilities", buildJsonObject {
                    put("ecash", caps.ecash)
                    put("lightning", caps.lightning)
                    put("onchain", caps.onchain)
                })
            }
        }
        method("federation.balance") { _ -> result { put("amountMsats", num(federation().balance())) } }
        method("federation.observedBalance") { _ ->
            federation()
            val b = s.balance
            result {
                putOrNull("amountMsats", b.amount?.let { num(it) })
                put("updates", b.updates)
                putOrNull("error", b.error)
            }
        }

        method("lightning.receive") { p ->
            val amount = p.uint("amountMsats")
            val description = p.str("description")
            val receive = lightning().receive(amount, description)
            s.track(typed(receive.operation))
            result {
                put("invoice", receive.invoice)
                put("operationId", receive.operation.id())
            }
        }
        method("lightning.quote") { p ->
            val invoice = p.str("invoice")
            val quote = lightning().quote(invoice)
            val id = s.nextRef("q").also { s.quotes[it] = quote }
            result {
                put("quoteId", id)
                put("invoiceAmountMsats", num(quote.invoiceAmount()))
                put("feeMsats", num(quote.fee()))
                put("totalMsats", num(quote.total()))
                put("route", Encode.route(quote.route()))
                put("expiresAtMs", num(quote.expiresAt()))
            }
        }
        method("lightning.send") { p ->
            val quote = quote<LnQuote>(p.str("quoteId"))
            val op = lightning().send(quote)
            s.track(typed(op))
            result { put("operationId", op.id()) }
        }

        method("ecash.quote") { p ->
            val amount = p.uint("amountMsats")
            val quote = ecash().quote(amount)
            val id = s.nextRef("q").also { s.quotes[it] = quote }
            result {
                put("quoteId", id)
                put("requestedAmountMsats", num(quote.requestedAmount()))
                put("notesValueMsats", num(quote.notesValue()))
                put("feeMsats", num(quote.fee()))
                put("totalMsats", num(quote.total()))
                put("expiresAtMs", num(quote.expiresAt()))
            }
        }
        method("ecash.send") { p ->
            val quote = quote<EcashQuote>(p.str("quoteId"))
            val sent = ecash().send(quote)
            val ref = s.nextRef("n").also { s.notes[it] = sent.notes }
            s.track(typed(sent.operation))
            result {
                put("operationId", sent.operation.id())
                put("notesRef", ref)
                put("notesValueMsats", num(sent.notes.value()))
            }
        }
        method("ecash.receive") { p ->
            val text = p.optStr("notes")
            val ref = p.optStr("notesRef")
            if ((text == null) == (ref == null)) throw badParams("pass exactly one of \"notes\" and \"notesRef\"")
            val ecash = ecash()
            // A held note goes back out through display() and in through
            // parse(), so the string round trip is exercised either way.
            val source = text ?: s.notes[ref]?.display()
                ?: throw HarnessError("HARNESS_UNKNOWN_REF", "no notes \"$ref\"")
            val notes = Notes.parse(source)
            val value = notes.value()
            val op = notes.use { ecash.receive(it) }
            s.track(typed(op))
            result {
                put("operationId", op.id())
                put("notesValueMsats", num(value))
            }
        }

        method("onchain.receive") { _ ->
            val onchain = onchain()
            // deposits() first: it reports only deposits found after it is
            // opened, so opening it after showing the address could miss one.
            val opened = s.depositsJob == null
            if (opened) s.depositsJob = s.followDeposits(onchain.deposits())
            val address = try {
                onchain.receive()
            } catch (e: Throwable) {
                if (opened) {
                    s.depositsJob?.cancel()
                    s.depositsJob = null
                }
                throw e
            }
            result { put("address", address) }
        }
        method("onchain.deposits") { _ ->
            federation()
            result {
                put("operationIds", strings(synchronized(s.depositIds) { s.depositIds.toList() }))
                putOrNull("error", s.depositsError)
            }
        }
        method("onchain.quote") { p ->
            val address = p.str("address")
            val amount = p.uint("amountSats")
            val quote = onchain().quote(address, amount)
            val id = s.nextRef("q").also { s.quotes[it] = quote }
            result {
                put("quoteId", id)
                put("amountSats", num(quote.amount()))
                put("feeMsats", num(quote.fee()))
                put("totalMsats", num(quote.total()))
                put("expiresAtMs", num(quote.expiresAt()))
            }
        }
        method("onchain.send") { p ->
            val quote = quote<OnchainQuote>(p.str("quoteId"))
            val op = onchain().send(quote)
            s.track(typed(op))
            result { put("operationId", op.id()) }
        }

        method("operation.lookup") { p ->
            val id = p.str("operationId")
            val any = federation().operation(id)
            if (any == null) {
                result { put("found", false) }
            } else {
                any.use { op ->
                    val typed = typedOf(op)
                    val state = typed?.state?.invoke()
                    typed?.let { s.track(it) }
                    result {
                        put("found", true)
                        put("kind", op.kind().name)
                        put("support", op.support().name)
                        putOrNull("state", state)
                    }
                }
            }
        }
        method("operation.observed") { p -> record(p.str("operationId")).toJson() }
        method("operation.awaitFinal") { p ->
            val record = record(p.str("operationId"))
            val timeout = (p.optUint("timeoutMs") ?: 120_000uL).toLong()
            val state = withTimeoutOrNull(timeout) { record.op.awaitFinal() }
                ?: throw HarnessError("HARNESS_TIMEOUT", "not final within ${timeout}ms")
            result {
                put("kind", record.op.kind.name)
                put("state", state)
            }
        }

        method("activity.list") { p ->
            val limit = (p.optUint("limit") ?: 20uL).coerceAtMost(UShort.MAX_VALUE.toULong())
            val page = federation().activity(p.optStr("cursor"), limit.toUShort())
            result {
                put("items", JsonArray(page.items.map { item ->
                    buildJsonObject {
                        put("operationId", item.operationId)
                        put("kind", item.kind.name)
                        put("timeMs", num(item.time))
                        putOrNull("amountMsats", item.amount?.let { num(it) })
                        putOrNull("feeMsats", item.fee?.let { num(it) })
                        put("direction", item.direction?.name)
                        put("status", item.status.name)
                        put("isFinal", item.isFinal)
                    }
                }))
                put("next", page.next)
            }
        }
    }

    suspend fun call(method: String, params: JsonObject): JsonElement {
        val handler = methods[method]
            ?: throw HarnessError("HARNESS_UNKNOWN_METHOD", "no method \"$method\"")
        return handler(params)
    }

    private fun describe() = result {
        put("protocolVersion", PROTOCOL_VERSION)
        put("platform", "android")
        // The Kotlin bindings carry no version constant.
        putOrNull("sdkVersion", null)
        put("methods", strings(methods.keys.toList()))
        put("features", JsonArray(emptyList()))
    }

    /**
     * `createFedimintSdk` over the harness's own directory, then the first
     * federation it has joined, if any, so a joined federation survives a
     * restart. With words, restores that seed instead.
     */
    private suspend fun open(words: List<String>?): JsonElement {
        if (s.sdk != null) throw HarnessError("HARNESS_STATE", "the SDK is already open")
        val mnemonic = words?.let { Mnemonic.fromWords(it) }
        val opened = try {
            createFedimintSdk(s.dataDir.path, mnemonic)
        } finally {
            mnemonic?.close()
        }
        s.sdk = opened
        val federations = opened.federations()
        val ids = federations.map { it.id() }
        federations.firstOrNull()?.let { s.attach(it) }
        federations.drop(1).forEach { it.close() }
        return result {
            put("federationIds", strings(ids))
            put("attachedFederationId", ids.firstOrNull())
        }
    }

    private fun strings(values: List<String>) = JsonArray(values.map { JsonPrimitive(it) })

    private fun sdk(): Sdk =
        s.sdk ?: throw HarnessError("HARNESS_STATE", "the SDK is not open; call sdk.open")

    private fun federation(): Federation =
        s.federation ?: throw HarnessError("HARNESS_STATE", "no federation is attached")

    private fun lightning(): Lightning =
        federation().lightning() ?: throw HarnessError("HARNESS_STATE", "no lightning module")

    private fun ecash(): Ecash =
        federation().ecash() ?: throw HarnessError("HARNESS_STATE", "no mint module")

    private fun onchain(): Onchain =
        federation().onchain() ?: throw HarnessError("HARNESS_STATE", "no wallet module")

    private inline fun <reified Q> quote(id: String): Q {
        val quote = s.quotes[id] ?: throw HarnessError("HARNESS_UNKNOWN_REF", "no quote \"$id\"")
        return quote as? Q ?: throw badParams("quote \"$id\" is a ${quote::class.java.simpleName}")
    }

    private fun record(id: String): OpRecord =
        s.operations[id] ?: throw HarnessError("HARNESS_UNKNOWN_REF", "no operation \"$id\" is tracked")
}
