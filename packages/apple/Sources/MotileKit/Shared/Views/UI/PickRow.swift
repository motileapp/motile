import SwiftUI

/// The text of a choice that is more than a word, as every client sets it.
enum PickText {
    static let title = (font: Theme.proseFont, lineHeight: Theme.proseLineHeight)
    static let cardTitle = (font: PlatformFont.ui(14, weight: .medium), lineHeight: scaled(20))
    static let detail = (font: PlatformFont.ui(13), lineHeight: scaled(20))
    /// The room between the mark and the words.
    static let gap: CGFloat = 8
    /// The room between a title and what it means.
    static let detailGap: CGFloat = 2
    /// The room inside a card of a choice, above and below its words and beside them.
    static let cardPadding = (ends: CGFloat(14), sides: CGFloat(16))
}

/// A checkbox, or a radio of a list, as a row: its mark, then its title and what it means under
/// it. The whole row is pressed. A `struck` title is crossed out, as a thing done is.
struct PickRow: View {
    let title: String
    var detail: String?
    let picked: Bool
    let multiple: Bool
    var struck = false
    var size = ControlSize.regular
    let action: () -> Void
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        Button(action: action) {
            HStack(spacing: PickText.gap) {
                PickMark(picked: picked, multiple: multiple)
                VStack(alignment: .leading, spacing: PickText.detailGap) {
                    Text(title)
                        .strikethrough(struck)
                        .lines(PickText.title.lineHeight, font: PickText.title.font)
                        .foregroundStyle(struck ? Color.themeMutedForeground : Color.themeForeground)
                    if let detail {
                        Text(detail)
                            .lines(PickText.detail.lineHeight, font: PickText.detail.font)
                            .foregroundStyle(Color.themeMutedForeground)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(minHeight: size.height)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .opacity(.disabled, when: !enabled)
        .accessibilityLabel(title)
        .accessibilityAddTraits(picked ? .isSelected : [])
    }
}

/// A choice as a card: its title and what it means, bordered, lit under the pointer, and in the
/// primary colour's wash and edge once it is chosen.
struct OptionCard: View {
    let title: String
    var detail: String?
    let selected: Bool
    let action: () -> Void
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: PickText.detailGap) {
                Text(title)
                    .lines(PickText.cardTitle.lineHeight, font: PickText.cardTitle.font)
                    .foregroundStyle(Color.themeForeground)
                if let detail {
                    Text(detail)
                        .lines(PickText.detail.lineHeight, font: PickText.detail.font)
                        .foregroundStyle(Color.themeMutedForeground)
                }
            }
            .frame(maxWidth: .infinity, alignment: .topLeading)
            .padding(.vertical, PickText.cardPadding.ends)
            .padding(.horizontal, PickText.cardPadding.sides)
        }
        .buttonStyle(OptionCardStyle(selected: selected))
        .opacity(.disabled, when: !enabled)
        .accessibilityLabel(title)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

private struct OptionCardStyle: ButtonStyle {
    let selected: Bool

    func makeBody(configuration: Configuration) -> some View {
        OptionCardBody(label: configuration.label, selected: selected, pressed: configuration.isPressed)
    }
}

private struct OptionCardBody<Label: View>: View {
    let label: Label
    let selected: Bool
    let pressed: Bool
    @State private var hovering = false
    @Environment(\.surface) private var surface
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Radius.lg, style: .continuous)
        label
            .background(fill, in: shape)
            .overlay { shape.strokeBorder(selected ? Color.themePrimary : surface.border, lineWidth: 1) }
            .contentShape(shape)
            .onHover { hovering = $0 }
            .animation(.easeOut(duration: 0.12), value: selected)
    }

    private var fill: AnyShapeStyle {
        if selected { return AnyShapeStyle(Color.themePrimary.wash()) }
        guard enabled else { return AnyShapeStyle(Color.clear) }
        if pressed { return AnyShapeStyle(surface.color(.rowSelected)) }
        return AnyShapeStyle(hovering ? surface.color(.row) : Color.clear)
    }
}
