import SwiftUI

/// A symbol's menu for a button at the right of what it lies in: it opens under the button with its
/// right edge on the button's, where SwiftUI's Mac menus open from the button's left edge.
struct EndMenu: View {
    /// AppKit opens a menu 4 points higher than it is asked to, so this leaves 4 between them.
    private static let gap: CGFloat = 8

    let icon: Symbol
    let help: String
    var variant: ButtonVariant = .ghost
    var size: ControlSize = .regular
    var symbolSize: IconSize?
    var round = false
    var margin = EdgeInsets()
    let items: [MenuItem]
    #if os(macOS)
    @State private var anchor = MenuAnchorView()
    #endif

    var body: some View {
        #if os(macOS)
        ActionButton(icon: icon, help: help, variant: variant, size: size, symbolSize: symbolSize, round: round, margin: margin, action: open)
            .background(MenuAnchor(anchor: anchor))
        #else
        ActionMenu(icon: icon, help: help, variant: variant, size: size, symbolSize: symbolSize, round: round, margin: margin) {
            ForEach(items.indices, id: \.self) { items[$0] }
        }
        #endif
    }

    #if os(macOS)
    private func open() {
        guard let view = anchor.view else { return }
        let menu = NSMenu()
        for item in items {
            let entry = ActionMenuItem(title: item.title, run: item.action)
            entry.image = .symbol(item.symbol, size: .md)
            menu.addItem(entry)
        }
        let button = view.bounds.inset(by: margin, flipped: view.isFlipped)
        menu.popUp(underEndOf: button, in: view, gap: Self.gap)
    }
    #endif
}

#if os(macOS)
/// Where a button is in its window, for its menu to open against.
final class MenuAnchorView {
    weak var view: NSView?
}

struct MenuAnchor: NSViewRepresentable {
    let anchor: MenuAnchorView

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        anchor.view = view
        return view
    }

    func updateNSView(_ view: NSView, context: Context) {
        anchor.view = view
    }
}

/// A menu item that runs a closure.
final class ActionMenuItem: NSMenuItem {
    private let run: () -> Void

    init(title: String, run: @escaping () -> Void) {
        self.run = run
        super.init(title: title, action: #selector(chosen), keyEquivalent: "")
        target = self
    }

    required init(coder: NSCoder) {
        fatalError("not used")
    }

    @objc private func chosen() {
        run()
    }
}

extension NSMenu {
    /// Opens `gap` under `rect` of the view, the menu's right edge on the rect's.
    func popUp(underEndOf rect: CGRect, in view: NSView, gap: CGFloat) {
        let below = view.isFlipped ? rect.maxY + gap : rect.minY - gap
        popUp(positioning: nil, at: NSPoint(x: rect.maxX - size.width, y: below), in: view)
    }
}

private extension CGRect {
    /// The rect without the room `insets` leave around what it holds, in a view flipped or not.
    func inset(by insets: EdgeInsets, flipped: Bool) -> CGRect {
        CGRect(
            x: minX + insets.leading, y: minY + (flipped ? insets.top : insets.bottom),
            width: width - insets.leading - insets.trailing, height: height - insets.top - insets.bottom)
    }
}
#endif
