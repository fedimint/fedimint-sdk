import SwiftUI

/// The rounded square an icon sits in at the start of a row.
struct IconTile: View {
    let systemName: String

    var body: some View {
        Image(systemName: systemName)
            .font(.system(size: 17, weight: .medium))
            .foregroundStyle(Palette.ink)
            .frame(width: 40, height: 40)
            .background(Palette.subtle, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
}

/// A row in a card list: icon, title over subtitle, and an optional figure on
/// the trailing edge.
struct ListRow<Trailing: View>: View {
    let icon: String
    let title: String
    let subtitle: String?
    let trailing: Trailing

    init(icon: String, title: String, subtitle: String? = nil, @ViewBuilder trailing: () -> Trailing) {
        self.icon = icon
        self.title = title
        self.subtitle = subtitle
        self.trailing = trailing()
    }

    var body: some View {
        HStack(spacing: Metrics.gap) {
            IconTile(systemName: icon)
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(Typeface.headline)
                    .foregroundStyle(Palette.ink)
                if let subtitle {
                    Text(subtitle)
                        .font(Typeface.footnote)
                        .foregroundStyle(Palette.muted)
                }
            }
            Spacer(minLength: Metrics.gap)
            trailing
        }
        .padding(.horizontal, 16)
        .frame(maxWidth: .infinity, minHeight: Metrics.rowHeight)
        // The whole row is the tap target when it sits in a link, gaps and
        // padding included, not only the parts that draw something.
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }
}

extension ListRow where Trailing == EmptyView {
    init(icon: String, title: String, subtitle: String? = nil) {
        self.init(icon: icon, title: title, subtitle: subtitle) { EmptyView() }
    }
}
