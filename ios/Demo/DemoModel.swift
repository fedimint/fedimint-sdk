import Foundation
import SwiftUI

import FedimintSdk

/// The mutinynet federation `js/examples/vite-core` and `android/app` pre-fill,
/// so all three demos start from the same place.
let testnetFederationCode =
    "fed11qgqrgvnhwden5te0v9k8q6rp9ekh2arfdeukuet595cr2ttpd3jhq6rzve6zuer9wchxvetyd938gcewvdhk6tcqq"
    + "ysptkuvknc7erjgf4em3zfh90kffqf9srujn6q53d6r056e4apze5cw27h75"

/// Names a result line, so each section can render its own outcome.
enum Section: String {
    case wallet, join, lnReceive, ecashReceive, ecashSend, lnPay, parseInvite
    case deposit, onchainSend, activity
}

/// Every SDK handle the screen holds, and one `run` helper that funnels each
/// action's outcome — including an `SdkError` — into one result line.
///
/// Threading is simpler than the Android demo's. There, every SDK call had to be
/// hopped onto `Dispatchers.IO` by hand and every view update hopped back. Here
/// each export is already `async throws`, so the whole type is `@MainActor` and
/// the awaits take care of themselves.
@MainActor
final class DemoModel: ObservableObject {

    // Rendered output, one entry per section.
    @Published var results: [Section: String] = [:]
    @Published var balance = "Balance: —"
    @Published var walletStatus = "No wallet open"
    @Published var seed: String?

    // Inputs.
    @Published var restoreWords = ""
    @Published var invite = testnetFederationCode
    @Published var lnReceiveAmount = "5000"
    @Published var lnReceiveDescription = "Coffee"
    @Published var ecashNotes = ""
    @Published var ecashSendAmount = "1000"
    /// Editing the invoice invalidates any quote taken against the previous one.
    /// Without this, Pay stays enabled (it is gated on `lnQuote != nil`) and
    /// would send the invoice the user has just replaced. The `!= nil` guard
    /// keeps a keystroke from publishing a change when there is no quote to
    /// invalidate.
    @Published var lnInvoice = "" {
        didSet { if lnQuote != nil { lnQuote = nil } }
    }
    @Published var parseInviteText = testnetFederationCode
    /// Same invalidation as `lnInvoice`, for the same reason: Send is gated on
    /// `onchainQuote != nil`, and a quote carries the destination and amount it
    /// was taken for.
    @Published var onchainAmount = "10000" {
        didSet { if onchainQuote != nil { onchainQuote = nil } }
    }
    @Published var onchainAddress = "" {
        didSet { if onchainQuote != nil { onchainQuote = nil } }
    }

    // Copyable results of the last run of each flow.
    @Published var lastInvoice: String?
    @Published var lastNotes: Notes?
    @Published var lastAddress: String?

    @Published private(set) var hasSdk = false
    @Published private(set) var hasFederation = false

    private var sdk: Sdk?
    private var federation: Federation?
    private var balanceTask: Task<Void, Never>?

    /// One long-lived operation watcher per section, so a new one can replace
    /// the old instead of racing it. Same ownership `balanceTask` has, and for
    /// the same reason: the Kotlin demo gets this free from `lifecycleScope`
    /// (MainActivity.kt:506), and Swift has no equivalent ambient scope.
    private var watchTasks: [Section: Task<Void, Never>] = [:]

    /// Quotes are single use: held between the Quote and Pay/Send taps so the
    /// user sees the fee before committing, then dropped once submitted. The
    /// Rust side enforces this too — a second `send` on the same quote throws
    /// `.quoteExpired` — so clearing them here is courtesy, not the guarantee.
    @Published private(set) var lnQuote: LnQuote?
    @Published private(set) var onchainQuote: OnchainQuote?

    // MARK: - Wallet

