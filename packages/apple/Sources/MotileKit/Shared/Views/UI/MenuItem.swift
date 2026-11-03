import SwiftUI

/// An item of a menu. A destructive one is in the system's red, its words and its symbol, which
/// neither the Mac's menus nor iOS's tint give it by themselves.
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
            if role == .destructive {
                Label {
                    Text(title).foregroundStyle(Color(platform: .systemRed))
                } icon: {
                    Image(platform: TintedSymbol.image(symbol, size: .md, color: .systemRed))
                }
            } else {
                Label(title, symbol: symbol)
            }
        }
    }
}
