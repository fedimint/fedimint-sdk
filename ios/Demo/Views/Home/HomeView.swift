import SwiftUI

import FedimintSdk

/// 04 Home: the balance, the two things to do with it, and the latest history.
struct HomeView: View {
    @ObservedObject var model: DemoModel
    let receive: () -> Void
    let send: () -> Void
    let showActivity: () -> Void
    let showFederations: () -> Void
    let showRecoveryPhrase: () -> Void

    /// How many recent rows Home shows before "See all".
    private static let recentCount = 3

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Metrics.sectionGap) {
                header
                if model.hasFederation {
                    balance
                    if recovering { recoveryCard }
                    actions
                    recent
                } else {
                    joinPrompt
                }
            }
            .padding(Metrics.screenPadding)
        }
        .refreshable {
            model.refreshBalance()
            model.activity()
        }
        .screenBackground()
        // Home's header row stands in for a navigation bar.
        .toolbar(.hidden, for: .navigationBar)
        .navigationDestination(for: ActivityItem.self) { OperationDetailView(model: model, item: $0) }
        .task(id: model.federationName) {
            // Once per federation: later loads are a pull or a tap, so a
            // redraw never goes back to the SDK behind the user's back.
            if model.hasFederation, model.activityItems.isEmpty { model.activity() }
        }
    }

    private var header: some View {
        HStack(spacing: 8) {
            Image("FedimintMark")
                .resizable()
                .scaledToFit()
                .frame(height: 26)
                .accessibilityHidden(true)
            Text(model.hasFederation ? (model.federationName ?? "Unnamed federation") : "Fedimint")
                .font(Typeface.headline)
                .foregroundStyle(Palette.ink)
            Spacer()
            Menu {
                Button(action: showRecoveryPhrase) {
                    Label("Recovery phrase", systemImage: "key")
                }
            } label: {
                Image(systemName: "ellipsis.circle")
                    .font(.system(size: 20, weight: .medium))
                    .foregroundStyle(Palette.ink)
            }
            .accessibilityLabel("More")
        }
    }

    private var balance: some View {
        VStack(alignment: .leading, spacing: 8) {
            Caption("Total balance")
            if let msats = model.balanceMsats {
                AmountView(msats: msats)
                // The amount is the last one that arrived; nothing updates it
                // until a refresh follows the balance again.
                if let failure = model.balanceFailure {
                    Text("May be out of date: \(failure). Pull down to refresh.")
                        .font(Typeface.footnote)
                        .foregroundStyle(Palette.warning)
                        .textSelection(.enabled)
                }
            } else if let failure = model.balanceFailure {
                Text("Balance unavailable: \(failure)")
                    .font(Typeface.footnote)
                    .foregroundStyle(Palette.danger)
                    .textSelection(.enabled)
            } else {
                HStack(spacing: 8) {
                    Text("—").font(Typeface.amount).foregroundStyle(Palette.muted)
                    ProgressView()
                }
            }
            statusLine
        }
    }

    /// The federation's status and the rails it has, both as the SDK reports
    /// them.
    private var statusLine: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(FederationStatusStyle.tone(model.federationStatus).foreground)
                .frame(width: 7, height: 7)
            Text(([FederationStatusStyle.label(model.federationStatus)] + rails).joined(separator: " · "))
                .font(Typeface.mono(.regular, 13, relativeTo: .footnote))
                .foregroundStyle(Palette.muted)
        }
    }

    private var rails: [String] {
        guard let capabilities = model.capabilities else { return [] }
        return [
            capabilities.lightning ? "LN" : nil,
            capabilities.ecash ? "Ecash" : nil,
            capabilities.onchain ? "Onchain" : nil,
        ].compactMap { $0 }
    }

    /// The SDK refuses sends and receives until a recovery is done.
    private var recovering: Bool {
        if case .recovering = model.federationStatus { return true }
        return false
    }

    private var recoveryCard: some View {
        NavigationLink {
            RecoveryView(model: model)
        } label: {
            ListRow(icon: "arrow.clockwise", title: "Recovering your funds",
                    subtitle: "Sending and receiving open once it's done") {
                Image(systemName: "chevron.right")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(Palette.muted)
            }
            .card(padding: 0)
        }
        .buttonStyle(.plain)
    }

    private var actions: some View {
        HStack(spacing: Metrics.gap) {
            Button(action: receive) {
                Label("Receive", systemImage: "arrow.down.left")
            }
            .buttonStyle(.primary)
            Button(action: send) {
                Label("Send", systemImage: "arrow.up.right")
            }
            .buttonStyle(.secondary)
        }
        .disabled(recovering)
    }

    private var recent: some View {
        VStack(alignment: .leading, spacing: Metrics.gap) {
            HStack {
                Caption("Recent activity")
                Spacer()
                Button("See all", action: showActivity)
                    .font(Typeface.geist(.medium, 15, relativeTo: .subheadline))
                    .foregroundStyle(Palette.primary)
            }
            if model.activityItems.isEmpty {
                Text("No activity yet. Receive something to get started.")
                    .font(Typeface.callout)
                    .foregroundStyle(Palette.muted)
                    .card(padding: 20)
            } else {
                CardList(Array(model.activityItems.prefix(Self.recentCount))) { item in
                    NavigationLink(value: item) { ActivityRow(item: item) }
                        .buttonStyle(.plain)
                }
            }
        }
    }

    /// Before any federation is joined there is nothing to hold a balance in.
    private var joinPrompt: some View {
        VStack(alignment: .leading, spacing: Metrics.gap) {
            Text("Join a federation to start")
                .font(Typeface.headline)
                .foregroundStyle(Palette.ink)
            Text("A federation holds your bitcoin and issues ecash. Paste an invite code from its guardians to join.")
                .font(Typeface.callout)
                .foregroundStyle(Palette.muted)
            Button("Join a federation", action: showFederations)
                .buttonStyle(.primary)
                .padding(.top, 4)
        }
        .card(padding: 20)
    }
}

/// How a `FederationStatus` reads and which tone it carries, shared by every
/// screen that shows one.
enum FederationStatusStyle {
    static func label(_ status: FederationStatus?) -> String {
        switch status {
        case .running: return "Running"
        case .recovering: return "Recovering"
        case .quarantined: return "Quarantined"
        case .closed: return "Closed"
        case .forgetting: return "Forgetting"
        case .forgotten: return "Forgotten"
        case nil: return "Unknown"
        }
    }

    static func tone(_ status: FederationStatus?) -> StatusPill.Tone {
        switch status {
        case .running: return .success
        case .recovering: return .warning
        case .quarantined: return .danger
        case .closed, .forgetting, .forgotten, nil: return .neutral
        }
    }
}
