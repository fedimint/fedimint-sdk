import SwiftUI

import FedimintSdk

/// 10 Recovery: a restored wallet rescanning the federation's history for what
/// its seed owns.
///
/// Until the rescan is done the federation is locked: sends and receives are
/// refused, because spending from notes the rescan has not reached yet could
/// spend one twice. A stopped attempt does not unlock it; only `done` does.
struct RecoveryView: View {
    @ObservedObject var model: DemoModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Metrics.gap) {
                Caption("Restoring wallet")
                Text(headline)
                    .font(Typeface.amount)
                    .monospacedDigit()
                    .foregroundStyle(Palette.ink)
                Text(title)
                    .font(Typeface.title)
                    .foregroundStyle(Palette.ink)
                Text("Scanning the federation's history for ecash issued to your recovery phrase. This can take a few minutes, and keeps running if you leave this screen.")
                    .font(Typeface.callout)
                    .foregroundStyle(Palette.muted)

                progressCard.padding(.top, Metrics.gap)

                if let lost = model.watchFailures[.recovery] {
                    Text("Stopped following the recovery: \(lost). Open this screen again to follow it.")
                        .font(Typeface.footnote)
                        .foregroundStyle(Palette.warning)
                        .textSelection(.enabled)
                }
                if let failure = model.failures[.recovery] {
                    Text(failure)
                        .font(Typeface.footnote)
                        .foregroundStyle(Palette.danger)
                        .textSelection(.enabled)
                }
            }
            .padding(Metrics.screenPadding)
        }
        .safeAreaInset(edge: .bottom) {
            if case .failed = model.recoveryState {
                Button("Resume recovery") { model.resumeRecovery() }
                    .buttonStyle(.primary)
                    .disabled(model.isWorking(.recovery))
                    .padding(Metrics.screenPadding)
                    .background(Palette.background)
            }
        }
        .screenBackground()
        .navigationTitle("Recovery")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear { model.followRecovery() }
    }

    private var fraction: Double? {
        guard case let .running(progress?) = model.recoveryState, progress.total > 0 else { return nil }
        return Double(progress.complete) / Double(progress.total)
    }

    private var headline: String {
        switch model.recoveryState {
        case .done: "100%"
        case .running: fraction.map { "\(Int(($0 * 100).rounded(.down)))%" } ?? "…"
        case .failed, nil: "…"
        }
    }

    private var title: String {
        switch model.recoveryState {
        case .done: "Your funds are recovered"
        case .failed: "Recovery stopped"
        case .running, nil: "Recovering your funds"
        }
    }

    @ViewBuilder private var progressCard: some View {
        VStack(alignment: .leading, spacing: 10) {
            switch model.recoveryState {
            case let .running(progress):
                if let progress, progress.total > 0 {
                    ProgressView(value: Double(progress.complete), total: Double(progress.total))
                        .tint(Palette.primary)
                    Text("\(progress.complete) of \(progress.total)")
                        .font(Typeface.mono(.regular, 13, relativeTo: .footnote))
                        .foregroundStyle(Palette.muted)
                } else {
                    // The rescan has not said how much work it has yet.
                    ProgressView().frame(maxWidth: .infinity)
                }
            case .done:
                StatusPill(text: "Done · sends and receives are open again", tone: .success)
            case let .failed(reason):
                StatusPill(text: "Stopped", tone: .danger)
                Text(reason).font(Typeface.footnote).foregroundStyle(Palette.muted)
            case nil:
                ProgressView().frame(maxWidth: .infinity)
            }
        }
        .card()
    }
}
