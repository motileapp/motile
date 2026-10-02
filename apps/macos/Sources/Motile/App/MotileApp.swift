import AppKit
import SwiftUI
import UniformTypeIdentifiers

@main
struct MotileApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @State private var store = AppStore()
    @AppStorage("appearance") private var appearance = Appearance.system
    @AppStorage(MainView.sidebarHiddenKey) private var sidebarHidden = false

    var body: some Scene {
        Window("Motile", id: "main") {
            RootView()
                .environment(store)
                .frame(minWidth: 780, minHeight: 500)
                .onAppear {
                    guard !delegate.started else { return }
                    delegate.started = true
                    appearance.apply()
                    store.start()
                    DemoDriver.startIfRequested(store: store)
                }
                .onChange(of: appearance) { appearance.apply() }
        }
        .defaultSize(width: 1180, height: 780)
        .windowToolbarStyle(.unified)
        .commands {
            CommandGroup(replacing: .sidebar) {
                Button(sidebarHidden ? "Show Sidebar" : "Hide Sidebar") { sidebarHidden.toggle() }
                    .keyboardShortcut("s", modifiers: [.command, .control])
            }
            CommandGroup(after: .appInfo) {
                Button("Check for Updates…") { store.updater.check(asked: true) }
            }
            CommandGroup(replacing: .newItem) {
                Button("New Thread…") { store.openPanel(.projects) }
                    .keyboardShortcut("n")
                Button("Go to Thread…") { store.openPanel(.threads) }
                    .keyboardShortcut("p")
                Button("Commands…") { store.openPanel(.commands) }
                    .keyboardShortcut("k")
            }
            CommandMenu("Thread") {
                Button(store.selectedThread?.isDone == true ? "Mark Undone" : "Mark Done") { store.toggleDone() }
                    .keyboardShortcut("d", modifiers: [.command, .shift])
                    .disabled(store.selectedThread == nil)
                Button("Stop") { store.stop() }
                    .keyboardShortcut(".")
                    .disabled(!store.activity.busy)
                Divider()
                Button("Add a Project…") { store.showsFolderPicker = true }
                    .disabled(store.servers.isEmpty)
                Button("Add a Server…") { store.showsAddServer = true }
                    .disabled(!store.account.signedIn)
            }
        }

        Settings {
            SettingsView()
                .environment(store)
        }
    }
}

enum Appearance: String, CaseIterable, Identifiable {
    case system, light, dark

    var id: String { rawValue }

    var label: String {
        switch self {
        case .system: "System"
        case .light: "Light"
        case .dark: "Dark"
        }
    }

    func apply() {
        switch self {
        case .system: NSApp.appearance = nil
        case .light: NSApp.appearance = NSAppearance(named: .aqua)
        case .dark: NSApp.appearance = NSAppearance(named: .darkAqua)
        }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    var started = false

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Needed when launched as a bare executable, without an app bundle.
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
        RowTextView.warmUp()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }
}

struct RootView: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        @Bindable var store = store
        Group {
            if !store.ready {
                Color.clear
            } else if !store.account.signedIn {
                SignInView()
            } else if store.servers.isEmpty {
                ConnectServerView(isFirst: true)
            } else {
                MainView()
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(GlassBackground())
        .background(WindowReveal(shown: store.ready || store.errorMessage != nil))
        .toolbarBackground(.hidden, for: .windowToolbar)
        .modifier(HiddenWindowTitle())
        .overlay {
            if let page = store.panel {
                CommandPanel(start: page)
                    .id(page)
            }
        }
        .sheet(isPresented: $store.showsAddServer) {
            ConnectServerView(isFirst: false)
                .frame(width: 620)
                .background(Color.themeBackground)
        }
        .alert(
            "Something went wrong",
            isPresented: Binding(get: { store.errorMessage != nil }, set: { if !$0 { store.errorMessage = nil } })
        ) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(store.errorMessage ?? "")
        }
    }
}

/// The sidebar and the open thread, side by side on the window's one surface.
struct MainView: View {
    static let sidebarHiddenKey = "sidebar.hidden"
    private static let sidebarWidths: ClosedRange<Double> = 240...420
    /// What the sidebar always leaves to the thread, however wide it was dragged.
    private static let threadMinWidth = 500.0

    @Environment(AppStore.self) private var store
    @AppStorage(MainView.sidebarHiddenKey) private var sidebarHidden = false
    @AppStorage("sidebar.width") private var sidebarWidth = 280.0

