import SwiftUI

import FedimintSdk

/// 08 Activity: the federation's local history, a page at a time.
struct ActivityView: View {
    @ObservedObject var model: DemoModel

    enum Filter: String, CaseIterable, Identifiable {
        case all = "All", lightning = "Lightning", ecash = "Ecash", onchain = "Onchain"
        var id: Self { self }

        func includes(_ kind: OperationKind) -> Bool {
            switch self {
            case .all: true
            case .lightning: kind == .lnSend || kind == .lnReceive
            case .ecash: kind == .ecashSend || kind == .ecashReceive
            case .onchain: kind == .onchainSend || kind == .onchainReceive
            }
        }
    }

    @State private var filter = Filter.all

    /// The loaded rows that pass the filter, grouped by local day, newest
    /// first. The filter only narrows what is loaded; it never fetches.
    private var days: [(title: String, items: [ActivityItem])] {
        let calendar = Calendar.current
        let shown = model.activityItems.filter { filter.includes($0.kind) }
        let grouped = Dictionary(grouping: shown) { calendar.startOfDay(for: ActivityTime.date($0.time)) }
        return grouped.keys.sorted(by: >).map { day in
            let title = calendar.isDateInToday(day) ? "Today"
                : calendar.isDateInYesterday(day) ? "Yesterday"
                : day.formatted(date: .abbreviated, time: .omitted)
            return (title, grouped[day] ?? [])
        }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Metrics.sectionGap) {
                filters
                if !model.hasFederation {
                    empty("Join a federation to see its history.")
                } else if days.isEmpty {
                    empty(filter == .all ? "No activity yet." : "No \(filter.rawValue.lowercased()) activity loaded.")
                } else {
                    ForEach(days, id: \.title) { day in
                        VStack(alignment: .leading, spacing: Metrics.gap) {
                            Caption(day.title)
                            CardList(day.items) { item in
                                NavigationLink(value: item) { ActivityRow(item: item) }
                                    .buttonStyle(.plain)
                            }
                        }
                    }
                }
                if model.activityNext != nil {
                    Button("Load more") { model.moreActivity() }
                        .buttonStyle(.secondary)
                }
            }
            .padding(Metrics.screenPadding)
        }
        .refreshable { model.activity() }
        .screenBackground()
        .navigationTitle("Activity")
        .navigationBarTitleDisplayMode(.inline)
        .navigationDestination(for: ActivityItem.self) { OperationDetailView(model: model, item: $0) }
        .task { if model.hasFederation, model.activityItems.isEmpty { model.activity() } }
    }

    private var filters: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(Filter.allCases) { option in
                    Button(option.rawValue) { filter = option }
                        .font(Typeface.geist(.medium, 15, relativeTo: .subheadline))
                        .foregroundStyle(option == filter ? .white : Palette.ink)
                        .padding(.horizontal, 16)
                        .frame(height: 36)
                        .background(option == filter ? Palette.navy : Palette.surface, in: Capsule())
                        .overlay(Capsule().strokeBorder(option == filter ? .clear : Palette.border))
                        .accessibilityAddTraits(option == filter ? .isSelected : [])
                }
            }
        }
    }

    private func empty(_ text: String) -> some View {
        Text(text)
            .font(Typeface.callout)
            .foregroundStyle(Palette.muted)
            .card(padding: 20)
    }
}
