import AppKit
import SwiftUI

/// The surface of the window: what is behind it, blurred, under a tint that is nearly opaque.
struct GlassBackground: View {
    var body: some View {
        ZStack {
            Backdrop()
            Color.themeGlassTint
        }
        .ignoresSafeArea()
    }
}

private struct Backdrop: NSViewRepresentable {
    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = .sidebar
        view.blendingMode = .behindWindow
        return view
    }

    func updateNSView(_ view: NSVisualEffectView, context: Context) {}
}

/// The room around a button on `ToolbarGlass`: the buttons touch, and the space seen between
/// them is theirs.
let toolbarButtonInset = EdgeInsets(top: 3, leading: 3, bottom: 3, trailing: 3)

/// A toolbar's buttons on clear glass, which stands out less from the window than the system's.
struct ToolbarGlass<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        let buttons = HStack(spacing: 0) { content }
            .padding(.horizontal, 2)
        if #available(macOS 26.0, *) {
            buttons.glassEffect(.clear, in: .capsule)
        } else {
            buttons
        }
    }
}

extension ToolbarContent {
    @ToolbarContentBuilder
    func withoutSystemGlass() -> some ToolbarContent {
        if #available(macOS 26.0, *) {
            sharedBackgroundVisibility(.hidden)
        } else {
            self
        }
    }
}
