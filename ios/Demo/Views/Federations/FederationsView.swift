import SwiftUI

import FedimintSdk

/// 07 Federations: the federation this wallet holds, and the way to join one.
///
/// The demo works with one federation at a time, so this lists the one that is
/// attached rather than everything the storage remembers.
struct FederationsView: View {
    @ObservedObject var model: DemoModel

    enum Route: Hashable { case join, detail }
    @State private var path: [Route] = []

    var body: some View {
        NavigationStack(path: $path) {
            ScrollView {
                VStack(alignment: .leading, spacing: Metrics.sectionGap) {
                    if model.hasFederation {
                        VStack(alignment: .leading, spacing: Metrics.gap) {
                            Caption("Active · 1")
                            Button { path.append(.detail) } label: { row }
                                .buttonStyle(.plain)
                                .card(padding: 0)
                        }
                    } else {
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
            .onAppear { model.refreshFederationStatus() }
        }
    }

    private var row: some View {
        HStack(spacing: Metrics.gap) {
            FederationAvatar(name: model.federationName)
            VStack(alignment: .leading, spacing: 4) {
                Text(model.federationName ?? "Unnamed federation")
                    .font(Typeface.headline)
                    .foregroundStyle(Palette.ink)
                if let msats = model.balanceMsats {
                    Text("\(SatsFormat.grouped(msats)) sats")
                        .font(Typeface.footnote)
                        .foregroundStyle(Palette.muted)
                }
                StatusPill(text: FederationStatusStyle.label(model.federationStatus),
                           tone: FederationStatusStyle.tone(model.federationStatus))
            }
            Spacer()
            Image(systemName: "chevron.right")
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(Palette.muted)
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
