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

/// A button on `ToolbarGlass`. The buttons touch, and the space seen between them is theirs.
/// The light under the pointer is a circle, the same distance from the capsule all around.
struct ToolbarGlassButton: View {
    private static let size: CGFloat = 30
    private static let margin: CGFloat = 3

    let symbol: String
    let help: String
    let action: () -> Void

    var body: some View {
        IconOnlyButton(
            symbol: symbol,
            help: help,
            size: Self.size,
            symbolSize: 15,
            radius: Self.size / 2,
            inset: EdgeInsets(top: Self.margin, leading: Self.margin, bottom: Self.margin, trailing: Self.margin),
            action: action
        )
    }
}

/// A toolbar's buttons on clear glass, which stands out less from the window than the system's.
struct ToolbarGlass<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        let buttons = HStack(spacing: 0) { content }
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
