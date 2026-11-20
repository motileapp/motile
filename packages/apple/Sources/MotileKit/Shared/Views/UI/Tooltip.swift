import SwiftUI

extension View {
    /// What the view does, shown when the pointer rests on it. On the Mac it is AppKit's own
    /// tooltip: SwiftUI's `help` waits again on every move of the pointer, so a hand that isn't
    /// still never sees it. It is only there while the view is hovered, as AppKit's tooltip
    /// ignores what is drawn over the view.
    func tooltip(_ text: String?) -> some View {
        modifier(Tooltip(text: text?.isEmpty == false ? text : nil))
    }
}

private struct Tooltip: ViewModifier {
    let text: String?
    @State private var hovering = false

    func body(content: Content) -> some View {
        #if os(macOS)
        content
            .background(TooltipCarrier(text: hovering ? text : nil))
            .onHover { hovering = $0 }
            .accessibilityHint(text ?? "")
        #else
        content.help(text ?? "")
        #endif
    }
}

#if os(macOS)
private struct TooltipCarrier: NSViewRepresentable {
    let text: String?

    func makeNSView(context: Context) -> TooltipView { TooltipView() }

    func updateNSView(_ view: TooltipView, context: Context) {
        guard view.toolTip != text else { return }
        view.toolTip = text
    }

    /// It lies behind its view at that view's size; asking AppKit instead builds a layout engine for it.
    func sizeThatFits(_ proposal: ProposedViewSize, nsView: TooltipView, context: Context) -> CGSize? {
        CGSize(width: proposal.width ?? 0, height: proposal.height ?? 0)
    }
}

private final class TooltipView: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}
#endif
