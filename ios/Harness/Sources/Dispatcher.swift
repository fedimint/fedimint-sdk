import FedimintSdk
import Foundation

/// The protocol's methods (js/e2e/PROTOCOL.md), each a direct call into the
/// generated bindings. Parameters are read before any state is checked, so a
/// mistyped call answers HARNESS_BAD_PARAMS whatever the harness holds.
@MainActor
enum Dispatcher {
    typealias Handler = (HarnessSession, Params) async throws -> JSON

    static func call(_ s: HarnessSession, _ method: String, _ p: Params) async throws -> JSON {
        guard let handler = methods.first(where: { $0.0 == method })?.1 else {
            throw HarnessError(code: "HARNESS_UNKNOWN_METHOD", message: "no method \"\(method)\"")
        }
        return try await handler(s, p)
    }

    // In PROTOCOL.md's order, which `harness.describe` reports.
    static let methods: [(String, Handler)] = [
        ("harness.describe", { _, _ in describe() }),
        ("harness.state", { s, _ in
            [
                "sdkOpen": s.sdk != nil,
                "federationId": orNull(s.federation?.id()),
                "operations": s.operations.count,
                "quotes": s.quotes.count,
            ]
        }),
        ("harness.echo", { _, p in ["payload": try p.str("payload")] }),

        ("sdk.open", { s, p in try await open(s, words: p.optStrList("mnemonic")) }),
        ("sdk.mnemonic", { s, _ in ["words": try sdk(s).exportMnemonic().words()] }),
        ("invite.parse", { s, p in
            let code = try p.str("inviteCode")
            _ = try sdk(s)
            return ["federationId": try InviteCode.parse(code: code).federationId()]
        }),
        ("sdk.preview", { s, p in
            let code = try p.str("inviteCode")
            let sdk = try sdk(s)
            let preview = try await sdk.preview(invite: InviteCode.parse(code: code))
            return [
                "id": preview.id,
                "name": orNull(preview.name),
                "network": Encode.network(preview.network),
                "guardians": num(preview.guardians),
                "modules": preview.modules,
                "meta": preview.meta,
            ]
        }),
        ("sdk.join", { s, p in
            let code = try p.str("inviteCode")
            let sdk = try sdk(s)
            let joined = try await sdk.join(invite: InviteCode.parse(code: code))
            s.attach(joined)
            return ["federationId": joined.id()]
        }),

        ("federation.info", { s, _ in
            let f = try federation(s)
            let caps = f.capabilities()
            return [
                "id": f.id(),
                "name": orNull(f.name()),
                "network": Encode.network(f.network()),
                "capabilities": ["ecash": caps.ecash, "lightning": caps.lightning, "onchain": caps.onchain],
            ]
        }),
        ("federation.balance", { s, _ in ["amountMsats": try num(try await federation(s).balance())] }),
        ("federation.observedBalance", { s, _ in
            _ = try federation(s)
            return [
                "amountMsats": orNull(try s.balanceAmount.map(num)),
                "updates": s.balanceUpdates,
                "error": orNull(s.balanceError),
            ]
        }),

        ("lightning.receive", { s, p in
            let amount = try p.uint("amountMsats")
            let description = try p.str("description")
            let receive = try await lightning(s).receive(amount: amount, description: description)
            s.track(TypedOp(receive.operation))
            return ["invoice": receive.invoice, "operationId": receive.operation.id()]
        }),
        ("lightning.quote", { s, p in
            let invoice = try p.str("invoice")
            let quote = try await lightning(s).quote(invoice: invoice)
            let id = s.nextRef("q")
            s.quotes[id] = quote
            return [
                "quoteId": id,
                "invoiceAmountMsats": try num(quote.invoiceAmount()),
                "feeMsats": try num(quote.fee()),
                "totalMsats": try num(quote.total()),
                "route": Encode.route(quote.route()),
                "expiresAtMs": try num(quote.expiresAt()),
            ]
        }),
        ("lightning.send", { s, p in
            let quote: LnQuote = try held(s, p.str("quoteId"))
            let op = try await lightning(s).send(quote: quote)
            s.track(TypedOp(op))
            return ["operationId": op.id()]
        }),

        ("ecash.quote", { s, p in
            let amount = try p.uint("amountMsats")
            let quote = try await ecash(s).quote(amount: amount)
            let id = s.nextRef("q")
            s.quotes[id] = quote
            return [
                "quoteId": id,
                "requestedAmountMsats": try num(quote.requestedAmount()),
                "notesValueMsats": try num(quote.notesValue()),
                "feeMsats": try num(quote.fee()),
                "totalMsats": try num(quote.total()),
                "expiresAtMs": try num(quote.expiresAt()),
            ]
        }),
        ("ecash.send", { s, p in
            let quote: EcashQuote = try held(s, p.str("quoteId"))
            let sent = try await ecash(s).send(quote: quote)
            let ref = s.nextRef("n")
            s.notes[ref] = sent.notes
            s.track(TypedOp(sent.operation))
            return [
                "operationId": sent.operation.id(),
                "notesRef": ref,
                "notesValueMsats": try num(sent.notes.value()),
            ]
        }),
        ("ecash.receive", { s, p in
            let text = try p.optStr("notes")
            let ref = try p.optStr("notesRef")
            guard (text == nil) != (ref == nil) else {
                throw badParams("pass exactly one of \"notes\" and \"notesRef\"")
            }
            let ecash = try ecash(s)
            // A held note goes back out through display() and in through
            // parse(), so the string round trip is exercised either way.
            guard let source = text ?? ref.flatMap({ s.notes[$0]?.display() }) else {
                throw HarnessError(code: "HARNESS_UNKNOWN_REF", message: "no notes \"\(ref ?? "")\"")
            }
            let notes = try Notes.parse(notes: source)
            let value = notes.value()
            let op = try await ecash.receive(notes: notes)
            s.track(TypedOp(op))
            return ["operationId": op.id(), "notesValueMsats": try num(value)]
        }),

        ("onchain.receive", { s, _ in
            let onchain = try onchain(s)
            // deposits() first: it reports only deposits found after it is
            // opened, so opening it after showing the address could miss one.
            let opened = s.depositsTask == nil
            if opened { s.depositsTask = s.followDeposits(try await onchain.deposits()) }
            do {
                return ["address": try await onchain.receive()]
            } catch {
                if opened {
                    s.depositsTask?.cancel()
                    s.depositsTask = nil
                }
                throw error
            }
        }),
        ("onchain.deposits", { s, _ in
            _ = try federation(s)
            return ["operationIds": s.depositIds, "error": orNull(s.depositsError)]
        }),
        ("onchain.quote", { s, p in
            let address = try p.str("address")
            let amount = try p.uint("amountSats")
            let quote = try await onchain(s).quote(address: address, amount: amount)
            let id = s.nextRef("q")
            s.quotes[id] = quote
            return [
                "quoteId": id,
                "amountSats": try num(quote.amount()),
                "feeMsats": try num(quote.fee()),
                "totalMsats": try num(quote.total()),
                "expiresAtMs": try num(quote.expiresAt()),
            ]
        }),
        ("onchain.send", { s, p in
            let quote: OnchainQuote = try held(s, p.str("quoteId"))
            let op = try await onchain(s).send(quote: quote)
            s.track(TypedOp(op))
            return ["operationId": op.id()]
        }),

        ("operation.lookup", { s, p in
            let id = try p.str("operationId")
            guard let op = try await federation(s).operation(id: id) else { return ["found": false] }
            let typed = TypedOp(any: op)
            let state = try await typed?.state()
            if let typed { s.track(typed) }
            return [
                "found": true,
                "kind": Encode.kind(op.kind()),
                "support": Encode.support(op.support()),
                "state": orNull(state),
            ]
        }),
        ("operation.observed", { s, p in try record(s, p.str("operationId")).json }),
        ("operation.awaitFinal", { s, p in
            let record = try record(s, p.str("operationId"))
            let timeoutMs = try p.optUint("timeoutMs") ?? 120_000
            let state = try await firstOf(record.op.awaitFinal, orNilAfterMs: timeoutMs)
            guard let state else {
                throw HarnessError(code: "HARNESS_TIMEOUT", message: "not final within \(timeoutMs)ms")
            }
            return ["kind": Encode.kind(record.op.kind), "state": state]
        }),

        ("activity.list", { s, p in
            let limit = UInt16(min(try p.optUint("limit") ?? 20, UInt64(UInt16.max)))
            let page = try await federation(s).activity(cursor: p.optStr("cursor"), limit: limit)
            let items: [JSON] = try page.items.map { item in
                [
                    "operationId": item.operationId,
                    "kind": Encode.kind(item.kind),
                    "timeMs": try num(item.time),
                    "amountMsats": orNull(try item.amount.map(num)),
                    "feeMsats": orNull(try item.fee.map(num)),
                    "direction": orNull(item.direction.map(Encode.direction)),
                    "status": Encode.status(item.status),
                    "isFinal": item.isFinal,
                ]
            }
            return ["items": items, "next": orNull(page.next)]
        }),
    ]

