import SwiftUI

/// A box with the border around it: a group of settings, a table, a note. What is inside lies
/// on the card.
private struct Card: ViewModifier {
    let radius: CGFloat

    func body(content: Content) -> some View {
        content
            .layered(in: RoundedRectangle(cornerRadius: radius, style: .continuous))
            .overlay { RoundedRectangle(cornerRadius: radius, style: .continuous).strokeBorder(Color.themeBorderCard, lineWidth: 1) }
    }
}

extension View {
    func card(radius: CGFloat = Radius.card) -> some View {
        modifier(Card(radius: radius))
    }
}