    func openWallet() {
        run(.wallet) {
            let words = self.restoreWords
                .split(whereSeparator: \.isWhitespace)
                .map(String.init)

            // `nil` loads the seed the directory already holds, or generates one
            // over an empty directory. A non-empty word list restores instead.
            let mnemonic = words.isEmpty ? nil : try Mnemonic.fromWords(words: words)
            let dataDir = try Self.dataDirectory()
            let sdk = try await createFedimintSdk(dataDir: dataDir.path, mnemonic: mnemonic)
            self.sdk = sdk
            self.hasSdk = true
            self.walletStatus = "Wallet open at \(dataDir.lastPathComponent)"

            // Reattach to whatever this storage already joined, so a second
            // launch comes back with a federation rather than a blank screen.
            if let existing = sdk.federations().first {
                self.attach(existing)
            }
            return words.isEmpty ? "wallet opened" : "wallet restored from \(words.count) words"
        }
    }

    /// `exportMnemonic()` hands back an opaque `Mnemonic`; `words()` is the
    /// deliberate step that takes the phrase out as plain strings.
    func toggleSeed() {
        guard seed == nil else {
            seed = nil
            return
        }
        guard let sdk else { return }
        seed = sdk.exportMnemonic().words()
            .enumerated()
            .map { "\($0.offset + 1). \($0.element)" }
            .joined(separator: "\n")
    }

    func refreshBalance() {
        run(.wallet) {
            guard let federation = self.federation else { return "join a federation first" }
            self.balance = "Balance: " + formatMsats(try await federation.balance())
            return "balance refreshed"
        }
    }

    // MARK: - Join

    func preview() {
        run(.join) {
            guard let sdk = self.sdk else { return "open the wallet first" }
            let preview = try await sdk.preview(invite: InviteCode.parse(code: self.trimmedInvite))
            var lines = [
                "Federation ID  \(preview.id)",
                "Name           \(preview.name ?? "(unnamed)")",
                "Network        \(preview.network)",
                "Guardians      \(preview.guardians)",
                "Modules        \(preview.modules.joined(separator: ", "))",
            ]
            lines += preview.meta.map { "meta.\($0.key) = \($0.value)" }
            return lines.joined(separator: "\n")
        }
    }

    /// Joins the pasted federation, or reattaches if this storage already joined
    /// it: `.alreadyJoined` is the cue to look the running federation up by the
    /// id the invite code carries.
    func join() {
        run(.join) {
            guard let sdk = self.sdk else { return "open the wallet first" }
            let code = try InviteCode.parse(code: self.trimmedInvite)

            let joined: Federation
            let verb: String
            do {
                joined = try await sdk.join(invite: code)
                verb = "Joined"
            } catch let error as SdkError where error.code() == .alreadyJoined {
                guard let existing = try sdk.federation(id: code.federationId()) else { throw error }
                joined = existing
                verb = "Already joined — reattached to"
            }

            self.attach(joined)
            return "\(verb) \(joined.id())\n\(joined.name() ?? "(unnamed)") on \(joined.network())"
        }
    }

    /// Joins with a seed-recovery rescan instead of a plain join: the path for a
    /// restored seed that already holds funds here. Spends and receives are
    /// refused until the rescan completes.
    func recover() {
        run(.join) {
            guard let sdk = self.sdk else { return "open the wallet first" }
            let recovery = try await sdk.recover(invite: InviteCode.parse(code: self.trimmedInvite))
            self.attach(recovery.federation)

            let header = "Recovering \(recovery.federation.id())"
            let progress = recovery.progress.updates()
            self.watch(.join, header: header) { try await progress.next() }
            let state = try await recovery.progress.state()
            return "\(header)\n\nstate: \(state)"
        }
    }

    // MARK: - Lightning in

    func lnReceive() {
        run(.lnReceive) {
            guard let lightning = self.federation?.lightning() else {
                return "this federation has no lightning module"
            }
            guard let msats = UInt64(self.lnReceiveAmount.trimmed) else {
                return "enter an amount in msats"
            }

            let receive = try await lightning.receive(
                amount: msats,
                description: self.lnReceiveDescription
            )
            self.lastInvoice = receive.invoice

            let header = "Invoice:\n\(receive.invoice)\n\noperation \(receive.operation.id())"
            let updates = receive.operation.updates()
            self.watch(.lnReceive, header: header) { try await updates.next() }
            return header
        }
    }

    // MARK: - Ecash

