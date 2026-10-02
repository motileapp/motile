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