    var body: some View {
        @Bindable var store = store
        GeometryReader { window in
            let widest = min(Self.sidebarWidths.upperBound, Double(window.size.width) - 1 - Self.threadMinWidth)
            let widths = Self.sidebarWidths.lowerBound...max(Self.sidebarWidths.lowerBound, widest)
            HStack(spacing: 0) {
                if !sidebarHidden {
                    SidebarView()
                        .frame(width: min(widths.upperBound, max(widths.lowerBound, sidebarWidth)))
                    SidebarDivider(width: $sidebarWidth, widths: widths)
                        .zIndex(1)
                }
                ThreadPane(titleInset: sidebarHidden ? 240 : 20)
            }
        }
        .toolbar {
            ToolbarItem(placement: .navigation) {
                ToolbarGlass {
                    IconOnlyButton(symbol: "sidebar.left", help: sidebarHidden ? "Show the sidebar (⌃⌘S)" : "Hide the sidebar (⌃⌘S)", size: 30, symbolSize: 15, inset: toolbarButtonInset) {
                        sidebarHidden.toggle()
                    }
                    IconOnlyButton(symbol: "folder.badge.plus", help: "Add a project", size: 30, symbolSize: 15, inset: toolbarButtonInset) {
                        store.showsFolderPicker = true
                    }
                    IconOnlyButton(symbol: "square.and.pencil", help: "New thread", size: 30, symbolSize: 15, inset: toolbarButtonInset) {
                        store.startNewThread()
                    }
                }
            }
            .withoutSystemGlass()
        }
        .onDrop(of: [UTType.fileURL] + ImageFiles.attachable, isTargeted: $store.dropTargeted) { providers in
            store.attach(dropped: providers)
            return true
        }
    }
}

/// The line between the sidebar and the thread. Dragging it makes the sidebar wider or narrower.
private struct SidebarDivider: View {
    @Binding var width: Double
    let widths: ClosedRange<Double>
    @State private var widthAtStart: Double?

    var body: some View {
        Rectangle()
            .fill(Color.themeBorder)
            .frame(width: 1)
            .ignoresSafeArea()
            .overlay {
                Color.clear
                    .frame(width: Theme.resizeGrab)
                    .contentShape(Rectangle())
                    .onHover { inside in
                        if inside { NSCursor.resizeLeftRight.push() } else { NSCursor.pop() }
                    }
                    .gesture(
                        DragGesture(minimumDistance: 1, coordinateSpace: .global)
                            .onChanged { drag in
                                let start = widthAtStart ?? min(widths.upperBound, max(widths.lowerBound, width))
                                widthAtStart = start
                                width = min(widths.upperBound, max(widths.lowerBound, start + drag.translation.width))
                            }
                            .onEnded { _ in widthAtStart = nil }
                    )
            }
    }
}

/// Keeps the window invisible until `shown`, so that the first thing seen is the finished layout.
private struct WindowReveal: NSViewRepresentable {
    let shown: Bool

    func makeNSView(context: Context) -> RevealView { RevealView() }

    func updateNSView(_ view: RevealView, context: Context) {
        guard shown else { return }
        view.shown = true
    }

    final class RevealView: NSView {
        var shown = false {
            didSet { window?.alphaValue = shown ? 1 : 0 }
        }

        override func viewDidMoveToWindow() {
            window?.alphaValue = shown ? 1 : 0
        }
    }
}

/// The window's title is drawn by the thread pane, over the pane, so the toolbar shows none.
private struct HiddenWindowTitle: ViewModifier {
    func body(content: Content) -> some View {
        if #available(macOS 15.0, *) {
            content.toolbar(removing: .title)
        } else {
            content.background(WindowTitleHider())
        }
    }
}

private struct WindowTitleHider: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView { NSView() }

    func updateNSView(_ view: NSView, context: Context) {
        DispatchQueue.main.async { view.window?.titleVisibility = .hidden }
    }
}

/// The app's icon. Its tile is 824 of the image's 1024 points; the frame makes the tile `size`
/// wide.
struct LogoView: View {
    let size: CGFloat

    var body: some View {
        Image(nsImage: NSApp.applicationIconImage)
            .resizable()
            .interpolation(.high)
            .frame(width: size * 1024 / 824, height: size * 1024 / 824)
            .frame(width: size, height: size)
    }
}
