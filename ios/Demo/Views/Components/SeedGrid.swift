import SwiftUI

/// A recovery phrase as the design lays it out: two columns of numbered cells,
/// 01 | 02 on the first row, ruled like a table. Backup fills the cells with
/// the words; Restore with fields to type them into.
struct SeedGrid<Cell: View>: View {
    let count: Int
    /// The cell being typed into, tinted the way the design marks focus.
    var highlighted: Int? = nil
    let cell: (Int) -> Cell

    init(count: Int, highlighted: Int? = nil, @ViewBuilder cell: @escaping (Int) -> Cell) {
        self.count = count
        self.highlighted = highlighted
        self.cell = cell
    }

    var body: some View {
        VStack(spacing: 0) {
            ForEach(Array(stride(from: 0, to: count, by: 2)), id: \.self) { left in
                if left > 0 { Rectangle().fill(Palette.border).frame(height: Metrics.borderWidth) }
                HStack(spacing: 0) {
                    slot(left)
                    Rectangle().fill(Palette.border).frame(width: Metrics.borderWidth)
                    if left + 1 < count { slot(left + 1) } else { Color.clear.frame(maxWidth: .infinity) }
                }
                .frame(height: Metrics.seedWordHeight)
            }
        }
        .card(padding: 0)
        .clipShape(RoundedRectangle(cornerRadius: Metrics.cardRadius, style: .continuous))
    }

    private func slot(_ index: Int) -> some View {
        HStack(spacing: 10) {
            Text(String(format: "%02d", index + 1))
                .font(Typeface.mono(.regular, 13, relativeTo: .footnote))
                .foregroundStyle(Palette.muted)
                            cell(index)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 16)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(index == highlighted ? Palette.primary.opacity(0.08) : .clear)
        .accessibilityElement(children: .contain)
    }
}
