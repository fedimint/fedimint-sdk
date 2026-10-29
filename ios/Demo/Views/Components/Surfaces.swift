import SwiftUI

extension View {
    /// The design's card: a white surface with a hairline border.
    func card(padding: CGFloat = 16) -> some View {
        self
            .padding(padding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: Metrics.cardRadius, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: Metrics.cardRadius, style: .continuous)
                    .strokeBorder(Palette.border, lineWidth: Metrics.borderWidth)
            )
    }

    /// Fills the screen with the design's ground, under the safe areas too.
    func screenBackground() -> some View {
        background(Palette.background.ignoresSafeArea())
    }
}

/// Rows stacked inside one card with dividers between them, the layout every
/// list in the design uses.
struct CardList<Item, Row: View>: View {
    let items: [Item]
    let row: (Item) -> Row

    init(_ items: [Item], @ViewBuilder row: @escaping (Item) -> Row) {
        self.items = items
        self.row = row
    }

    var body: some View {
        VStack(spacing: 0) {
            // Offsets as identity: the rows are a snapshot rendered in order,
            // and nothing here reorders or animates them individually.
            ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                if index > 0 { RowDivider(inset: 16) }
                row(item)
            }
        }
        .card(padding: 0)
    }
}

/// The hairline between the rows of a card, inset to the rows' text the way
/// the design draws it.
struct RowDivider: View {
    var inset: CGFloat = 20

    var body: some View {
        Rectangle()
            .fill(Palette.border)
            .frame(height: Metrics.borderWidth)
            .padding(.horizontal, inset)
    }
}
