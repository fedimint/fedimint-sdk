import FedimintSdk
import Foundation

// The value encodings of js/e2e/PROTOCOL.md. Every `switch` here is
// exhaustive, so a variant added to the SDK fails this build until it is
// encoded. Values are Foundation JSON objects: [String: Any], [Any], String,
// NSNumber, NSNull.

typealias JSON = [String: Any]

/// A failure of the harness itself, answered with a HARNESS_* code.
struct HarnessError: Swift.Error {
    let code: String
    let message: String
}

func badParams(_ message: String) -> HarnessError {
    HarnessError(code: "HARNESS_BAD_PARAMS", message: message)
}

/// The largest integer every platform's JSON reader holds exactly.
private let maxSafeInteger: UInt64 = 9_007_199_254_740_991

func num(_ value: UInt64) throws -> NSNumber {
    guard value <= maxSafeInteger else {
        throw HarnessError(code: "HARNESS_INTERNAL", message: "\(value) is above 2^53-1")
    }
    return NSNumber(value: value)
}

func num(_ value: UInt32) -> NSNumber { NSNumber(value: value) }
func num(_ value: UInt16) -> NSNumber { NSNumber(value: value) }

/// `value`, or JSON null.
func orNull(_ value: Any?) -> Any { value ?? NSNull() }

enum Encode {
    static func error(_ e: SdkError) -> JSON { error(code(e.code()), e.reason()) }

    static func error(_ code: String, _ reason: String) -> JSON {
        ["code": code, "reason": reason]
    }

    /// Any thrown error as the protocol's `{code, reason}`.
    static func error(_ e: Swift.Error) -> JSON {
        if let e = e as? HarnessError { return error(e.code, e.message) }
        if let e = e as? SdkError { return error(e) }
        return error("HARNESS_INTERNAL", "\(type(of: e)): \(e)")
    }

    static func code(_ code: ErrorCode) -> String {
        switch code {
        case .invalidInput: "INVALID_INPUT"
        case .alreadyJoined: "ALREADY_JOINED"
        case .unsupportedFederation: "UNSUPPORTED_FEDERATION"
        case .federationUnreachable: "FEDERATION_UNREACHABLE"
        case .insufficientBalance: "INSUFFICIENT_BALANCE"
        case .balanceNotEmpty: "BALANCE_NOT_EMPTY"
        case .pendingOperations: "PENDING_OPERATIONS"
        case .gatewayUnavailable: "GATEWAY_UNAVAILABLE"
        case .recovering: "RECOVERING"
        case .notSupported: "NOT_SUPPORTED"
        case .unsupportedOperation: "UNSUPPORTED_OPERATION"
        case .seedMismatch: "SEED_MISMATCH"
        case .storageInUse: "STORAGE_IN_USE"
        case .storageOrphaned: "STORAGE_ORPHANED"
        case .quoteExpired: "QUOTE_EXPIRED"
        case .quoteChanged: "QUOTE_CHANGED"
        case .amountlessInvoice: "AMOUNTLESS_INVOICE"
        case .networkMismatch: "NETWORK_MISMATCH"
        case .federationClosed: "FEDERATION_CLOSED"
        case .timeout: "TIMEOUT"
        case .entropy: "ENTROPY"
        case .storage: "STORAGE"
        case .internal: "INTERNAL"
        }
    }

    static func kind(_ kind: OperationKind) -> String {
        switch kind {
        case .ecashSend: "ECASH_SEND"
        case .ecashReceive: "ECASH_RECEIVE"
        case .lnSend: "LN_SEND"
        case .lnReceive: "LN_RECEIVE"
        case .onchainSend: "ONCHAIN_SEND"
        case .onchainReceive: "ONCHAIN_RECEIVE"
        case .recovery: "RECOVERY"
        case .unknown: "UNKNOWN"
        }
    }

    static func support(_ support: OperationSupport) -> String {
        switch support {
        case .observable: "OBSERVABLE"
        case .unknownKind: "UNKNOWN_KIND"
        case .stateSchemaTooNew: "STATE_SCHEMA_TOO_NEW"
        }
    }

    static func network(_ network: Network) -> String {
        switch network {
        case .bitcoin: "BITCOIN"
        case .testnet: "TESTNET"
        case .testnet4: "TESTNET4"
        case .signet: "SIGNET"
        case .regtest: "REGTEST"
        }
    }

    static func direction(_ direction: Direction) -> String {
        switch direction {
        case .incoming: "INCOMING"
        case .outgoing: "OUTGOING"
        }
    }

    static func status(_ status: ActivityStatus) -> String {
        switch status {
        case .pending: "PENDING"
        case .success: "SUCCESS"
        case .failed: "FAILED"
        case .refunded: "REFUNDED"
        case .canceled: "CANCELED"
        case .unknown: "UNKNOWN"
        }
    }