    func ecashReceive() {
        run(.ecashReceive) {
            guard let ecash = self.federation?.ecash() else {
                return "this federation has no mint module"
            }
            guard !self.ecashNotes.trimmed.isEmpty else { return "paste ecash notes first" }

            let notes = try Notes.parse(notes: self.ecashNotes.trimmed)
            let operation = try await ecash.receive(notes: notes)

            // `awaitFinal` returns the terminal state, and `.failed` is one of
            // them: an operation failing is a state, while a thrown error means
            // the operation could not be observed. Only `.done` redeemed
            // anything.
            let outcome: String
            switch try await operation.awaitFinal() {
            case .done:
                outcome = "Redeemed \(formatMsats(notes.value()))"
            case .failed(let reason):
                outcome = "Not redeemed: \(reason)"
            case let state:
                outcome = "Ended in \(state)"
            }
            return "\(outcome)\n\noperation \(operation.id())"
        }
    }

    /// Ecash out: quote the amount, then send it. The notes are a bearer
    /// instrument — hand them to a receiver out of band — and the operation
    /// keeps tracking whether they were redeemed or reclaimed.
    func ecashSend() {
        run(.ecashSend) {
            guard let ecash = self.federation?.ecash() else {
                return "this federation has no mint module"
            }
            guard let msats = UInt64(self.ecashSendAmount.trimmed) else {
                return "enter an amount in msats"
            }

            let quote = try await ecash.quote(amount: msats)
            let sent = try await ecash.send(quote: quote)
            self.lastNotes = sent.notes

            let state = try await sent.operation.state()
            return """
                notes \(formatMsats(quote.notesValue())) + fee \(formatMsats(quote.fee()))

                hand these notes to the receiver:
                \(sent.notes.display())

                operation \(sent.operation.id()) — \(state)
                """
        }
    }

    // MARK: - Lightning out

    /// Quotes a pasted invoice: amount, fee, route and expiry, before anything
    /// is paid.
    func lnQuoteInvoice() {
        // Disarmed synchronously, before the call: if this quote fails, the
        // previous one must not stay live behind the error message.
        lnQuote = nil
        run(.lnPay) {
            guard let lightning = self.federation?.lightning() else {
                return "this federation has no lightning module"
            }
            guard !self.lnInvoice.trimmed.isEmpty else { return "paste an invoice first" }

            let quote = try await lightning.quote(invoice: self.lnInvoice.trimmed)
            self.lnQuote = quote
            return """
                Amount   \(formatMsats(quote.invoiceAmount()))
                Fee      \(formatMsats(quote.fee()))
                Total    \(formatMsats(quote.total()))
                Route    \(quote.route())
                Expires  \(formatTimestamp(quote.expiresAt()))
                """
        }
    }

    func lnPay() {
        guard let quote = lnQuote, let lightning = federation?.lightning() else { return }
        lnQuote = nil
        run(.lnPay) {
            let operation = try await lightning.send(quote: quote)
            let state = try await operation.awaitFinal()
            return "operation \(operation.id())\nstate: \(state)"
        }
    }

    // MARK: - Parse invite code

    func parseInvite() {
        run(.parseInvite) {
            let code = try InviteCode.parse(code: self.parseInviteText.trimmed)
            return "Fed Id: \(code.federationId())"
        }
    }

    // MARK: - On-chain

    func deposit() {
        run(.deposit) {
            guard let onchain = self.federation?.onchain() else {
                return "this federation has no wallet module"
            }

            let receive = try await onchain.receive()
            self.lastAddress = receive.address

            let header = "Send bitcoin to:\n\(receive.address)\n\noperation \(receive.operation.id())"
            let updates = receive.operation.updates()
            self.watch(.deposit, header: header) { try await updates.next() }
            return header
        }
    }

