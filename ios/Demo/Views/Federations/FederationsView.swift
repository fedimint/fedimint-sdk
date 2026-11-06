import SwiftUI

import FedimintSdk

/// 07 Federations: every federation this wallet holds, and the way to join one.
///
/// The attached one comes first and opens its detail. The others can be
/// switched to, or reconnected if they are not running.
struct FederationsView: View {
    @ObservedObject var model: DemoModel

    enum Route: Hashable { case join, detail }
    @State private var path: [Route] = []

    /// Everything but the attached federation, in a stable order: the SDK lists
    /// them in none.
    private var others: [FederationInfo] {
        model.storedFederations
            .filter { $0.id != model.federationId }
            .sorted { ($0.name ?? "", $0.id) < ($1.name ?? "", $1.id) }
    }

    var body: some View {
        let others = others
        NavigationStack(path: $path) {
            ScrollView {
                VStack(alignment: .leading, spacing: Metrics.sectionGap) {
                    if model.hasFederation {
                        VStack(alignment: .leading, spacing: Metrics.gap) {
                            Caption("Active")
                            Button { path.append(.detail) } label: { activeRow }
                                .buttonStyle(.plain)
                                .card(padding: 0)
                        }
                    }
                    if !others.isEmpty {
                        VStack(alignment: .leading, spacing: Metrics.gap) {
                            Caption("Other federations · \(others.count)")
                            VStack(spacing: 0) {
                                ForEach(Array(others.enumerated()), id: \.element.id) { index, info in
                                    if index > 0 { RowDivider() }
                                    otherRow(info)
                                }
                            }
                            .card(padding: 0)
                        }
                    }
                    if !model.hasFederation, others.isEmpty {
                        Text("You haven't joined a federation yet. Join one with an invite code from its guardians.")
                            .font(Typeface.callout)
                            .foregroundStyle(Palette.muted)
                            .card(padding: 20)
                    }
                    Button { path.append(.join) } label: {
                        Label("Join with invite code", systemImage: "plus")
                    }
                    .buttonStyle(.secondary)
                }
                .padding(Metrics.screenPadding)
            }
            .screenBackground()
            .navigationTitle("Federations")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .primaryAction) {
                    Button { path.append(.join) } label: { Image(systemName: "plus") }
                        .accessibilityLabel("Join a federation")
                }
            }
            .navigationDestination(for: Route.self) { route in
                switch route {
                case .join: JoinView(model: model)
                case .detail: FederationDetailView(model: model)
                }
            }
            // On appear, which includes coming back from a pushed screen: a
            // failed switch belongs to the visit it happened in.
            .onAppear { model.clearFailure(.switching) }
        }
    }

    private var activeRow: some View {
        // With the failure of a Reconnect started from the detail screen, which
        // can land after the user is back here.
        FederationRow(name: model.federationName, status: model.federationStatus,
                      detail: model.balanceMsats.map { SatsFormat.text($0) },
                      failure: model.switchTarget == model.federationId ? model.failures[.switching] : nil) {
            Image(systemName: "chevron.right")
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(Palette.muted)
        }
    }

    private func otherRow(_ info: FederationInfo) -> some View {
        let action = FederationStatusStyle.action(info.status)
        return Button {
            model.switchFederation(to: info.id)
        } label: {
            FederationRow(name: info.name, status: info.status, detail: NetworkName.of(info.network),
                          failure: model.switchTarget == info.id ? model.failures[.switching] : nil) {
                if model.isWorking(.switching), model.switchTarget == info.id {
                    ProgressView()
                } else if let action {
                    Text(action)
                        .font(Typeface.callout)
                        .foregroundStyle(Palette.navy)
                }
            }
        }
        .buttonStyle(.plain)
        .disabled(action == nil || model.isAttaching || model.isSending)
    }
}

/// A federation in the list: avatar, name, one line under it, its status, and
/// what tapping it does.
private struct FederationRow<Trailing: View>: View {
    let name: String?
    let status: FederationStatus?
    let detail: String?
    /// Why the last switch to this one failed.
    var failure: String? = nil
    @ViewBuilder let trailing: Trailing

    var body: some View {
        HStack(spacing: Metrics.gap) {
            FederationAvatar(name: name)
            VStack(alignment: .leading, spacing: 4) {
                Text(name ?? "Unnamed federation")
                    .font(Typeface.headline)
                    .foregroundStyle(Palette.ink)
                if let detail {
                    Text(detail)
                        .font(Typeface.footnote)
                        .foregroundStyle(Palette.muted)
                }
                StatusPill(text: FederationStatusStyle.label(status),
                           tone: FederationStatusStyle.tone(status))
                if case let .quarantined(diagnostic) = status {
                    Text(diagnostic.message)
                        .font(Typeface.footnote)
                        .foregroundStyle(Palette.danger)
                }
                if let failure {
                    Text(failure)
                        .font(Typeface.footnote)
                        .foregroundStyle(Palette.danger)
                        .textSelection(.enabled)
                }
            }
            Spacer()
            trailing
        }
        .padding(16)
        .contentShape(Rectangle())
    }
}

/// A federation's initials on a navy tile, standing in for a logo the SDK does
/// not provide.
struct FederationAvatar: View {
    let name: String?

    private var initials: String {
        let words = (name ?? "").split(whereSeparator: { !$0.isLetter && !$0.isNumber })
        let letters = words.prefix(2).compactMap(\.first).map(String.init).joined()
        return letters.isEmpty ? "?" : letters.uppercased()
    }

    var body: some View {
        Text(initials)
            .font(Typeface.mono(.semibold, 15, relativeTo: .subheadline))
            .foregroundStyle(.white)
            .frame(width: 44, height: 44)
            .background(Palette.navy, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .accessibilityHidden(true)
    }
}
