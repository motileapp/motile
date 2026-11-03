import SwiftUI

/// An item of a menu, with its symbol in the colour of its words: iOS writes a destructive one in
/// the system's red, which the menu's tint would keep from its symbol, and the Mac in its text colour.
struct MenuItem: View {
    let title: String
    let symbol: Symbol
    var role: ButtonRole?
    let action: () -> Void

    init(_ title: String, symbol: Symbol, role: ButtonRole? = nil, action: @escaping () -> Void) {
        self.title = title
        self.symbol = symbol
        self.role = role
        self.action = action
    }

    var body: some View {
        Button(role: role, action: action) {
            #if os(iOS)
            if role == .destructive {
                Label {
                    Text(title)
                } icon: {
                    Image(platform: TintedSymbol.image(symbol, size: .md, color: .systemRed))
                }
            } else {
                Label(title, symbol: symbol)
            }
            #else
            Label(title, symbol: symbol)
            #endif
        }
    }
}
