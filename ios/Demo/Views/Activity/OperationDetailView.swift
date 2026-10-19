import SwiftUI

import FedimintSdk

/// 09 Operation detail: one entry of the history, as the SDK describes it now.
///
/// The SDK reports an operation's current state and follows it live, but not
/// when each earlier state was reached, so the progress below marks the steps
/// reached without times.
struct OperationDetailView: View {
    @ObservedObject var model: DemoModel
    let item: ActivityItem

    @StateObject private var follower = OperationFollower()

    var body: some View {
        ScrollView {
            VStack(spacing: Metrics.sectionGap) {
                header
                if let facts = follower.facts { factsCard(facts) }
                if let stage = follower.stage, !stage.steps.isEmpty { progress(stage) }
                if follower.stage?.canCancel == true, let notes = follower.notes { notesCard(notes) }
                if let lost = follower.lost {
                    // Reading the state failed; how the operation ended is
                    // not known from here.
                    Text("The SDK stopped reporting this operation's state: \(lost)")
                        .font(Typeface.footnote)
                        .foregroundStyle(Palette.muted)
                        .textSelection(.enabled)
                }
            }
            .padding(Metrics.screenPadding)
        }
        .safeAreaInset(edge: .bottom) {
            if follower.stage?.canCancel == true {
                Button("Request cancel") { follower.requestCancel() }
                    .buttonStyle(.destructive)
                    .padding(Metrics.screenPadding)
                    .background(Palette.background)
            }
        }
        .screenBackground()
        .navigationTitle(ActivityRow.rail(item.kind))
        .navigationBarTitleDisplayMode(.inline)
        .task { await follower.follow(model: model, item: item) }
    }

    private var header: some View {
        VStack(spacing: 10) {
            IconTile(systemName: ActivityRow.icon(item.kind))
            Text(ActivityRow.title(item))
                .font(Typeface.footnote)
                .foregroundStyle(Palette.muted)
            if let amount = item.amount {
                AmountView(msats: amount, sign: ActivityRow.sign(item.direction))
            }
            if let stage = follower.stage {
                StatusPill(text: stage.label, tone: stage.tone)
            }
        }
        .frame(maxWidth: .infinity)
    }

    private func factsCard(_ facts: OperationFollower.Facts) -> some View {
        VStack(spacing: 0) {
            HStack {
                ReviewRow("Operation ID", middleTruncated(item.operationId, keep: 6), monospaced: true)
                CopyIconButton(value: item.operationId).padding(.trailing, 8)
            }
            RowDivider()
            ReviewRow("Federation", model.federationName ?? "Unnamed")
            ForEach(Array(facts.rows.enumerated()), id: \.offset) { _, row in
                RowDivider()
                ReviewRow(row.label, row.value, monospaced: row.monospaced)
            }
            RowDivider()
            ReviewRow("Created", ActivityTime.date(facts.createdAt).formatted(date: .abbreviated, time: .shortened))
        }
        .card(padding: 0)
    }

    /// An unredeemed send's notes, so they can still be handed over after the
    /// ready screen has moved on to a newer send. Bearer value: copied the
    /// local-only, expiring way, and never shown in full.
    private func notesCard(_ notes: Notes) -> some View {
        let token = notes.display()
        return VStack(alignment: .leading, spacing: Metrics.gap) {
            Caption("Notes")
            HStack {
                Text(middleTruncated(token, keep: 10))
                    .font(Typeface.mono(.regular, 13, relativeTo: .footnote))
                    .foregroundStyle(Palette.ink)
                Spacer()
                CopyIconButton(value: token, bearer: true)
                ShareLink(item: token) { Image(systemName: "square.and.arrow.up") }
                    .foregroundStyle(Palette.ink)
            }
            .card()
        }
    }

    private func progress(_ stage: OperationFollower.Stage) -> some View {
        VStack(alignment: .leading, spacing: Metrics.gap) {
            Caption("Progress")
            VStack(alignment: .leading, spacing: 14) {
                ForEach(Array(stage.steps.enumerated()), id: \.offset) { _, step in
                    HStack(spacing: 10) {
                        Circle()
                            .strokeBorder(step.reached ? Palette.success : Palette.warning, lineWidth: 2)
                            .background(Circle().fill(step.reached ? Palette.success : .clear))
                            .frame(width: 10, height: 10)
                        Text(step.title)
                            .font(Typeface.body)
                            .foregroundStyle(step.reached ? Palette.ink : Palette.warning)
                    }
                }
            }
            .card()
        }
    }
}

