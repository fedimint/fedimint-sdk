import SwiftUI
import UIKit

import FedimintSdk

/// 12 Join: an invite code, a preview of the federation it names, then join.
///
/// A wallet restored from a recovery phrase may already hold funds in the
/// federation it joins, which a plain join would not look for. So unless the
/// wallet is known to have been created on this device, the action is the
/// SDK's `recover`, which joins and rescans (see `DemoModel.recoversOnJoin`).
struct JoinView: View {
    @ObservedObject var model: DemoModel
    @Environment(\.dismiss) private var dismiss

    private var recovering: Bool { model.recoversOnJoin }

    /// Set by Join, so the screen can tell its own join finishing apart from a
    /// preview finishing: both report on the same line.
    @State private var joining = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Metrics.gap) {
                Text("Join a federation")
                    .font(Typeface.title)
                    .foregroundStyle(Palette.ink)
                Text("Paste an invite code from one of its guardians.")
                    .font(Typeface.callout)
                    .foregroundStyle(Palette.muted)

                Field("Invite code") {
                    TextEditor(text: $model.invite)
                        .autocorrectionDisabled()
                        .textInputAutocapitalization(.never)
                        .frame(minHeight: 110)
                        .inputStyle(monospaced: true)
                }
                .padding(.top, Metrics.gap)

                HStack(spacing: Metrics.gap) {
                    Button {
                        if let pasted = UIPasteboard.general.string { model.invite = pasted.trimmed }
                    } label: {
                        Label("Paste", systemImage: "doc.on.clipboard")
                    }
                    Button("Preview") { model.preview() }
                        .disabled(model.invite.trimmed.isEmpty || model.isWorking(.join))
                }
                .buttonStyle(.secondary)

                if let preview = model.invitePreview {
                    PreviewCard(preview: preview).padding(.top, Metrics.gap)
                }

                if recovering {
                    Text("Joining also rescans the federation for funds this wallet's recovery phrase may already hold there.")
                        .font(Typeface.footnote)
                        .foregroundStyle(Palette.muted)
                        .padding(.top, 4)
                }

                if let failure = model.failures[.join] {
                    Text(failure)
                        .font(Typeface.footnote)
                        .foregroundStyle(Palette.danger)
                        .textSelection(.enabled)
                }
            }
            .padding(Metrics.screenPadding)
        }
        .scrollDismissesKeyboard(.interactively)
        .safeAreaInset(edge: .bottom) {
            Button {
                joining = true
                recovering ? model.recover() : model.join()
            } label: {
                BusyLabel(title: recovering ? "Join and recover" : "Join", busy: model.isWorking(.join))
            }
            .buttonStyle(.primary)
            .disabled(model.invite.trimmed.isEmpty || model.isWorking(.join) || model.isSendingEcash)
            .padding(Metrics.screenPadding)
            .background(Palette.background)
        }
        .screenBackground()
        .navigationTitle("Join")
        .navigationBarTitleDisplayMode(.inline)
        // A join that finished without failing is done here, including one that
        // found the federation already joined and reattached to it, which
        // leaves the federation id where it was.
        .onChange(of: model.isWorking(.join)) { _, working in
            guard joining, !working else { return }
            joining = false
            if model.failures[.join] == nil { dismiss() }
        }
    }
}

/// What a federation is, read from its invite code before joining it.
private struct PreviewCard: View {
    let preview: FederationPreview

    var body: some View {
        VStack(spacing: 0) {
            ReviewRow("Name", preview.name ?? "Unnamed")
            RowDivider()
            ReviewRow("Network", NetworkName.of(preview.network))
            RowDivider()
            ReviewRow("Guardians", "\(preview.guardians)", monospaced: true)
            RowDivider()
            ReviewRow("Modules", preview.modules.joined(separator: ", "))
        }
        .card(padding: 0)
    }
}

/// How a `Network` is named on screen.
enum NetworkName {
    static func of(_ network: Network?) -> String {
        switch network {
        case .bitcoin: "mainnet"
        case .testnet: "testnet"
        case .testnet4: "testnet4"
        case .signet: "signet"
        case .regtest: "regtest"
        case nil: "unknown"
        }
    }
}
