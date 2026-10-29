import SwiftUI

import FedimintSdk

/// 13 Federation detail: what the attached federation is and how it stands.
struct FederationDetailView: View {
    @ObservedObject var model: DemoModel
    @State private var showingInvite = false
    @State private var showingRecovery = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Metrics.sectionGap) {
                HStack(spacing: Metrics.gap) {
                    FederationAvatar(name: model.federationName)
                    VStack(alignment: .leading, spacing: 6) {
                        Text(model.federationName ?? "Unnamed federation")
                            .font(Typeface.headline)
                            .foregroundStyle(Palette.ink)
                        StatusPill(text: FederationStatusStyle.label(model.federationStatus),
                                   tone: FederationStatusStyle.tone(model.federationStatus))
                    }
                }

                if case let .quarantined(diagnostic) = model.federationStatus {
                    diagnosticCard(diagnostic)
                }

                if case .recovering = model.federationStatus {
                    Button("View recovery progress") { showingRecovery = true }
                        .buttonStyle(.secondary)
                }

                VStack(spacing: 0) {
                    ReviewRow("Network", NetworkName.of(model.federationNetwork))
                    RowDivider()
                    ReviewRow("Rails", rails)
                    if let guardians {
                        RowDivider()
                        ReviewRow("Guardians", "\(guardians)", monospaced: true)
                    }
                    if let id = model.federationId {
                        RowDivider()
                        HStack {
                            ReviewRow("Federation ID", middleTruncated(id, keep: 6), monospaced: true)
                            CopyIconButton(value: id).padding(.trailing, 8)
                        }
                    }
                }
                .card(padding: 0)

                invite
            }
            .padding(Metrics.screenPadding)
        }
        .screenBackground()
        .navigationTitle("Federation")
        .navigationDestination(isPresented: $showingRecovery) { RecoveryView(model: model) }
        .navigationBarTitleDisplayMode(.inline)
        .onAppear { model.refreshFederationStatus() }
    }

    private var rails: String {
        let rails = PaymentRail.available(model.capabilities).map(\.title)
        return rails.isEmpty ? "None" : rails.joined(separator: " · ")
    }

    /// Only known if this federation's invite was previewed: the SDK reports
    /// guardians in a preview, not on a joined federation.
    private var guardians: UInt16? {
        guard let preview = model.invitePreview, preview.id == model.federationId else { return nil }
        return preview.guardians
    }

    /// The SDK's reason for refusing to run the federation. The message is for
    /// people and may change between versions; the code is what is stable.
    private func diagnosticCard(_ diagnostic: Diagnostic) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Caption("Diagnostic", color: Palette.danger)
            Text("\(diagnostic.code)")
                .font(Typeface.figure)
                .foregroundStyle(Palette.ink)
            Text(diagnostic.message)
                .font(Typeface.footnote)
                .foregroundStyle(Palette.muted)
                .textSelection(.enabled)
        }
        .card()
    }

    /// The code that lets someone else join. It admits whoever holds it, so it
    /// is shown only when asked for.
    @ViewBuilder private var invite: some View {
        if let code = model.federationInvite {
            if showingInvite {
                VStack(spacing: Metrics.gap) {
                    Caption("Invite code")
                    QRCodeView(text: code).frame(maxWidth: 220)
                    HStack {
                        Text(middleTruncated(code, keep: 12))
                            .font(Typeface.mono(.regular, 13, relativeTo: .footnote))
                            .foregroundStyle(Palette.muted)
                        CopyIconButton(value: code)
                    }
                    ShareLink(item: code) { Label("Share invite code", systemImage: "square.and.arrow.up") }
                        .buttonStyle(.primary)
                }
                .frame(maxWidth: .infinity)
                .card(padding: 20)
            } else {
                Button("Show invite code") { showingInvite = true }
                    .buttonStyle(.secondary)
            }
        }
    }
}