    /// `work`'s result, or nil once `ms` have passed, whichever comes first.
    /// Not a task group: a group waits for every child, and an SDK call does
    /// not stop on Swift cancellation, so the timeout could never win. The
    /// call left behind keeps running until it ends on its own.
    private static func firstOf(
        _ work: @escaping () async throws -> JSON,
        orNilAfterMs ms: UInt64
    ) async throws -> JSON? {
        final class Once { var done = false }
        let once = Once()
        return try await withCheckedThrowingContinuation { continuation in
            // Both tasks run on the main actor, so `once` is never raced.
            Task { @MainActor in
                let outcome: Result<JSON?, Swift.Error>
                do { outcome = .success(try await work()) } catch { outcome = .failure(error) }
                guard !once.done else { return }
                once.done = true
                continuation.resume(with: outcome)
            }
            Task { @MainActor in
                try? await Task.sleep(nanoseconds: ms * 1_000_000)
                guard !once.done else { return }
                once.done = true
                continuation.resume(returning: nil)
            }
        }
    }

    private static func describe() -> JSON {
        [
            "protocolVersion": protocolVersion,
            "platform": "ios",
            "sdkVersion": fedimintSdkVersion,
            "methods": methods.map(\.0),
            "features": [String](),
        ]
    }

    /// `createFedimintSdk` over the harness's own directory, then the first
    /// federation it has joined, if any, so a joined federation survives a
    /// restart. With words, restores that seed instead.
    private static func open(_ s: HarnessSession, words: [String]?) async throws -> JSON {
        guard s.sdk == nil else { throw HarnessError(code: "HARNESS_STATE", message: "the SDK is already open") }
        let mnemonic = try words.map { try Mnemonic.fromWords(words: $0) }
        try FileManager.default.createDirectory(at: s.dataDir, withIntermediateDirectories: true)
        let opened = try await createFedimintSdk(dataDir: s.dataDir.path, mnemonic: mnemonic)
        s.sdk = opened
        let federations = opened.federations()
        if let first = federations.first { s.attach(first) }
        let ids = federations.map { $0.id() }
        return ["federationIds": ids, "attachedFederationId": orNull(ids.first)]
    }

