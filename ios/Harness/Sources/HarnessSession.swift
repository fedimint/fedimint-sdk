import FedimintSdk
import Foundation

/// js/e2e/PROTOCOL.md's version; a harness and the suite must agree on it.
let protocolVersion = 1

/// One typed operation handle, reduced to what the protocol needs: its
/// current and final state, and a way to follow its `updates()` to the end.
struct TypedOp {
    let id: String
    let kind: OperationKind
    let state: () async throws -> JSON
    let awaitFinal: () async throws -> JSON
    /// Delivers each state `updates()` yields until the subscription ends.
    let follow: (_ emit: @escaping @MainActor (JSON) -> Void) async throws -> Void
}

extension TypedOp {
    init(_ op: EcashSendOperation) {
        self.init(
            id: op.id(), kind: .ecashSend,
            state: { Encode.state(try await op.state()) },
            awaitFinal: { Encode.state(try await op.awaitFinal()) },
            follow: { emit in
                let updates = op.updates()
                while let s = try await updates.next() { await emit(Encode.state(s)) }
            })
    }

    init(_ op: EcashReceiveOperation) {
        self.init(
            id: op.id(), kind: .ecashReceive,
            state: { Encode.state(try await op.state()) },
            awaitFinal: { Encode.state(try await op.awaitFinal()) },
            follow: { emit in
                let updates = op.updates()
                while let s = try await updates.next() { await emit(Encode.state(s)) }
            })
    }

    init(_ op: LnSendOperation) {
        self.init(
            id: op.id(), kind: .lnSend,
            state: { try Encode.state(try await op.state()) },
            awaitFinal: { try Encode.state(try await op.awaitFinal()) },
            follow: { emit in
                let updates = op.updates()
                while let s = try await updates.next() { await emit(try Encode.state(s)) }
            })
    }

    init(_ op: LnReceiveOperation) {
        self.init(
            id: op.id(), kind: .lnReceive,
            state: { Encode.state(try await op.state()) },
            awaitFinal: { Encode.state(try await op.awaitFinal()) },
            follow: { emit in
                let updates = op.updates()
                while let s = try await updates.next() { await emit(Encode.state(s)) }
            })
    }

    init(_ op: OnchainSendOperation) {
        self.init(
            id: op.id(), kind: .onchainSend,
            state: { Encode.state(try await op.state()) },
            awaitFinal: { Encode.state(try await op.awaitFinal()) },
            follow: { emit in
                let updates = op.updates()
                while let s = try await updates.next() { await emit(Encode.state(s)) }
            })
    }

    init(_ op: OnchainReceiveOperation) {
        self.init(
            id: op.id(), kind: .onchainReceive,
            state: { try Encode.state(try await op.state()) },
            awaitFinal: { try Encode.state(try await op.awaitFinal()) },
            follow: { emit in
                let updates = op.updates()
                while let s = try await updates.next() { await emit(try Encode.state(s)) }
            })
    }

    init(_ op: RecoveryOperation) {
        self.init(
            id: op.id(), kind: .recovery,
            state: { Encode.state(try await op.state()) },
            awaitFinal: { Encode.state(try await op.awaitFinal()) },
            follow: { emit in
                let updates = op.updates()
                while let s = try await updates.next() { await emit(Encode.state(s)) }
            })
    }

    /// The typed handle behind a type-erased one, or nil when this build has none.
    init?(any op: AnyOperation) {
        if let h = op.asEcashSend() { self.init(h) }
        else if let h = op.asEcashReceive() { self.init(h) }
        else if let h = op.asLnSend() { self.init(h) }
        else if let h = op.asLnReceive() { self.init(h) }
        else if let h = op.asOnchainSend() { self.init(h) }
        else if let h = op.asOnchainReceive() { self.init(h) }
        else if let h = op.asRecovery() { self.init(h) }
        else { return nil }
    }
}

/// What one operation's watcher has seen.
@MainActor
final class OpRecord {
    let op: TypedOp
    var states: [JSON] = []
    var final = false
    var error: JSON?
    var task: Task<Void, Never>?

    init(_ op: TypedOp) { self.op = op }

    var json: JSON {
        ["kind": Encode.kind(op.kind), "states": states, "final": final, "error": orNull(error)]
    }
}

/**
 * The harness's state, for the life of the process, and the console's request
 * handling. Main-actor isolated: SDK calls suspend rather than block, so one
 * call runs at a time while watchers keep appending to their records.
 */
@MainActor
final class HarnessSession: ObservableObject {
    @Published private(set) var response = HarnessSession.render(["v": protocolVersion, "ready": true])

    let dataDir: URL = FileManager.default
        .urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        .appendingPathComponent("harness")

    var sdk: Sdk?
    var federation: Federation?
    var quotes: [String: AnyObject] = [:]
    var notes: [String: Notes] = [:]
    var operations: [String: OpRecord] = [:]
    var balanceAmount: UInt64?
    var balanceUpdates = 0
    var balanceError: JSON?
    var depositsTask: Task<Void, Never>?
    var depositIds: [String] = []
    var depositsError: JSON?
    private var balanceTask: Task<Void, Never>?
    private var refs = 0
    private var inFlight = false
    private var lastId: String?

