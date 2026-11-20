#if os(macOS)
import AppKit
import SwiftUI

/// An HtmlView's page over the whole window, under the window's top bar, which has the chip that
/// puts it back at its end, where the viewer's close button is: the top bar's middle moves the
/// window. Every click, drag, wheel and key in the page is the page's, but Esc, which puts it back.
struct ExpandedPageOverlay: View {
    private static let escape: UInt16 = 53

    @Environment(AppStore.self) private var store
    let page: ExpandedPage
    @State private var keys: Any?

    var body: some View {
        GeometryReader { window in
            let bar = window.safeAreaInsets.top
            ZStack {
                Color.themeBackground
                    .ignoresSafeArea()
                ExpandedPageContent(page: page)
            }
            .overlay(alignment: .topTrailing) {
                ActionButton(
                    "Exit full screen", icon: .minimize2, help: "Exit full screen (Esc)", variant: .outline, size: ProgramWeb.exitSize,
                    symbolSize: ProgramWeb.exitSymbol, round: true
                ) { store.closeExpandedPage() }
                .padding(.trailing, (bar - ProgramWeb.exitSize.height) / 2)
                .frame(height: bar)
                .offset(y: -bar)
            }
        }
        .onAppear {
            keys = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                guard event.keyCode == Self.escape, !(event.window is NSPanel) else { return event }
                store.closeExpandedPage()
                return nil
            }
        }
        .onDisappear {
            if let keys { NSEvent.removeMonitor(keys) }
            keys = nil
        }
    }
}

#endif