    static func route(_ route: LightningRoute) -> JSON {
        switch route {
        case .internal: ["type": "INTERNAL"]
        case .gateway(let gatewayId): ["type": "GATEWAY", "gatewayId": gatewayId]
        }
    }

    static func state(_ state: EcashSendState) -> JSON {
        switch state {
        case .created: ["type": "CREATED"]
        case .cancelRequested: ["type": "CANCEL_REQUESTED"]
        case .canceled: ["type": "CANCELED"]
        case .redeemed: ["type": "REDEEMED"]
        }
    }

    static func state(_ state: EcashReceiveState) -> JSON {
        switch state {
        case .created: ["type": "CREATED"]
        case .issuing: ["type": "ISSUING"]
        case .done: ["type": "DONE"]
        case .failed(let reason): ["type": "FAILED", "reason": reason]
        }
    }

    static func state(_ state: LnSendState) throws -> JSON {
        switch state {
        case .created: ["type": "CREATED"]
        case .funded: ["type": "FUNDED"]
        case .success(let preimage, let fee, let route):
            ["type": "SUCCESS", "preimage": preimage, "feeMsats": try num(fee), "route": Self.route(route)]
        case .refunded: ["type": "REFUNDED"]
        case .failed(let reason): ["type": "FAILED", "reason": reason]
        }
    }

    static func state(_ state: LnReceiveState) -> JSON {
        switch state {
        case .created: ["type": "CREATED"]
        case .waitingForPayment: ["type": "WAITING_FOR_PAYMENT"]
        case .funded: ["type": "FUNDED"]
        case .claimed: ["type": "CLAIMED"]
        case .canceled(let reason): ["type": "CANCELED", "reason": reason]
        case .expired: ["type": "EXPIRED"]
        case .failed: ["type": "FAILED"]
        }
    }

    static func state(_ state: OnchainSendState) -> JSON {
        switch state {
        case .created: ["type": "CREATED"]
        case .succeeded(let txid): ["type": "SUCCEEDED", "txid": txid]
        case .refunded(let reason): ["type": "REFUNDED", "reason": reason]
        case .failed(let reason): ["type": "FAILED", "reason": reason]
        }
    }

    static func state(_ state: OnchainReceiveState) throws -> JSON {
        switch state {
        case .confirmed(let txid, let gross):
            ["type": "CONFIRMED", "txid": txid, "grossDepositedSats": try num(gross)]
        case .claimed(let txid, let gross, let net):
            [
                "type": "CLAIMED", "txid": txid, "grossDepositedSats": try num(gross),
                "netCreditMsats": try num(net),
            ]
        case .failed(let reason): ["type": "FAILED", "reason": reason]
        }
    }

    static func state(_ state: RecoveryState) -> JSON {
        switch state {
        case .running(let progress):
            [
                "type": "RUNNING",
                "progress": orNull(progress.map { ["complete": num($0.complete), "total": num($0.total)] }),
            ]
        case .done: ["type": "DONE"]
        case .failed(let reason): ["type": "FAILED", "reason": reason]
        }
    }
}

// ── Params ─────────────────────────────────────────────────────────────────

/// A request's `params`, read with the protocol's type checks.
struct Params {
    let values: JSON

    private func present(_ key: String) -> Any? {
        guard let value = values[key], !(value is NSNull) else { return nil }
        return value
    }

    func str(_ key: String) throws -> String {
        guard values[key] != nil else { throw badParams("missing \"\(key)\"") }
        guard let value = values[key] as? String else { throw badParams("\"\(key)\" must be a string") }
        return value
    }

    func optStr(_ key: String) throws -> String? {
        present(key) == nil ? nil : try str(key)
    }

    /// A non-negative integer: an amount, a limit or a timeout.
    func uint(_ key: String) throws -> UInt64 {
        guard let value = values[key] else { throw badParams("missing \"\(key)\"") }
        guard let number = value as? NSNumber, CFGetTypeID(number) != CFBooleanGetTypeID(),
              number.doubleValue >= 0, number.doubleValue == number.doubleValue.rounded(),
              number.doubleValue <= Double(maxSafeInteger)
        else { throw badParams("\"\(key)\" must be a non-negative integer") }
        return number.uint64Value
    }

    func optUint(_ key: String) throws -> UInt64? {
        present(key) == nil ? nil : try uint(key)
    }

    func optStrList(_ key: String) throws -> [String]? {
        guard let value = present(key) else { return nil }
        guard let list = value as? [Any], let strings = list as? [String], strings.count == list.count else {
            throw badParams("\"\(key)\" must be an array of strings")
        }
        return strings
    }
}
