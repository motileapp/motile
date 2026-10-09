import SwiftUI

/// A card: a box of its own with its edge around it, like a group of settings, a table or a
/// note. What is inside lies on the card.
private struct Card: ViewModifier {
    let radius: CGFloat

    func body(content: Content) -> some View {
        content
            .environment(\.surface, .card)
            .background(Color.themeCard, in: RoundedRectangle(cornerRadius: radius, style: .continuous))
            .overlay { RoundedRectangle(cornerRadius: radius, style: .continuous).strokeBorder(Color.themeBorderCard, lineWidth: 1) }
    }
}

extension View {
    func card(radius: CGFloat = Radius.lg) -> some View {
        modifier(Card(radius: radius))
    }
}