    func onchainQuoteAddress() {
        // Same as `lnQuoteInvoice`: a failed re-quote must not leave the
        // previous destination armed.
        onchainQuote = nil
        run(.onchainSend) {
            guard let onchain = self.federation?.onchain() else {
                return "this federation has no wallet module"
            }
            // Note the unit: `Onchain.quote` is the one call that takes *sats*.
            // Every other amount in this SDK is msats, and both are UInt64, so
            // the compiler will not catch a mix-up.
            guard let sats = UInt64(self.onchainAmount.trimmed) else {
                return "enter an amount in sats"
            }
            guard !self.onchainAddress.trimmed.isEmpty else {
                return "enter a destination address"
            }

            let quote = try await onchain.quote(
                address: self.onchainAddress.trimmed,
                amount: sats
            )
            self.onchainQuote = quote
            return """
                Amount   \(quote.amount()) sat
                Fee      \(formatMsats(quote.fee()))
                Total    \(formatMsats(quote.total()))
                Expires  \(formatTimestamp(quote.expiresAt()))
                """
        }
    }

    func onchainSend() {
        guard let quote = onchainQuote, let onchain = federation?.onchain() else { return }
        onchainQuote = nil
        run(.onchainSend) {
            let operation = try await onchain.send(quote: quote)
            let header = "operation \(operation.id())"
            let updates = operation.updates()
            self.watch(.onchainSend, header: header) { try await updates.next() }
            let state = try await operation.state()
            return "\(header)\nstate: \(state)"
        }
    }

    // MARK: - Activity

    func activity() {
        run(.activity) {
            guard let federation = self.federation else { return "join a federation first" }
            let page = try await federation.activity(cursor: nil, limit: 20)
            if page.items.isEmpty { return "no activity yet" }
            return page.items.map { item in
                var line = "\(formatTimestamp(item.time))\n\(item.kind)"
                if let direction = item.direction { line += " \(direction)" }
                line += " \(item.status)"
                if let amount = item.amount { line += "  \(formatMsats(amount))" }
                if let fee = item.fee { line += "  fee \(formatMsats(fee))" }
                return line
            }
            .joined(separator: "\n\n")
        }
    }

    // MARK: - Helpers

    private var trimmedInvite: String { invite.trimmed }

