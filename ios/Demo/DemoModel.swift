import Foundation
import LocalAuthentication
import SwiftUI

import FedimintSdk

/// The mutinynet federation `android/app` also pre-fills, so both demos start
/// from the same place.
let testnetFederationCode =
    "fed11qgqrgvnhwden5te0v9k8q6rp9ekh2arfdeukuet595cr2ttpd3jhq6rzve6zuer9wchxvetyd938gcewvdhk6tcqq"
    + "ysptkuvknc7erjgf4em3zfh90kffqf9srujn6q53d6r056e4apze5cw27h75"

/// Names a result line, so each section can render its own outcome.
enum Section: String {
    case wallet, join, lnReceive, ecashReceive, ecashSend, lnPay
    case deposit, onchainSend, activity
    /// Reclaiming the last ecash send, kept apart from `ecashSend` so asking
    /// for it never overwrites the send's own line.
    case ecashReclaim
    /// Following, or resuming, the attached federation's seed recovery.
    case recovery
}

extension Section {
    /// Sections whose contents belong to one federation, and are therefore
    /// meaningless the moment another one is attached.
    ///
    /// `wallet` is SDK-scoped, so it survives a switch; `join` is written by
    /// the caller of `attach` immediately afterwards.
    static let federationScoped: Set<Section> = [
        .lnReceive, .ecashReceive, .ecashSend, .lnPay, .deposit, .onchainSend, .activity,
        .ecashReclaim, .recovery,
    ]
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
    /// Why each section's last action failed, if it did: the same text `run`
    /// renders into `results`, kept apart so a screen can tell a failure from
    /// an action's progress without reading its line.
    @Published private(set) var failures: [Section: String] = [:]
    /// Why following a section's operation stopped before its state was final:
    /// the subscription failed, not the call that started the operation. The
    /// operation may still have completed; this says only that its state can
    /// no longer be read.
    @Published private(set) var watchFailures: [Section: String] = [:]
    @Published var seed: String?

    /// The same phrase as `seed`, as words, for a screen that lays them out and
    /// checks them itself. Set and cleared with `seed`, under the same
    /// authentication.
    @Published private(set) var seedWords: [String] = []

    // What the screens that lay values out themselves need, rather than the
    // sentences in `results`. All of it belongs to the attached federation and
    // is reset by `attach`.

    /// `nil` while there is none to show: no federation, or one whose first
    /// `balance()` has not landed.
    @Published private(set) var balanceMsats: UInt64?
    /// Why the balance subscription stopped, which leaves `balanceMsats` where
    /// it was.
    @Published private(set) var balanceFailure: String?
    @Published private(set) var federationName: String?
    @Published private(set) var federationId: FederationId?
    /// The attached federation's invite code, for inviting someone else into
    /// it. A credential that admits whoever holds it, so it is shown only on
    /// request.
    @Published private(set) var federationInvite: String?
    @Published private(set) var federationNetwork: Network?
    /// Which rails the federation has. Screens show only these: the design's
    /// rule is to hide a rail the federation cannot use, never to disable it.
    @Published private(set) var capabilities: Capabilities?
    /// As `Sdk.federationStatus` reported it when the federation was attached.
    @Published private(set) var federationStatus: FederationStatus?
    /// The attached federation's seed recovery, while one is being followed.
    @Published private(set) var recoveryState: RecoveryState?
    @Published private(set) var activityItems: [ActivityItem] = []
    /// Where the next page of history starts, or `nil` at its end.
    @Published private(set) var activityNext: Cursor?

    // Inputs.
    @Published var restoreWords = ""
    /// Editing the code drops the preview taken of the previous one, so a
    /// preview on screen always describes the code in the field.
    @Published var invite = testnetFederationCode {
        didSet { if invite != oldValue, invitePreview != nil { invitePreview = nil } }
    }
    /// The last preview of `invite`: what joining it would mean, before it
    /// is joined.
    @Published private(set) var invitePreview: FederationPreview?
    @Published var lnReceiveAmount = "5000"
    @Published var lnReceiveDescription = "Coffee"
    @Published var ecashNotes = ""
    /// Same invalidation as `lnInvoice`: a quote is for the amount it was
    /// taken for.
    @Published var ecashSendAmount = "1000" {
        didSet { if ecashSendAmount != oldValue, ecashQuote != nil { ecashQuote = nil } }
    }
    /// Editing the invoice invalidates any quote taken against the previous one.
    /// Without this, Pay stays enabled (it is gated on `lnQuote != nil`) and
    /// would send the invoice the user has just replaced. The `!= nil` guard
    /// keeps a keystroke from publishing a change when there is no quote to
    /// invalidate. The `!= oldValue` one is because a text field losing focus,
    /// as it does when Review is pushed, writes its unchanged text back.
    @Published var lnInvoice = "" {
        didSet { if lnInvoice != oldValue, lnQuote != nil { lnQuote = nil } }
    }
    /// Same invalidation as `lnInvoice`, for the same reason: Send is gated on
    /// `onchainQuote != nil`, and a quote carries the destination and amount it
    /// was taken for.
    @Published var onchainAmount = "10000" {
        didSet { if onchainAmount != oldValue, onchainQuote != nil { onchainQuote = nil } }
    }
    @Published var onchainAddress = "" {
        didSet { if onchainAddress != oldValue, onchainQuote != nil { onchainQuote = nil } }
    }

    // Copyable results of the last run of each flow.
    @Published var lastInvoice: String?
    @Published var lastNotes: Notes?
    @Published var lastAddress: String?

    // The live state of the last receive, for screens that render it as a
    // status rather than as the section's text.
    @Published private(set) var lnReceiveState: LnReceiveState?
    @Published private(set) var lnReceiveExpiresAt: Timestamp?
    /// What `lastInvoice` actually asks for, as the SDK reports it. A screen
    /// shows this next to the invoice, never the amount field it was typed
    /// into, which may have been edited since.
    @Published private(set) var lnReceiveInvoiceMsats: UInt64?
    /// Whether the wallet has found a payment to `lastAddress` yet. Until it
    /// has, a deposit is not an operation and has no state.
    @Published private(set) var depositFound = false
    @Published private(set) var depositState: OnchainReceiveState?

