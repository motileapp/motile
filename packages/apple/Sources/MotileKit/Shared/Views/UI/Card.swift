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

/// One row of a card that a lazy list draws a row at a time: the card's top edge on its first
/// row, its bottom edge on its last, and its sides on every one.
private struct CardSlice: ViewModifier {
    let top: Bool
    let bottom: Bool
    let radius: CGFloat

    func body(content: Content) -> some View {
        content
            .environment(\.surface, .card)
            .background {
                Color.clear
                    .overlay {
                        RoundedRectangle(cornerRadius: radius, style: .continuous)
                            .fill(Color.themeCard)
                            .strokeBorder(Color.themeBorderCard, lineWidth: 1)
                            .padding(.top, top ? 0 : -radius)
                            .padding(.bottom, bottom ? 0 : -radius)
                    }
                    .clipped()
            }
    }
}

extension View {
    func card(radius: CGFloat = Radius.lg) -> some View {
        modifier(Card(radius: radius))
    }

    func cardSlice(top: Bool, bottom: Bool, radius: CGFloat = Radius.lg) -> some View {
        modifier(CardSlice(top: top, bottom: bottom, radius: radius))
    }
}
