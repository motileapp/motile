import SwiftUI

/// An item of a menu. A destructive one is red, its words and its symbol: the Mac's menus colour
/// neither, and iOS's colours a symbol with its item's tint.
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
        #if os(macOS)
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
        #else
        Button(role: role, action: action) {
            Label(title, symbol: symbol)
        }
        .tint(role == .destructive ? .red : nil)
        #endif
    }
}