/// Looks one operation up, reads its details, and follows its state for as
/// long as the detail screen is open.
@MainActor
final class OperationFollower: ObservableObject {
    struct Facts {
        typealias Row = (label: String, value: String, monospaced: Bool)
        var rows: [Row]
        var createdAt: Timestamp
    }

    struct Stage {
        var label: String
        var tone: StatusPill.Tone
        var steps: [(title: String, reached: Bool)]
        var canCancel = false
    }

    @Published private(set) var facts: Facts?
    @Published private(set) var stage: Stage?
    @Published private(set) var lost: String?
    /// An ecash send's notes, for handing over while they are unredeemed.
    @Published private(set) var notes: Notes?

    /// Kept only to cancel it: the one kind of operation that can be.
    private var ecashSend: EcashSendOperation?

    /// Runs for the life of the screen's `.task`, which cancels it on the way
    /// out; a `next()` already in flight then returns and the loop stops.
    func follow(model: DemoModel, item: ActivityItem) async {
        do {
            guard let operation = try await model.operation(id: item.operationId) else {
                lost = "this federation has no record of the operation"
                return
            }
            if let op = operation.asLnSend() {
                let d = try await op.details()
                facts = Facts(rows: [("Amount", formatMsats(d.invoiceAmount), true), ("Fee", formatMsats(d.fee), true),
                                     ("Total", formatMsats(d.total), true), ("Route", d.route.label, true)],
                              createdAt: d.createdAt)
                try await track(op.updates().next, stage: Self.stage)
            } else if let op = operation.asLnReceive() {
                let d = try await op.details()
                facts = Facts(rows: [("Invoice amount", formatMsats(d.invoiceAmount), true), ("Fee", formatMsats(d.fee), true),
                                     ("Credited", formatMsats(d.netCredit), true)],
                              createdAt: d.createdAt)
                try await track(op.updates().next, stage: Self.stage)
            } else if let op = operation.asEcashSend() {
                ecashSend = op
                let d = try await op.details()
                notes = d.notes
                facts = Facts(rows: [("Notes worth", formatMsats(d.notesValue), true), ("Fee", formatMsats(d.fee), true),
                                     ("Debited", formatMsats(d.totalDebited), true),
                                     ("Reclaimed after", ActivityTime.date(d.reclaimAt).formatted(date: .abbreviated, time: .shortened), false)],
                              createdAt: d.createdAt)
                try await track(op.updates().next, stage: Self.stage)
            } else if let op = operation.asEcashReceive() {
                let d = try await op.details()
                facts = Facts(rows: [("Notes worth", formatMsats(d.notesValue), true), ("Fee", formatMsats(d.fee), true),
                                     ("Credited", formatMsats(d.netCredit), true)],
                              createdAt: d.createdAt)
                try await track(op.updates().next, stage: Self.stage)
            } else if let op = operation.asOnchainSend() {
                let d = try await op.details()
                facts = Facts(rows: [("To", middleTruncated(d.address, keep: 8), true), ("Amount", "\(d.amount) sat", true),
                                     ("Fee", formatMsats(d.fee), true), ("Total", formatMsats(d.total), true)],
                              createdAt: d.createdAt)
                try await track(op.updates().next, stage: Self.stage)
            } else if let op = operation.asOnchainReceive() {
                let d = try await op.details()
                var rows: [Facts.Row] = [("Deposited", "\(d.grossDeposited) sat", true),
                                         ("Transaction", middleTruncated(d.txid, keep: 8), true)]
                if let fee = d.fee { rows.append(("Fee", formatMsats(fee), true)) }
                if let net = d.netCredit { rows.append(("Credited", formatMsats(net), true)) }
                facts = Facts(rows: rows, createdAt: d.createdAt)
                try await track(op.updates().next, stage: Self.stage)
            } else {
                // Recovery, or a kind this build of the SDK does not know.
                facts = Facts(rows: [], createdAt: item.time)
            }
        } catch let error as SdkError {
            if !Task.isCancelled { lost = error.reason() }
        } catch {
            if !Task.isCancelled { lost = "\(error)" }
        }
    }