    /// True while an ecash send is committing.
    ///
    /// Ecash send is the only value-moving action here without a natural gate:
    /// `lnPay` and `onchainSend` both require a quote that is cleared before the
    /// run starts, so a second tap does nothing, while this one quotes and sends
    /// in a single block. Two taps would be two real spends.
    @Published private(set) var isSendingEcash = false

    /// Counts attaches, so a screen about the attached federation can tell it
    /// was replaced, by another one or by a new handle under the same id.
    @Published private(set) var attachments = 0

    /// Whether a send on any rail is starting. Joining or reopening waits for
    /// it: `attach` stops the federation's runs, which would drop the
    /// operation a send has just submitted along with its result.
    var isSending: Bool { isSendingEcash || isPayingLightning || isSendingOnchain }
    /// Set around the payment itself, not the section: quoting runs in the
    /// same sections, and a quote spends nothing.
    @Published private(set) var isPayingLightning = false
    @Published private(set) var isSendingOnchain = false

    /// Open Wallet, Join and Recover runs in flight — everything that may end in
    /// `attach`.
    ///
    /// The other half of `isSendingEcash`. That flag stops an attach from
    /// starting during a send; this one stops a send from starting while an
    /// attach is still pending. `attach` clears `lastNotes` and
    /// `results[.ecashSend]`, so a send that finished first would lose its notes
    /// and operation id the moment a join started before it landed.
    ///
    /// A count rather than a flag: a superseded Join keeps running until its
    /// uniffi call returns (see `isSuperseded`), so several of these blocks can
    /// be alive at once, and the first to finish must not clear the gate for
    /// the rest.
    @Published private var attachesInFlight = 0
    var isAttaching: Bool { attachesInFlight > 0 }

    @Published private(set) var hasSdk = false
    @Published private(set) var hasFederation = false

    /// How the open wallet came about this launch, so onboarding shows the
    /// backup step right after a wallet is created, and only then.
    enum WalletOrigin { case created, restored, reopened }
    @Published private(set) var walletOrigin: WalletOrigin?

    /// Whether Join recovers rather than joins plainly. It does unless the seed
    /// is known to have been created on this device: any other seed may already
    /// hold funds in the federation, which a plain join would never look for,
    /// and the SDK will not recover a federation joined plainly. Recovering a
    /// seed that holds nothing only costs the rescan, so an unknown origin,
    /// such as a wallet from before `createdMarkerURL` existed, recovers too.
    @Published private(set) var recoversOnJoin = true

    private var sdk: Sdk?
    private var federation: Federation?
    private var balanceTask: Task<Void, Never>?

    /// One long-lived operation watcher per section, so a new one can replace
    /// the old instead of racing it. Same ownership `balanceTask` has, and for
    /// the same reason: the Kotlin demo gets this free from `lifecycleScope`
    /// (`watch` in MainActivity.kt), and Swift has no equivalent ambient scope.
    private var watchTasks: [Section: Task<Void, Never>] = [:]

    /// One in-flight SDK action per section. See `run` for why this is separate
    /// from `watchTasks`.
    private var runTasks: [Section: Task<Void, Never>] = [:]

    /// True once the action currently running has been superseded — by a newer
    /// tap on the same section, or by `attach` replacing the federation it was
    /// started against.
    ///
    /// **Every mutation that follows an `await` inside a `run` block has to
    /// consult this**, and that is not the same as `run` checking it once at the
    /// end. Cancelling a task does not stop the block: uniffi calls cannot be
    /// interrupted, so the block runs to completion and each of its writes lands
    /// unless that write is guarded. `run`'s own check covers the result text
    /// and nothing else.
    ///
    /// It reads `Task.isCancelled` because a block is awaited *by* its run task,
    /// so the current task inside the block is that task.
    private var isSuperseded: Bool { Task.isCancelled }

    /// Quotes are single use: held between the Quote and Pay/Send taps so the
    /// user sees the fee before committing, then dropped once submitted. The
    /// Rust side enforces this too — a second `send` on the same quote throws
    /// `.quoteExpired` — so clearing them here is courtesy, not the guarantee.
    @Published private(set) var lnQuote: LnQuote?
    @Published private(set) var onchainQuote: OnchainQuote?
    @Published private(set) var ecashQuote: EcashQuote?

    // The live state of the last send on each rail, for screens that render
    // it as a status. `nil` until a send has been submitted.
    @Published private(set) var lnSendState: LnSendState?
    @Published private(set) var onchainSendState: OnchainSendState?
    @Published private(set) var ecashSendState: EcashSendState?
    /// The last ecash send, kept so it can be reclaimed while unredeemed.
    private var lastEcashSend: EcashSendOperation?

    // MARK: - Wallet

    func openWallet() {
        guard !isSending else { return }
        attachesInFlight += 1
        run(.wallet) {
            defer { self.attachesInFlight -= 1 }

            let words = self.restoreWords
                .split(whereSeparator: \.isWhitespace)
                .map(String.init)

            // `nil` loads the seed the directory already holds, or generates one
            // over an empty directory. A non-empty word list restores instead.
            let mnemonic = words.isEmpty ? nil : try Mnemonic.fromWords(words: words)
            // Read before the open, which is what fills an empty directory.
            let origin: WalletOrigin =
                !words.isEmpty ? .restored : (Self.hasStoredWallet() ? .reopened : .created)
            let dataDir = try Self.dataDirectory()
            let sdk = try await createFedimintSdk(dataDir: dataDir.path, mnemonic: mnemonic)
            guard !self.isSuperseded else { return "superseded by a newer open" }
            self.sdk = sdk
            self.hasSdk = true
            self.walletOrigin = origin
            self.recoversOnJoin = Self.recoversOnJoin(after: origin)
            // The words are the seed. Once the SDK holds it there is no reason
            // for a copy to stay in an input field for the life of the screen.
            self.restoreWords = ""

            // Reattach to whatever this storage already joined, so a second
            // launch comes back with a federation rather than a blank screen.
            if let existing = sdk.federations().first {
                self.attach(existing)
                // A recovery interrupted by the last quit carries on in the
                // SDK; follow it, or Home stays locked with nothing moving.
                if case .recovering = self.federationStatus { self.followRecovery() }
            }
            return words.isEmpty ? "wallet opened" : "wallet restored from \(words.count) words"
        }
    }