    /// An app-private directory. `.applicationSupportDirectory` rather than
    /// `.documentDirectory`: the store is the app's own state, not a user
    /// document, and should not show up in Files.
    private static func dataDirectory() throws -> URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        let dir = base.appendingPathComponent("fedimint", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    /// Makes `joined` the current federation and keeps the balance line live
    /// from its `balanceUpdates()` subscription.
    private func attach(_ joined: Federation) {
        federation = joined
        hasFederation = true
        balanceTask?.cancel()
        // Every open watcher belongs to the federation being replaced, so its
        // cursor is stale by definition.
        cancelWatchers()
        lnQuote = nil
        onchainQuote = nil
        balance = "Balance: …"

        balanceTask = Task { [weak self] in
            do {
                let initial = try await joined.balance()
                // `if let`, not `guard let`: a `guard let self` here binds for
                // the rest of the `do` block — the loop included — which pins
                // the model for the life of an endless subscription and makes
                // the `[weak self]` above do nothing.
                if let self { self.balance = "Balance: " + formatMsats(initial) }

                let updates = joined.balanceUpdates()
                while !Task.isCancelled {
                    let next = try await updates.next()
                    // Scoped to one iteration, so nothing strong is held across
                    // the await above. `break` rather than skipping: with the
                    // model gone there is nobody left to render into, and
                    // `next()` is a uniffi call that cancellation cannot stop.
                    guard let self else { break }
                    self.balance = "Balance: " + formatMsats(next)
                }
            } catch let error as SdkError {
                self?.balance = "Balance: unavailable (\(error.code()))"
            } catch {
                self?.balance = "Balance: unavailable"
            }
        }
    }

    /// Runs one SDK action and renders what comes back into its result line,
    /// including an `SdkError` — which is what a real application would branch
    /// on. This is the Swift counterpart of the Android demo's `section`.
    private func run(_ section: Section, _ block: @MainActor @escaping () async throws -> String) {
        results[section] = "working…"
        Task {
            do {
                self.results[section] = try await block()
            } catch let error as SdkError {
                self.results[section] = self.describe(error)
            } catch {
                self.results[section] = "\(error)"
            }
        }
    }

    /// Follows an operation's state changes into `section` under `header`, until
    /// the subscription ends — `next()` returns `nil` once the state is final.
    ///
    /// Takes a closure that pulls the cursor rather than the cursor itself, and
    /// that is not an accident. `Operation<S>` is generic in Rust, but a UniFFI
    /// object cannot be, so the crate monomorphises it: there are seven
    /// unrelated `*OperationUpdates` classes with no common protocol between
    /// them. A closure is the one thing that unifies them without inventing a
    /// Swift-only protocol layer the Kotlin SDK does not have. See
    /// `ios/README.md`, "What's not here yet".
    private func watch<State>(
        _ section: Section,
        header: String,
        next: @MainActor @escaping () async throws -> State?
    ) {
        watchTasks[section]?.cancel()
        // `[weak self]` for the same reason `balanceTask` uses it: the model
        // owns the task and the task would otherwise own the model, and a
        // watcher can stay open for as long as an on-chain confirmation takes.
        // It also makes a write from a watcher that outlives the screen a
        // no-op rather than a resurrection.
        watchTasks[section] = Task { [weak self] in
            do {
                while let state = try await next() {
                    // Checked before the write rather than only at loop entry.
                    // `next()` is a uniffi call and is not cancellable, so a
                    // cancelled watcher stays alive until its in-flight call
                    // returns; guarding the write is what actually stops a
                    // stale watcher from overwriting a section that a newer
                    // one now owns.
                    if Task.isCancelled { break }
                    // Also stop if the screen is gone: `self?.` alone would
                    // keep pulling a cursor nothing can render.
                    guard let self else { break }
                    self.results[section] = "\(header)\n\nstate: \(state)"
                }
                // A `nil` means the state is final, not that anything went
                // wrong; leave the last rendering in place and stop.
            } catch let error as SdkError {
                if !Task.isCancelled {
                    self?.results[section] = "\(header)\n\nstopped watching: \(error.reason())"
                }
            } catch {
                if !Task.isCancelled {
                    self?.results[section] = "\(header)\n\nstopped watching: \(error)"
                }
            }
        }
    }

    /// Stops every operation watcher.
    ///
    /// Finished tasks are left in the dictionary rather than removing
    /// themselves on completion: self-removal needs a token to avoid a task
    /// that finished late clearing the entry belonging to its own replacement,
    /// and cancelling an already-finished `Task` is a no-op.
    private func cancelWatchers() {
        watchTasks.values.forEach { $0.cancel() }
        watchTasks.removeAll()
    }

    private func describe(_ error: SdkError) -> String {
        switch error.code() {
        case .invalidInput:
            return "That input isn't valid.\n\n\(error.reason())"
        case .alreadyJoined:
            return "You're already in this federation."
        case .federationUnreachable, .timeout:
            return "Couldn't reach the federation.\n\n\(error.reason())"
        case .unsupportedFederation:
            return "This app can't work with that federation."
        case .insufficientBalance:
            return "Not enough balance.\n\n\(error.reason())"
        case .quoteExpired:
            return "That quote is no longer usable — quote again.\n\n\(error.reason())"
        case .recovering:
            return "Still recovering — try again when the rescan finishes."
        default:
            return "\(error.code()): \(error.reason())"
        }
    }
}

// MARK: - Formatting

/// Exact for every `UInt64`.
///
/// Integer division rather than `Double(msats) / 1000`: the binding preserves
/// the full `u64`, and going through `Double` would quietly round anything
/// above 2^53 msats. Same output as the Kotlin demo's `formatMsats`
/// (MainActivity.kt:553), so both render an amount identically.
func formatMsats(_ msats: UInt64) -> String {
    let sats = msats / 1_000
    return msats % 1_000 == 0 ? "\(sats) sat" : "\(sats) sat (\(msats) msat)"
}

/// `Double` is fine here, unlike in `formatMsats`: epoch milliseconds are
/// ~1.7e12, four orders of magnitude below 2^53, and `Date` takes a
/// `TimeInterval` anyway.
func formatTimestamp(_ epochMillis: UInt64) -> String {
    let date = Date(timeIntervalSince1970: Double(epochMillis) / 1000)
    return DateFormatter.localizedString(from: date, dateStyle: .short, timeStyle: .medium)
}

extension String {
    var trimmed: String { trimmingCharacters(in: .whitespacesAndNewlines) }
}
