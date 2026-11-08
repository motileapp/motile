import SwiftUI

/// How every list a dropdown draws is set: rows as tall as a regular control, the same room around
/// them, and a search over them whose symbol and words line up with the rows'.
enum DropdownList {
    /// The room between the card's edge and its rows.
    static let padding: CGFloat = 4
    static let rowHeight = max(ControlSize.regular.height, Platform.minimumPress)
    /// The room between a row's side and what it shows.
    static let rowPadding: CGFloat = 8
    /// The room between a row's symbol and its words.
    static let gap: CGFloat = 8
    static let font = Font.ui(size: 13)
    #if os(macOS)
    /// `font` for the search's `LineField`.
    static let lineFont = NSFont.ui(13)
    #endif
    static let symbol = IconSize.md
    static let check = IconSize.sm
    /// The room between two rows' lights: only drawn, so that the rows' presses meet.
    static let light = EdgeInsets(top: 1, leading: 0, bottom: 1, trailing: 0)
    /// The search over the list stands where a row would, its symbol over the rows' symbols.
    static let searchHeight = rowHeight + 2 * padding
    static let searchPadding = padding + rowPadding
}

extension View {
    /// A row of a dropdown's list, lit by `.dropdownLight()`.
    func dropdownRow() -> some View {
        font(DropdownList.font)
            .foregroundStyle(Color.themeForeground)
            .padding(.horizontal, DropdownList.rowPadding)
            .frame(minHeight: DropdownList.rowHeight)
            .contentShape(Rectangle())
    }
}

extension ButtonStyle where Self == HighlightButtonStyle {
    /// The light of a dropdown's row, under the pointer or the arrow keys.
    static func dropdownLight(lit: Bool = false) -> HighlightButtonStyle {
        .highlight(radius: Radius.sm, lit: lit, inset: DropdownList.light)
    }
}

/// What a group of a dropdown's rows is, and what it is about on the right.
struct DropdownTitle: View {
    let title: String
    var about: String?

    init(_ title: String, about: String? = nil) {
        self.title = title
        self.about = about
    }

    var body: some View {
        HStack(spacing: DropdownList.gap) {
            Text(title)
                .foregroundStyle(Color.themeMutedForeground)
            Spacer(minLength: 0)
            if let about {
                Text(about)
                    .foregroundStyle(Color.themeMutedStrongerForeground)
                    .lineLimit(1)
            }
        }
        .font(ControlSize.regular.font)
        .padding(.horizontal, DropdownList.rowPadding)
        .padding(.top, 8)
        .padding(.bottom, 4)
    }
}