    /// `exportMnemonic()` hands back an opaque `Mnemonic`; `words()` is the
    /// deliberate step that takes the phrase out as plain strings.
    ///
    /// Showing the phrase needs device-owner authentication first: Face ID or
    /// Touch ID, falling back to the passcode. Hiding it needs nothing. It fails
    /// closed: a device with no passcode cannot authenticate the owner, so it
    /// never shows the phrase.
    func toggleSeed() {
        guard seed == nil else {
            hideSeed()
            return
        }
        guard sdk != nil else { return }

        let generation = revealGeneration
        Task {
            let context = LAContext()
            var unavailable: NSError?
            guard context.canEvaluatePolicy(.deviceOwnerAuthentication, error: &unavailable) else {
                results[.wallet] = "set a device passcode to view the seed"
                return
            }
            do {
                try await context.evaluatePolicy(
                    .deviceOwnerAuthentication,
                    localizedReason: "Show the wallet's recovery phrase")
            } catch {
                results[.wallet] = "seed stays hidden: \(error.localizedDescription)"
                return
            }
            // Hidden while the prompt was up, such as the sheet showing it
            // being dismissed: the phrase must not appear behind that.
            guard generation == revealGeneration else { return }
            // Read the handle after the prompt, not before: an Open Wallet
            // that lands while it is up replaces the instance, and the phrase
            // shown has to be the open wallet's.
            guard let sdk = self.sdk else { return }
            let words = sdk.exportMnemonic().words()
            seedWords = words
            seed = words
                .enumerated()
                .map { "\($0.offset + 1). \($0.element)" }
                .joined(separator: "\n")
        }
    }

    /// Hides the phrase, and drops a reveal still waiting on authentication so
    /// it cannot show the phrase after whatever displayed it has gone. Every way
    /// off a screen that shows the phrase calls this, swiping a sheet away
    /// included.
    func hideSeed() {
        revealGeneration += 1
        seed = nil
        seedWords = []
    }
    private var revealGeneration = 0

    func refreshBalance() {
        run(.wallet) {
            guard let federation = self.federation else { return "join a federation first" }
            let amount = try await federation.balance()
            // `.wallet` is deliberately outside `federationScoped` — that set is
            // what `attach` both cancels and clears, and this section holds the
            // SDK-level status line — so `attach` never cancels this run and
            // `isSuperseded` alone cannot see a federation switch. Compare the
            // handle instead, or a Refresh started on the previous federation
            // lands its balance under the new one.
            guard !self.isSuperseded, self.federation === federation else {
                return "federation changed — refresh again"
            }
            self.balanceMsats = amount
            // The live updates stopped earlier: start them again, which is
            // what clears the warning that the amount may be out of date.
            if self.balanceFailure != nil { self.followBalance(of: federation) }
            return "balance refreshed"
        }
    }

    // MARK: - Join

