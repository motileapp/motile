import SwiftUI

/// A box on the secondary background with the border around it: a group of settings, a table,
/// a note, the track of a `Segmented`. What is inside lies on the secondary surface.
private struct Card: ViewModifier {
    let radius: CGFloat

    func body(content: Content) -> some View {
        content
            .environment(\.surface, .secondary)
            .background(Color.themeBackgroundSecondary, in: RoundedRectangle(cornerRadius: radius, style: .continuous))
            .overlay { RoundedRectangle(cornerRadius: radius, style: .continuous).strokeBorder(Color.themeBorder, lineWidth: 1) }
    }
}

extension View {
    func card(radius: CGFloat = Radius.card) -> some View {
        modifier(Card(radius: radius))
    }
}
