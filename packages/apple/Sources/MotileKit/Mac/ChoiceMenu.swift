#if os(macOS)
import AppKit

extension Choice {
    /// Opens the choices as a menu at the pointer. It exists only while it is open.
    static func pop(_ choices: [Choice]) {
        let menu = NSMenu()
        for choice in choices {
            menu.addItem(ChoiceItem(choice))
        }
        menu.popUp(positioning: nil, at: NSEvent.mouseLocation, in: nil)
    }

    /// Opens the choices as a menu under the view, at least as wide as it, as a pop-up button does.
    static func offer(_ choices: [Choice], under view: NSView) {
        let menu = NSMenu()
        for choice in choices {
            menu.addItem(ChoiceItem(choice))
        }
        menu.minimumWidth = view.bounds.width
        menu.popUp(positioning: nil, at: NSPoint(x: 0, y: view.isFlipped ? view.bounds.maxY + 4 : -4), in: view)
    }
}

private final class ChoiceItem: NSMenuItem {
    private let run: () -> Void

    init(_ choice: Choice) {
        run = choice.run
        super.init(title: choice.title, action: #selector(chosen), keyEquivalent: "")
        target = self
        state = choice.chosen ? .on : .off
        guard let symbol = choice.symbol else { return }
        image = .symbol(symbol, size: 13)
    }

    required init(coder: NSCoder) { fatalError("not used") }

    @objc private func chosen() { run() }
}
#endif