    private static func sdk(_ s: HarnessSession) throws -> Sdk {
        guard let sdk = s.sdk else {
            throw HarnessError(code: "HARNESS_STATE", message: "the SDK is not open; call sdk.open")
        }
        return sdk
    }

    private static func federation(_ s: HarnessSession) throws -> Federation {
        guard let federation = s.federation else {
            throw HarnessError(code: "HARNESS_STATE", message: "no federation is attached")
        }
        return federation
    }

    private static func lightning(_ s: HarnessSession) throws -> Lightning {
        guard let module = try federation(s).lightning() else {
            throw HarnessError(code: "HARNESS_STATE", message: "no lightning module")
        }
        return module
    }

    private static func ecash(_ s: HarnessSession) throws -> Ecash {
        guard let module = try federation(s).ecash() else {
            throw HarnessError(code: "HARNESS_STATE", message: "no mint module")
        }
        return module
    }

    private static func onchain(_ s: HarnessSession) throws -> Onchain {
        guard let module = try federation(s).onchain() else {
            throw HarnessError(code: "HARNESS_STATE", message: "no wallet module")
        }
        return module
    }

    private static func held<Q>(_ s: HarnessSession, _ id: String) throws -> Q {
        guard let quote = s.quotes[id] else {
            throw HarnessError(code: "HARNESS_UNKNOWN_REF", message: "no quote \"\(id)\"")
        }
        guard let typed = quote as? Q else { throw badParams("quote \"\(id)\" is a \(type(of: quote))") }
        return typed
    }

    private static func record(_ s: HarnessSession, _ id: String) throws -> OpRecord {
        guard let record = s.operations[id] else {
            throw HarnessError(code: "HARNESS_UNKNOWN_REF", message: "no operation \"\(id)\" is tracked")
        }
        return record
    }
}