    func nextRef(_ prefix: String) -> String {
        refs += 1
        return "\(prefix)\(refs)"
    }

    // ── The console ──────────────────────────────────────────────────────

    /// Takes the request text from a tap on `run`. The pending marker, a
    /// cached answer for a repeated id, or an error shows at once; the final
    /// answer follows when the call returns.
    func submit(_ raw: String) {
        let request: (id: String, method: String, params: JSON)
        do {
            request = try parse(raw)
        } catch {
            publish(failed(nil, Encode.error(error), ms: nil, received: String(raw.prefix(200))))
            return
        }
        // A repeated id is a retried tap: leave its answer showing.
        if request.id == lastId { return }
        if inFlight {
            publish(failed(request.id, Encode.error("HARNESS_BUSY", "another call is still running"), ms: 0))
            return
        }
        inFlight = true
        lastId = request.id
        publish(["v": protocolVersion, "id": request.id, "pending": true])

        Task {
            let started = Date()
            var answer: JSON
            do {
                let result = try await Dispatcher.call(self, request.method, Params(values: request.params))
                answer = ["v": protocolVersion, "id": request.id, "ok": true, "result": result]
            } catch {
                answer = failed(request.id, Encode.error(error), ms: nil)
            }
            answer["ms"] = Int(Date().timeIntervalSince(started) * 1000)
            inFlight = false
            publish(answer)
        }
    }

    private func publish(_ envelope: JSON) {
        response = Self.render(envelope)
    }

    private static func render(_ envelope: JSON) -> String {
        do {
            let data = try JSONSerialization.data(withJSONObject: envelope, options: [.withoutEscapingSlashes])
            return String(decoding: data, as: UTF8.self)
        } catch {
            return #"{"v":1,"id":null,"ok":false,"error":{"code":"HARNESS_INTERNAL","reason":"unencodable answer"}}"#
        }
    }

    private func parse(_ raw: String) throws -> (id: String, method: String, params: JSON) {
        func bad(_ message: String) -> HarnessError { HarnessError(code: "HARNESS_BAD_REQUEST", message: message) }
        guard let object = try? JSONSerialization.jsonObject(with: Data(raw.utf8)) as? JSON else {
            throw bad("not a JSON object")
        }
        guard let v = object["v"] as? NSNumber, CFGetTypeID(v) != CFBooleanGetTypeID(),
              v.intValue == protocolVersion
        else { throw bad("expected protocol version \(protocolVersion)") }
        guard let id = object["id"] as? String else { throw bad("\"id\" must be a string") }
        guard let method = object["method"] as? String else { throw bad("\"method\" must be a string") }
        let params: JSON
        switch object["params"] {
        case nil, is NSNull: params = [:]
        case let p as JSON: params = p
        default: throw bad("\"params\" must be an object")
        }
        return (id, method, params)
    }

    private func failed(_ id: String?, _ error: JSON, ms: Int?, received: String? = nil) -> JSON {
        var envelope: JSON = ["v": protocolVersion, "id": orNull(id), "ok": false, "error": error]
        if let ms { envelope["ms"] = ms }
        if let received { envelope["received"] = received }
        return envelope
    }

    // ── SDK state ────────────────────────────────────────────────────────

    /// Starts one watcher for an operation, or returns the one it has.
    @discardableResult
    func track(_ op: TypedOp) -> OpRecord {
        if let existing = operations[op.id] { return existing }
        let record = OpRecord(op)
        operations[op.id] = record
        record.task = Task { [weak record] in
            do {
                try await op.follow { state in record?.states.append(state) }
                record?.final = true
            } catch is CancellationError {
            } catch {
                record?.error = Encode.error(error)
            }
        }
        return record
    }

    /// Makes `joined` the current federation and subscribes to its balance.
    /// Quotes, notes, operations and the deposit subscription belong to the
    /// previous one, so they go with it.
    func attach(_ joined: Federation) {
        if federation?.id() == joined.id() { return }
        balanceTask?.cancel()
        depositsTask?.cancel()
        depositsTask = nil
        operations.values.forEach { $0.task?.cancel() }
        operations = [:]
        quotes = [:]
        notes = [:]
        depositIds = []
        depositsError = nil
        federation = joined

        balanceAmount = nil
        balanceUpdates = 0
        balanceError = nil
        balanceTask = Task { [weak self] in
            do {
                let initial = try await joined.balance()
                self?.balanceAmount = initial
                let updates = joined.balanceUpdates()
                while !Task.isCancelled {
                    let next = try await updates.next()
                    self?.balanceAmount = next
                    self?.balanceUpdates += 1
                }
            } catch is CancellationError {
            } catch {
                self?.balanceError = Encode.error(error)
            }
        }
    }

    /// Follows `deposits` for the federation's lifetime: each deposit the
    /// wallet finds becomes an operation, tracked like any other.
    func followDeposits(_ deposits: OnchainDeposits) -> Task<Void, Never> {
        Task { [weak self] in
            do {
                while !Task.isCancelled {
                    let op = try await deposits.next()
                    self?.depositIds.append(op.id())
                    self?.track(TypedOp(op))
                }
            } catch is CancellationError {
            } catch {
                self?.depositsError = Encode.error(error)
            }
        }
    }
}
