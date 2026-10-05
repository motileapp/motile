import SwiftUI

/// One of a short list a click on a control offers. The list is built only then and lives only
/// while it is open: on the Mac a menu at the pointer, on iOS a sheet.
struct Choice {
    let title: String
    let symbol: Symbol?
    /// It is what is set now.
    let chosen: Bool
    let run: () -> Void

    init(_ title: String, symbol: Symbol? = nil, chosen: Bool = false, run: @escaping () -> Void) {
        self.title = title
        self.symbol = symbol
        self.chosen = chosen
        self.run = run
    }

    /// Offers the choices: on the Mac a menu opens at the pointer, on iOS the view with
    /// `choices(offered)` shows them.
    static func offer(_ choices: [Choice], in offered: Binding<[Choice]>) {
        #if os(macOS)
        pop(choices)
        #else
        offered.wrappedValue = choices
        #endif
    }
}

extension View {
    /// Shows `offered` on iOS, where no menu opens at a finger. On the Mac it is already open.
    @ViewBuilder
    func choices(_ offered: Binding<[Choice]>) -> some View {
        #if os(macOS)
        self
        #else
        let shown = Binding { !offered.wrappedValue.isEmpty } set: { if !$0 { offered.wrappedValue = [] } }
        confirmationDialog("", isPresented: shown, titleVisibility: .hidden) {
            ForEach(offered.wrappedValue.indices, id: \.self) { index in
                let choice = offered.wrappedValue[index]
                Button(choice.chosen ? "✓ \(choice.title)" : choice.title, action: choice.run)
            }
        }
        #endif
    }
}