    func requestCancel() {
        guard let ecashSend else { return }
        Task {
            do { try await ecashSend.requestCancel() } catch let error as SdkError { lost = error.reason() } catch {}
        }
    }

    /// Pulls states until the final one, which `next()` follows with `nil`.
    private func track<State>(_ next: () async throws -> State?, stage: (State) -> Stage) async throws {
        while let state = try await next() {
            if Task.isCancelled { return }
            self.stage = stage(state)
        }
    }

    // MARK: - States to steps

    private static func stage(_ s: LnSendState) -> Stage {
        switch s {
        case .created: Stage(label: "Pending", tone: .warning, steps: [("Submitted", true), ("Funded", false), ("Settled", false)])
        case .funded: Stage(label: "Routing", tone: .warning, steps: [("Submitted", true), ("Funded", true), ("Settled", false)])
        case .success: Stage(label: "Sent", tone: .success, steps: [("Submitted", true), ("Funded", true), ("Settled", true)])
        case .refunded: Stage(label: "Refunded", tone: .neutral, steps: [("Submitted", true), ("Funded", true), ("Refunded to your balance", true)])
        case .failed: Stage(label: "Failed", tone: .danger, steps: [])
        }
    }

    private static func stage(_ s: LnReceiveState) -> Stage {
        switch s {
        case .created, .waitingForPayment: Stage(label: "Waiting for payment", tone: .warning, steps: [("Invoice created", true), ("Paid", false), ("Claimed", false)])
        case .funded: Stage(label: "Claiming", tone: .warning, steps: [("Invoice created", true), ("Paid", true), ("Claimed", false)])
        case .claimed: Stage(label: "Received", tone: .success, steps: [("Invoice created", true), ("Paid", true), ("Claimed", true)])
        case .expired: Stage(label: "Expired", tone: .neutral, steps: [])
        case .canceled: Stage(label: "Canceled", tone: .neutral, steps: [])
        case .failed: Stage(label: "Failed", tone: .danger, steps: [])
        }
    }

    private static func stage(_ s: EcashSendState) -> Stage {
        switch s {
        case .created: Stage(label: "Waiting for the recipient", tone: .warning, steps: [("Notes made", true), ("Redeemed", false)], canCancel: true)
        case .cancelRequested: Stage(label: "Reclaiming", tone: .warning, steps: [("Notes made", true), ("Reclaimed", false)])
        case .canceled: Stage(label: "Reclaimed", tone: .neutral, steps: [("Notes made", true), ("Reclaimed", true)])
        case .redeemed: Stage(label: "Redeemed", tone: .success, steps: [("Notes made", true), ("Redeemed", true)])
        }
    }

    private static func stage(_ s: EcashReceiveState) -> Stage {
        switch s {
        case .created, .issuing: Stage(label: "Redeeming", tone: .warning, steps: [("Submitted", true), ("Issued", false)])
        case .done: Stage(label: "Received", tone: .success, steps: [("Submitted", true), ("Issued", true)])
        case .failed: Stage(label: "Failed", tone: .danger, steps: [])
        }
    }

    private static func stage(_ s: OnchainSendState) -> Stage {
        switch s {
        case .created: Stage(label: "Pending", tone: .warning, steps: [("Submitted", true), ("Broadcast", false)])
        case .succeeded: Stage(label: "Sent", tone: .success, steps: [("Submitted", true), ("Broadcast", true)])
        case .refunded: Stage(label: "Refunded", tone: .neutral, steps: [])
        case .failed: Stage(label: "Failed", tone: .danger, steps: [])
        }
    }

    private static func stage(_ s: OnchainReceiveState) -> Stage {
        switch s {
        case .confirmed: Stage(label: "Claiming", tone: .warning, steps: [("Confirmed", true), ("Claimed", false)])
        case .claimed: Stage(label: "Deposited", tone: .success, steps: [("Confirmed", true), ("Claimed", true)])
        case .failed: Stage(label: "Failed", tone: .danger, steps: [])
        }
    }
}