    func preview() {
        guard !isSendingEcash else { return }
        run(.join) {
            guard let sdk = self.sdk else { return "open the wallet first" }
            let invite = self.trimmedInvite
            let preview = try await sdk.preview(invite: InviteCode.parse(code: invite))
            // The code may have been edited while this was in flight.
            guard !self.isSuperseded, invite == self.trimmedInvite else { return "superseded by a newer preview" }
            self.invitePreview = preview
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
        guard !isSending else { return }
        attachesInFlight += 1
        run(.join) {
            defer { self.attachesInFlight -= 1 }

            guard let sdk = self.sdk else { return "open the wallet first" }
            let code = try InviteCode.parse(code: self.trimmedInvite)

            let joined: Federation
            let verb: String
            do {
                joined = try await sdk.join(invite: code)
                verb = "Joined"
            } catch let error as SdkError where error.code() == .alreadyJoined {
                // `reopenFederation` returns the live handle of an open
                // federation, and brings back a quarantined or closed one.
                joined = try await sdk.reopenFederation(id: code.federationId())
                verb = "Already joined — reattached to"
            }

            guard !self.isSuperseded else { return "superseded by a newer join" }
            self.attach(joined)
            return "\(verb) \(joined.id())\n\(joined.name() ?? "(unnamed)") on \(joined.network())"
        }
    }

    /// Joins with a seed-recovery rescan instead of a plain join: the path for a
    /// restored seed that already holds funds here. Spends and receives are
    /// refused until the rescan completes.
    func recover() {
        guard !isSending else { return }
        attachesInFlight += 1
        run(.join) {
            defer { self.attachesInFlight -= 1 }

            guard let sdk = self.sdk else { return "open the wallet first" }
            let code = try InviteCode.parse(code: self.trimmedInvite)
            let recovery: RecoveryHandle
            do {
                recovery = try await sdk.recover(invite: code)
            } catch let error as SdkError where error.code() == .alreadyJoined {
                // Joined already, such as one recovered earlier that another
                // federation has since replaced on screen: switch back to it, as
                // `join` does, and follow whatever recovery it has.
                let existing = try await sdk.reopenFederation(id: code.federationId())
                guard !self.isSuperseded else { return "superseded by a newer join" }
                self.attach(existing)
                self.followRecovery()
                return "Already joined — reattached to \(existing.id())"
            }
            guard !self.isSuperseded else { return "superseded by a newer join" }
            self.attach(recovery.federation)

            // The watcher's first update is the current state, so no separate
            // read: one landing after a newer update would set it back.
            let header = "Recovering \(recovery.federation.id())"
            let progress = recovery.progress.updates()
            self.watch(.recovery, header: header, onState: { [weak self] in self?.recoveryProgressed($0) }) {
                try await progress.next()
            }
            return header
        }
    }

    // MARK: - Lightning in

    func lnReceive() {
        // Before anything is awaited: the previous invoice's watcher would
        // otherwise go on writing into the state this invoice is about to show.
        // The previous invoice can still be paid; it is only no longer followed
        // here.
        stopWatching(.lnReceive)
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
            guard !self.isSuperseded else { return "superseded by a newer request" }
            self.lastInvoice = receive.invoice
            self.lnReceiveState = nil
            self.lnReceiveInvoiceMsats = nil
            self.lnReceiveExpiresAt = nil

            // `try?`: the invoice is already usable without these, which only
            // describe it.
            let details = try? await receive.operation.details()
            guard !self.isSuperseded else { return "superseded by a newer request" }
            self.lnReceiveExpiresAt = details?.expiresAt
            self.lnReceiveInvoiceMsats = details?.invoiceAmount

            let header = "Invoice:\n\(receive.invoice)\n\noperation \(receive.operation.id())"
            let updates = receive.operation.updates()
            self.watch(.lnReceive, header: header, onState: { [weak self] in self?.lnReceiveState = $0 }) {
                try await updates.next()
            }
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

    /// Quotes an ecash send of `ecashSendAmount`, so the fee is on screen
    /// before anything is spent. `ecashSend` then executes it.
    func ecashQuoteAmount() {
        // Both share `.ecashSend`, and `run` cancels the section's previous
        // run: quoting during a send would cancel that send mid-commit.
        guard !isSendingEcash else { return }
        // Disarmed before the call, like the other quotes: a failed re-quote
        // must not leave the previous one live behind the error.
        ecashQuote = nil
        run(.ecashSend) {
            guard let ecash = self.federation?.ecash() else {
                return "this federation has no mint module"
            }
            guard let msats = UInt64(self.ecashSendAmount.trimmed) else {
                return "enter an amount in msats"
            }
            let quote = try await ecash.quote(amount: msats)
            // The same two races the lightning quote guards against: a newer
            // quote, and the amount edited while this one was in flight.
            guard !self.isSuperseded else { return "superseded by a newer quote" }
            guard msats == UInt64(self.ecashSendAmount.trimmed) else {
                return "the amount changed while quoting — quote again"
            }
            self.ecashQuote = quote
            return """
                Notes    \(formatMsats(quote.notesValue()))
                Fee      \(formatMsats(quote.fee()))
                Total    \(formatMsats(quote.total()))
                Expires  \(formatTimestamp(quote.expiresAt()))
                """
        }
    }

    /// Ecash out: sends the quote `ecashQuoteAmount` took, or quotes and sends
    /// in one go when there is none. The notes are a bearer instrument — hand
    /// them to a receiver out of band — and the operation keeps tracking
    /// whether they were redeemed or reclaimed.
    /// Whether the send started; Review moves on to its result only if it did.
    @discardableResult
    func ecashSend() -> Bool {
        // Refused here, not only by the disabled button: the button's state is a
        // frame behind, so a fast double tap would otherwise still start a
        // second spend and `run` would cancel the first one mid-commit. Same lag
        // for `isAttaching`: a join already in flight would clear this send's
        // notes when it lands, even if the send has finished by then.
        guard !isSendingEcash, !isAttaching else { return false }
        isSendingEcash = true
        // Single use, like the other quotes: dropped before the send so a
        // second tap has nothing to execute.
        let held = ecashQuote
        ecashQuote = nil
        stopWatching(.ecashSend)
        ecashSendState = nil
        lastEcashSend = nil
        // The previous send's notes would otherwise stand in for this one's
        // until it succeeds, and hide its failure. They are not lost: that
        // send's operation detail still offers them, and can reclaim it.
        lastNotes = nil

        run(.ecashSend) {
            defer { self.isSendingEcash = false }

            guard let ecash = self.federation?.ecash() else {
                return "this federation has no mint module"
            }
            guard let msats = UInt64(self.ecashSendAmount.trimmed) else {
                return "enter an amount in msats"
            }

            let quote: EcashQuote
            if let held {
                quote = held
            } else {
                quote = try await ecash.quote(amount: msats)
            }

            // Checked before anything commits. `ecash` was captured from the
            // federation attached when this started, so without this a switch
            // during the quote goes on to spend from the federation the user has
            // just left.
            guard !self.isSuperseded else { return "federation changed — quote again" }

            let sent = try await ecash.send(quote: quote)

            // Looked up here, before anything is published, and with `try?`.
            // The state is cosmetic; the notes and the operation id are not.
            // An earlier version published the notes first and then awaited
            // this, which left a window where `attach` cleared the notes of a
            // send that had already moved value — and where a throw here
            // discarded the whole summary along with them.
            let state = try? await sent.operation.state()

            // No `await` from here to the end of the block, so nothing can
            // interleave: the notes, the operation id and the rendered summary
            // all land together or not at all.
            //
            // None of it is guarded on `isSuperseded`, unlike every other
            // post-await write in this file. The send has committed, and this
            // is the only record of where that value went; dropping it because
            // the run was superseded loses funds outright. Showing it under a
            // federation the user has since switched away from is the lesser
            // evil.
            //
            // The token leaves only through `lastNotes` and "Copy notes". The
            // summary names the notes but never prints them, because it renders
            // as selectable text, and a long-press copy
            // would put bearer value on the pasteboard without the local-only,
            // expiring write that button makes. `attach` clears `lastNotes` and
            // this summary together, so leaving the token out of it loses
            // nothing.
            self.lastNotes = sent.notes
            self.lastEcashSend = sent.operation

            let summary = """
                notes \(formatMsats(quote.notesValue())) + fee \(formatMsats(quote.fee()))

                hand these notes to the receiver: tap Copy notes. The copy stays
                on this device and leaves the pasteboard after two minutes.

                operation \(sent.operation.id()) — \(state.map { "\($0)" } ?? "state unavailable")
                """

            // Written directly as well as returned: `run` drops the returned
            // string when the run was superseded, and this is the only place
            // the operation id is ever shown. On the happy path `run` then
            // assigns the identical string, so this costs nothing.
            self.results[.ecashSend] = summary
            let updates = sent.operation.updates()
            self.watch(.ecashSend, header: summary, onState: { [weak self] in self?.ecashSendState = $0 }) {
                try await updates.next()
            }
            return summary
        }
        return true
    }

    /// Asks to take the last ecash send back. A request, not a guarantee: if
    /// the receiver redeems first, the send ends `redeemed`, and
    /// `ecashSendState` says which way it went.
    func reclaimEcash() {
        guard let operation = lastEcashSend else { return }
        run(.ecashReclaim) {
            try await operation.requestCancel()
            return "reclaim requested"
        }
    }

    // MARK: - Lightning out

    /// Quotes a pasted invoice: amount, fee, route and expiry, before anything
    /// is paid.
    func lnQuoteInvoice() {
        // Shares `.lnPay` with the payment, and `run` cancels the section's
        // previous run: quoting mid-payment would drop the payment's result.
        guard !isPayingLightning else { return }
        // Disarmed synchronously, before the call: if this quote fails, the
        // previous one must not stay live behind the error message.
        lnQuote = nil
        run(.lnPay) {
            guard let lightning = self.federation?.lightning() else {
                return "this federation has no lightning module"
            }
            let invoice = self.lnInvoice.trimmed
            guard !invoice.isEmpty else { return "paste an invoice first" }

            let quote = try await lightning.quote(invoice: invoice)

            // Two taps on Quote with the *same* invoice race each other, and the
            // input check below cannot see that: the field never changed. Arming
            // Pay with the older quote while the screen shows the newer one's fee
            // is the worst version of this bug.
            guard !self.isSuperseded else { return "superseded by a newer quote" }

            // Quoting is a round trip, and the field is editable throughout it.
            // The `didSet` on `lnInvoice` clears any existing quote the moment
            // the user types, but it cannot stop a call already in flight from
            // landing afterwards — and arming Pay with a quote for an invoice
            // that is no longer on screen is how the wrong invoice gets paid.
            guard invoice == self.lnInvoice.trimmed else {
                return "the invoice changed while quoting — quote again"
            }
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

    @discardableResult
    func lnPay() -> Bool {
        // `isAttaching` as in `ecashSend`: the button's state lags a frame, and
        // an attach landing mid-send would cancel the run following it.
        guard !isAttaching, let quote = lnQuote, let lightning = federation?.lightning() else { return false }
        lnQuote = nil
        isPayingLightning = true
        // An invoice is paid once, so the next send starts from an empty field
        // rather than one a paste would add to.
        lnInvoice = ""
        stopWatching(.lnPay)
        lnSendState = nil
        run(.lnPay) {
            defer { self.isPayingLightning = false }
            let operation = try await lightning.send(quote: quote)
            guard !self.isSuperseded else { return "operation \(operation.id()) — superseded" }
            let header = "operation \(operation.id())"
            let updates = operation.updates()
            self.watch(.lnPay, header: header, onState: { [weak self] in self?.lnSendState = $0 }) {
                try await updates.next()
            }
            return header
        }
        return true
    }

    // MARK: - On-chain

    /// Shows a deposit address, then follows the first deposit the wallet finds.
    ///
    /// The subscription is opened before the address is requested. It yields
    /// only the deposits found after it was opened, so one opened later could
    /// miss a payment that is found right away.
    ///
    /// An address is not an operation: a deposit becomes one once the
    /// federation's wallet finds the payment. Until then there is nothing to
    /// follow, so the block ends with the address on screen and leaves the wait
    /// for the deposit to `watchNextDeposit`.
    func deposit() {
        // An earlier request's watcher renders into this section too, and it
        // may still be waiting for a payment to that request's address. It is
        // stopped here, or its next update would overwrite this request's
        // address.
        stopWatching(.deposit)

        run(.deposit) {
            guard let onchain = self.federation?.onchain() else {
                return "this federation has no wallet module"
            }

            let deposits = try await onchain.deposits()
            let address = try await onchain.receive()
            guard !self.isSuperseded else { return "superseded by a newer request" }
            self.lastAddress = address
            self.depositFound = false
            self.depositState = nil

            let header = "Send bitcoin to:\n\(address)"
            self.watchNextDeposit(deposits, header: header)
            return header
        }
    }

    func onchainQuoteAddress() {
        guard !isSendingOnchain else { return }
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
            let address = self.onchainAddress.trimmed
            guard !address.isEmpty else {
                return "enter a destination address"
            }

            let quote = try await onchain.quote(address: address, amount: sats)

            // Same supersession race as the lightning quote above.
            guard !self.isSuperseded else { return "superseded by a newer quote" }

            // Same in-flight race as the lightning quote above, over two fields
            // instead of one: a quote carries the destination *and* the amount
            // it was taken for, so either one moving invalidates it.
            guard address == self.onchainAddress.trimmed,
                  sats == UInt64(self.onchainAmount.trimmed)
            else {
                return "the destination changed while quoting — quote again"
            }
            self.onchainQuote = quote
            return """
                Amount   \(quote.amount()) sat
                Fee      \(formatMsats(quote.fee()))
                Total    \(formatMsats(quote.total()))
                Expires  \(formatTimestamp(quote.expiresAt()))
                """
        }
    }

    @discardableResult
    func onchainSend() -> Bool {
        // `isAttaching`: see `lnPay`.
        guard !isAttaching, let quote = onchainQuote, let onchain = federation?.onchain() else { return false }
        onchainQuote = nil
        isSendingOnchain = true
        stopWatching(.onchainSend)
        onchainSendState = nil
        run(.onchainSend) {
            defer { self.isSendingOnchain = false }
            let operation = try await onchain.send(quote: quote)
            guard !self.isSuperseded else { return "operation \(operation.id()) — superseded" }
            let header = "operation \(operation.id())"
            let updates = operation.updates()
            self.watch(.onchainSend, header: header, onState: { [weak self] in self?.onchainSendState = $0 }) {
                try await updates.next()
            }
            return header
        }
        return true
    }

    /// Follows the attached federation's recovery again: the attempt that is
    /// running, or a new one if the last attempt stopped. Because of the second
    /// case, the recovery screen opens with `followRecovery` and leaves this to
    /// the user's tap.
    func resumeRecovery() {
        guard let sdk, let id = federationId else { return }
        run(.recovery) {
            let handle = try await sdk.resumeRecovery(id: id)
            guard !self.isSuperseded else { return "superseded by a newer resume" }
            // As in `recover`: the watcher's first update is the current state.
            let header = "recovery \(handle.progress.id())"
            let updates = handle.progress.updates()
            self.watch(.recovery, header: header, onState: { [weak self] in self?.recoveryProgressed($0) }) {
                try await updates.next()
            }
            return header
        }
    }

    /// A recovery that completed also changed the federation's status, from
    /// `recovering`, so that is read again with it.
    /// Reads where the recovery stands without starting anything, and follows
    /// it only if it is running. A stopped attempt is left stopped: resuming
    /// it starts a new attempt, which is the user's call, not the screen's.
    func followRecovery() {
        guard let sdk, let id = federationId else { return }
        Task {
            let read: RecoveryState?
            do {
                read = try await sdk.recoveryStatus(id: id)
            } catch let error as SdkError {
                if self.federationId == id { self.fail(.recovery, "couldn't read the recovery: \(error.reason())") }
                return
            } catch {
                if self.federationId == id { self.fail(.recovery, "couldn't read the recovery: \(error)") }
                return
            }
            // Another federation attached while this was read: the answer, and
            // any resume it would lead to, are not about the one on screen.
            guard self.federationId == id, let state = read else { return }
            // Followed already: the watcher's updates are newer than this read,
            // and its pending `next()` cannot be stopped to make way for another.
            guard !self.followingRecovery else { return }
            self.recoveryProgressed(state)
            // Running: resuming reattaches to the attempt under way.
            if case .running = state { self.resumeRecovery() }
        }
    }

    /// Whether the recovery has a resume under way, or a watcher that is still
    /// live: installed, not failed, and not yet at a final state.
    private var followingRecovery: Bool {
        if isWorking(.recovery) { return true }
        guard watchTasks[.recovery] != nil, watchFailures[.recovery] == nil else { return false }
        switch recoveryState {
        case nil, .running: return true
        case .done, .failed: return false
        }
    }

    private func recoveryProgressed(_ state: RecoveryState) {
        recoveryState = state
        if case .done = state { refreshFederationStatus() }
    }

    /// The follower of one operation's detail, shared by every visit to it: its
    /// subscription cannot be interrupted, so reusing it is what stops each
    /// visit from leaving another one waiting. Dropped with every attach, since a
    /// reopened federation is a new handle even under the same id.
    func follower(for id: OperationId) -> OperationFollower {
        if let existing = followers[id] { return existing }
        let follower = OperationFollower()
        followers[id] = follower
        return follower
    }
    private var followers: [OperationId: OperationFollower] = [:]

    /// One operation of the attached federation, by id, for a screen that
    /// follows it. `nil` for an id this federation has no record of.
    func operation(id: OperationId) async throws -> AnyOperation? {
        try await federation?.operation(id: id)
    }

    /// Reads the attached federation's status again: it moves on its own, for
    /// example from `recovering` to `running` when a rescan completes.
    func refreshFederationStatus() {
        guard let sdk, let id = federationId else { return }
        federationStatus = try? sdk.federationStatus(id: id)
    }

    // MARK: - Activity

    /// How many rows one page of history asks for.
    private static let activityPageSize: UInt16 = 20

    func activity() {
        run(.activity) {
            guard let federation = self.federation else { return "join a federation first" }
            let page = try await federation.activity(cursor: nil, limit: Self.activityPageSize)
            // `.activity` is federation-scoped, so `attach` cancels this run;
            // rows from the federation being replaced must not land under its
            // successor.
            guard !self.isSuperseded else { return "superseded by a newer load" }
            self.activityItems = page.items
            self.activityNext = page.next
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

    /// The next page after `activityItems`, appended to it. `activityNext` is
    /// `nil` once the history is exhausted.
    func moreActivity() {
        guard let cursor = activityNext else { return }
        run(.activity) {
            guard let federation = self.federation else { return "join a federation first" }
            let page = try await federation.activity(cursor: cursor, limit: Self.activityPageSize)
            // A reload that started meanwhile owns the list now.
            guard !self.isSuperseded, cursor == self.activityNext else { return "superseded by a newer load" }
            self.activityItems += page.items
            self.activityNext = page.next
            return "loaded \(page.items.count) more"
        }
    }

    /// Brings the newest page of history up to date without the user asking.
    /// The SDK offers no stream of activity, so a balance update is the signal
    /// that something settled. Merged into the list where the two join up, so
    /// rows loaded with `moreActivity` stay, and kept apart from `run(.activity)`
    /// so it never cancels a load the user started.
    private func refreshRecentActivity() {
        guard let federation else { return }
        activityRefreshTask?.cancel()
        activityRefreshTask = Task { [weak self] in
            guard let page = try? await federation.activity(cursor: nil, limit: Self.activityPageSize),
                  !Task.isCancelled, let self, self.federation === federation
            else { return }
            // The newest page has no gaps down to its last row. If it reaches the
            // end of history it is the whole of it. If it reaches a row already
            // loaded, the two join up, and the old cursor still points past the
            // last loaded row. Otherwise more than a page has been created since
            // the last load and the loaded rows no longer join it: start again
            // from this page and its own cursor, and Load more fetches the rest.
            let fresh = Set(page.items.map(\.operationId))
            if page.next == nil {
                self.activityItems = page.items
                self.activityNext = nil
            } else if self.activityItems.contains(where: { fresh.contains($0.operationId) }) {
                self.activityItems = page.items + self.activityItems.filter { !fresh.contains($0.operationId) }
            } else {
                self.activityItems = page.items
                self.activityNext = page.next
            }
        }
    }
    private var activityRefreshTask: Task<Void, Never>?

    // MARK: - Helpers

    private var trimmedInvite: String { invite.trimmed }

    /// An app-private directory. `.applicationSupportDirectory` rather than
    /// `.documentDirectory`: the store is the app's own state, not a user
    /// document, and should not show up in Files.
    ///
    /// The SDK stores the seed phrase unencrypted inside it (see `Storage` in
    /// the Rust crate), so the directory also gets the two protections the SDK
    /// cannot set for itself.
    ///
    /// The Data Protection class is `completeUntilFirstUserAuthentication`, and
    /// deliberately not the stricter-looking `complete`. `complete` makes the
    /// files unreadable whenever the device is locked, so an SDK that keeps
    /// running in the background, or wakes for a notification, fails its next
    /// database read as a storage error. Until-first-unlock keeps the files
    /// encrypted from boot until the user first unlocks, which covers a device
    /// that is powered off or seized at rest. It is also iOS's default class
    /// for app files. Setting it here pins it, rather than leaving it to
    /// whatever an entitlement or a later default says.
    ///
    /// Excluded from backup: otherwise the seed goes into iCloud and into
    /// computer backups, which may not be encrypted. The seed phrase the user
    /// writes down is the backup.
    private static func dataDirectory() throws -> URL {
        var dir = dataDirectoryURL
        let protection: [FileAttributeKey: Any] = [
            .protectionKey: FileProtectionType.completeUntilFirstUserAuthentication,
        ]
        try FileManager.default.createDirectory(
            at: dir, withIntermediateDirectories: true, attributes: protection)
        // `createDirectory` applies `attributes` only when it creates the
        // directory, so set them again for one an earlier launch made. Files
        // created inside it afterwards inherit the directory's class.
        try FileManager.default.setAttributes(protection, ofItemAtPath: dir.path)

        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try dir.setResourceValues(values)
        return dir
    }

    private static var dataDirectoryURL: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("fedimint", isDirectory: true)
    }

    /// An empty file beside the data directory, present while the wallet in it
    /// was created on this device. Beside it rather than inside, so the SDK's
    /// directory holds only what the SDK put there.
    private static var createdMarkerURL: URL {
        dataDirectoryURL.deletingLastPathComponent().appendingPathComponent("fedimint-created")
    }

    /// Records how the wallet just opened came about, and answers
    /// `recoversOnJoin` for it. Creating a wallet sets the marker and restoring
    /// one clears it; a reopen reads what an earlier launch left.
    private static func recoversOnJoin(after origin: WalletOrigin) -> Bool {
        let marker = createdMarkerURL
        switch origin {
        case .created:
            try? Data().write(to: marker)
            return false
        case .restored:
            try? FileManager.default.removeItem(at: marker)
            return true
        case .reopened:
            return !FileManager.default.fileExists(atPath: marker.path)
        }
    }

    /// Whether an earlier launch left a wallet in the data directory.
    ///
    /// Only a hint for which screen to start on, read without creating the
    /// directory. Whether that storage really opens, and with which seed, is
    /// still the SDK's answer in `openWallet`.
    static func hasStoredWallet() -> Bool {
        let contents = try? FileManager.default.contentsOfDirectory(atPath: dataDirectoryURL.path)
        return !(contents ?? []).isEmpty
    }

    /// Makes `joined` the current federation and follows its balance (see
    /// `followBalance`).
    private func attach(_ joined: Federation) {
        attachments += 1
        federation = joined
        hasFederation = true
        balanceTask?.cancel()
        // Every open watcher belongs to the federation being replaced, so its
        // cursor is stale by definition.
        cancelWatchers()
        lnQuote = nil
        onchainQuote = nil
        ecashQuote = nil
        lnSendState = nil
        onchainSendState = nil
        ecashSendState = nil
        lastEcashSend = nil
        recoveryState = nil
        balanceMsats = nil
        balanceFailure = nil

        // Read synchronously, before any await, so these always describe the
        // federation this call attached rather than one that replaced it.
        federationName = joined.name()
        federationId = joined.id()
        federationInvite = joined.inviteCode().display()
        federationNetwork = joined.network()
        capabilities = joined.capabilities()
        federationStatus = try? sdk?.federationStatus(id: joined.id())
        activityRefreshTask?.cancel()
        followers = [:]
        activityItems = []
        activityNext = nil

        // All of this names values from the federation being replaced, and the
        // copy buttons are gated on it being non-nil — so leaving it in place
        // offers the user the previous federation's invoice, address and notes
        // under the new federation's screen. `lastNotes` is the one that
        // matters: e-cash is a bearer token, and those notes are drawn on a
        // different mint.
        lastInvoice = nil
        lastNotes = nil
        lastAddress = nil
        lnReceiveState = nil
        lnReceiveExpiresAt = nil
        lnReceiveInvoiceMsats = nil
        depositFound = false
        depositState = nil

        // Clearing the values is not enough on its own: an action still in
        // flight against the federation being replaced would repopulate them on
        // its way out — A's deposit address, A's invoice, A's watcher — and
        // render the result under B. Cancelling is what makes `isSuperseded`
        // true inside those blocks, which is what actually stops the writes.
        //
        // `federationScoped` deliberately excludes `.join` and `.wallet`:
        // `attach` runs inside one of those, so cancelling the whole set would
        // cancel its own caller and suppress the result it is about to return.
        for section in Section.federationScoped {
            stopRun(section)
            results[section] = nil
        }

        followBalance(of: joined)
    }

    /// Keeps `balanceMsats` live from `joined`'s `balanceUpdates()`, replacing
    /// whatever followed it before. A failure leaves the last amount in place and
    /// says so in `balanceFailure`; following again clears it once an amount
    /// lands.
    private func followBalance(of joined: Federation) {
        balanceTask?.cancel()
        balanceTask = Task { [weak self] in
            do {
                let initial = try await joined.balance()
                // Cancellation is re-checked *after* every await, not just
                // before: `attach` cancels this task when a new federation
                // replaces `joined`, and `balance()`/`next()` are uniffi calls
                // that cancellation cannot interrupt. Writing on the way out
                // would put the old federation's balance on top of the new
                // one's. Same discipline as `watch` below.
                //
                // `if let`, not `guard let`: a `guard let self` here binds for
                // the rest of the `do` block — the loop included — which pins
                // the model for the life of an endless subscription and makes
                // the `[weak self]` above do nothing.
                if !Task.isCancelled, let self {
                    self.balanceMsats = initial
                    self.balanceFailure = nil
                }

                let updates = joined.balanceUpdates()
                while !Task.isCancelled {
                    let next = try await updates.next()
                    if Task.isCancelled { break }
                    // Scoped to one iteration, so nothing strong is held across
                    // the await above. `break` rather than skipping: with the
                    // model gone there is nobody left to render into.
                    guard let self else { break }
                    self.balanceMsats = next
                    self.refreshRecentActivity()
                }
            } catch let error as SdkError {
                // A cancelled subscription failing on its way out must not
                // stamp an error over the federation that replaced it.
                if !Task.isCancelled { self?.balanceFailure = error.reason() }
            } catch {
                if !Task.isCancelled { self?.balanceFailure = "\(error)" }
            }
        }
    }

    /// Runs one SDK action and renders what comes back into its result line,
    /// including an `SdkError` — which is what a real application would branch
    /// on. This is the Swift counterpart of the Android demo's `section`.
    /// Kept in its own dictionary rather than sharing `watchTasks`: a `run`
    /// block may install a watcher on the *same* section, as `run(.lnReceive)`
    /// does with `watch(.lnReceive)`, so one shared key would have the watcher
    /// cancel its own caller.
    ///
    /// Deliberately not cancelled by `attach`, for the mirror-image reason:
    /// `attach` runs inside a run task itself, so cancelling them there would
    /// suppress the very result its caller is about to write. Cross-federation
    /// staleness is handled by clearing the section instead.
    private func fail(_ section: Section, _ reason: String) {
        results[section] = reason
        failures[section] = reason
    }

    /// Drops every held quote: the user declined it, so nothing may execute it.
    func discardQuotes() {
        lnQuote = nil
        onchainQuote = nil
        ecashQuote = nil
    }

    /// What a section's line reads while its action runs.
    private static let workingText = "working…"

    /// Whether `section`'s action is running, for a screen that shows progress.
    /// Tracked apart from `results`: a watcher writes that line too, while the
    /// action that started it may still be running.
    func isWorking(_ section: Section) -> Bool {
        working.contains(section)
    }
    @Published private var working: Set<Section> = []
    /// Which run of a section is the current one, so a replaced run finishing
    /// late does not mark its successor done.
    private var runGenerations: [Section: Int] = [:]

    private func stopRun(_ section: Section) {
        runTasks[section]?.cancel()
        runTasks[section] = nil
        runGenerations[section, default: 0] += 1
        working.remove(section)
    }

    private func run(_ section: Section, _ block: @MainActor @escaping () async throws -> String) {
        stopRun(section)
        let generation = runGenerations[section, default: 0]
        working.insert(section)
        results[section] = Self.workingText
        failures[section] = nil
        runTasks[section] = Task {
            defer { if self.runGenerations[section] == generation { self.working.remove(section) } }
            do {
                let output = try await block()
                // This guards the *result text* only. It does not protect any
                // state the block mutated on its way here — by this point those
                // writes have already happened. Anything a block assigns after
                // an await guards itself with `isSuperseded`; see that property.
                if !Task.isCancelled { self.results[section] = output }
            } catch let error as SdkError {
                if !Task.isCancelled { self.fail(section, self.describe(error)) }
            } catch {
                if !Task.isCancelled { self.fail(section, "\(error)") }
            }
        }
    }

    /// Stops following `section`'s previous operation, without cancelling the
    /// operation itself. A send calls this before its first `await`: `watch`
    /// only replaces the old watcher once the new operation exists, and until
    /// then a state the old one delivers would land on the new send's screen.
    private func stopWatching(_ section: Section) {
        watchTasks[section]?.cancel()
        watchTasks[section] = nil
        watchFailures[section] = nil
    }

    /// Follows an operation's state changes into `section` under `header`, until
    /// the subscription ends — `next()` returns `nil` once the state is final.
    ///
    /// Takes a closure that pulls the cursor rather than the cursor itself, and
    /// that is not an accident. `Operation<S>` is generic in Rust, but a UniFFI
    /// object cannot be, so the crate monomorphises it: there are seven
    /// unrelated `*OperationUpdates` classes with no common protocol between
    /// them. A closure is the one thing that unifies them without inventing a
    /// Swift-only protocol layer the Android SDK does not have. See
    /// `ios/README.md`, "What's not here yet".
    ///
    /// `onState`, when given, also receives each state, for a screen that
    /// renders it itself rather than as this section's text. It runs under the
    /// same cancellation guard as the text write.
    private func watch<State>(
        _ section: Section,
        header: String,
        onState: (@MainActor (State) -> Void)? = nil,
        next: @MainActor @escaping () async throws -> State?
    ) {
        watchTasks[section]?.cancel()
        watchFailures[section] = nil
        // `[weak self]` for the same reason `balanceTask` uses it: the model
        // owns the task and the task would otherwise own the model, and a
        // watcher can stay open for as long as an invoice goes unpaid.
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
                    onState?(state)
                    self.results[section] = "\(header)\n\nstate: \(state)"
                }
                // A `nil` means the state is final, not that anything went
                // wrong; leave the last rendering in place and stop.
            } catch let error as SdkError {
                if !Task.isCancelled {
                    self?.results[section] = "\(header)\n\nstopped watching: \(error.reason())"
                    self?.watchFailures[section] = error.reason()
                }
            } catch {
                if !Task.isCancelled {
                    self?.results[section] = "\(header)\n\nstopped watching: \(error)"
                    self?.watchFailures[section] = "\(error)"
                }
            }
        }
    }

    /// Waits for the next deposit on `deposits`, then follows its operation
    /// with `watch` under `header` plus the operation's id. The id cannot be in
    /// `header` earlier: a deposit has an operation only once the wallet has
    /// found it. This is the Swift counterpart of the Android demo's
    /// `watchNextDeposit`.
    ///
    /// The wait lasts until the wallet finds its next deposit, to any address
    /// it has handed out, and `next()` cannot be interrupted. That is why it
    /// runs in a watcher task and not in the `run` block that showed the
    /// address: a watcher does not hold the model, see `watch`. A wait that
    /// fails, such as the federation closing, is rendered under `header`.
    private func watchNextDeposit(_ deposits: OnchainDeposits, header: String) {
        watchTasks[.deposit]?.cancel()
        watchTasks[.deposit] = Task { [weak self] in
            let operation: OnchainReceiveOperation
            do {
                operation = try await deposits.next()
            } catch let error as SdkError {
                // The address stays on screen above the error: it may already
                // have been shown to a payer.
                if !Task.isCancelled, let self {
                    self.results[.deposit] = "\(header)\n\n\(self.describe(error))"
                    self.watchFailures[.deposit] = error.reason()
                }
                return
            } catch {
                if !Task.isCancelled {
                    self?.results[.deposit] = "\(header)\n\n\(error)"
                    self?.watchFailures[.deposit] = "\(error)"
                }
                return
            }
            // A cancelled watcher gets here once a deposit is found, since the
            // call above cannot be interrupted. That deposit is not its to show.
            // Bound only now, after the wait, so nothing strong was held across
            // it.
            guard !Task.isCancelled, let self else { return }

            let found = "\(header)\n\noperation \(operation.id())"
            self.results[.deposit] = found
            let updates = operation.updates()
            // `watch` cancels the watcher this section has, which is this task.
            // That is the hand-over: nothing follows it here.
            self.depositFound = true
            self.watch(.deposit, header: found, onState: { [weak self] in self?.depositState = $0 }) {
                try await updates.next()
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
/// (MainActivity.kt), so both render an amount identically.
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
