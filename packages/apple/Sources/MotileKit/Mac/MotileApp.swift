#if os(macOS)
import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// What the Mac app's executable runs.
public func runMotile() {
    MotileApp.main()
}

struct MotileApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @State private var store = AppStore()
    @AppStorage("appearance") private var appearance = Appearance.system
    @AppStorage(MainView.sidebarHiddenKey) private var sidebarHidden = false

    var body: some Scene {
        Window("Motile", id: "main") {
            RootView()
                .environment(store)
                .foregroundStyle(Color.themeText)
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
                Button(store.sidePanel.isOpen ? "Hide Side Panel" : "Show Side Panel") { store.sidePanel.isOpen.toggle() }
                    .keyboardShortcut("b", modifiers: [.command, .option])
                Button(store.sidePanel.isMaximized ? "Restore Side Panel" : "Maximize Side Panel") { store.sidePanel.toggleMaximized() }
                    .keyboardShortcut("b", modifiers: [.command, .option, .shift])
                    .disabled(!store.sidePanel.canMaximize)
                Button("Show Changes") { store.sidePanel.showDiff() }
                    .keyboardShortcut("d")
                    .disabled(store.panelUnavailable != nil || store.panelTarget?.repository != true)
                Button("Show Files") { store.sidePanel.open(.files) }
                    .keyboardShortcut("e", modifiers: [.command, .shift])
                    .disabled(store.panelUnavailable != nil)
                Button("Show Agents") { store.sidePanel.open(.agents) }
                    .keyboardShortcut("a", modifiers: [.command, .shift])
                    .disabled(store.panelUnavailable != nil)
                Button("Show Pull Request") { store.sidePanel.open(.pullRequest) }
                    .keyboardShortcut("r", modifiers: [.command, .shift])
                    .disabled(store.panelUnavailable != nil || store.pullRequestsUnavailable != nil)
                Button("Show All Pull Requests") { store.sidePanel.open(.pullRequests) }
                    .keyboardShortcut("r", modifiers: [.command, .option, .shift])
                    .disabled(store.panelUnavailable != nil || store.pullRequestsUnavailable != nil || !store.pullRequestsExtended)
                PanelTabCommands(store: store)
                Divider()
            }
            CommandGroup(replacing: .saveItem) {
                Button("Close") {
                    if store.settings != nil { return store.closeSettings() }
                    if !store.sidePanel.closeActive() { NSApp.keyWindow?.performClose(nil) }
                }
                .keyboardShortcut("w")
            }
            CommandGroup(after: .appInfo) {
                Button("Check for Updates…") { store.updater.check(asked: true) }
            }
            CommandGroup(replacing: .appSettings) {
                Button("Settings…") { store.openSettings() }
                    .keyboardShortcut(",")
            }
            CommandGroup(replacing: .newItem) {
                Button("New Thread…") { store.newThread() }
                    .keyboardShortcut("n")
                Button(store.composerProject.map { "New Thread in “\($0.name)”" } ?? "New Thread in This Project") {
                    store.startNewThread(in: store.composerProject)
                }
                .keyboardShortcut("n", modifiers: [.command, .shift])
                .disabled(store.composerProject == nil)
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
                Button("Add a Project…") { store.addProject() }
                    .disabled(store.servers.isEmpty)
                Button("Add a Server…") { store.showsAddServer = true }
                    .disabled(!store.account.signedIn)
            }
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
                    .putAway(store.settings != nil)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color.themeBackground.ignoresSafeArea())
        .background(WindowReveal(shown: store.ready || store.errorMessage != nil))
        .toolbarBackground(.hidden, for: .windowToolbar)
        .modifier(HiddenWindowTitle())
        .overlay {
            if let section = store.settings {
                SettingsRoute(section: section)
                    .appearing()
            }
        }
        .animation(.easeOut(duration: 0.15), value: store.settings != nil)
        .toolbar {
            if store.settings != nil {
                ToolbarItem(placement: .navigation) {
                    ActionButton("Back", icon: .arrowLeft, help: "Back to the threads (Esc)", variant: .ghost) { store.closeSettings() }
                        .keyboardShortcut(.cancelAction)
                }
                .withoutSystemGlass()
            }
        }
        .overlay {
            ZStack {
                if store.panel != nil {
                    Color.black.opacity(0.32)
                        .ignoresSafeArea()
                        .onTapGesture { store.closePanel() }
                        .appearing()
                }
            }
            .animation(.easeInOut(duration: 0.2), value: store.panel != nil)
        }
        .overlay {
            if let page = store.panel {
                CommandPanel(start: page)
                    .id(page)
            }
        }
        .overlay {
            if let viewing = store.viewing {
                MediaViewer(viewing: viewing)
            }
        }
        .sheet(isPresented: $store.showsAddServer) {
            ConnectServerView(isFirst: false)
                .frame(width: 620)
                .sheetSurface()
        }
        .alert(
            store.errorAlert.title,
            isPresented: Binding(get: { store.errorMessage != nil }, set: { if !$0 { store.errorMessage = nil } })
        ) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(store.errorAlert.detail)
        }
    }
}

/// The sidebar and the open thread, side by side on the window's one surface.
struct MainView: View {
    static let sidebarHiddenKey = "sidebar.hidden"
    static let sidebarWidthKey = "sidebar.width"
    private static let sidebarWidths: ClosedRange<Double> = 240...420
    /// What the sidebar always leaves to the thread, however wide it was dragged.
    private static let threadMinWidth = 500.0

    /// How wide the sidebar may be dragged in a window of the width. The settings' sidebar is
    /// the same width, so the window doesn't shift between the two.
    static func sidebarWidths(in windowWidth: Double) -> ClosedRange<Double> {
        let widest = min(sidebarWidths.upperBound, windowWidth - 1 - threadMinWidth)
        return sidebarWidths.lowerBound...max(sidebarWidths.lowerBound, widest)
    }
    /// What the side panel leaves to the thread. Without that much room, it lies over the thread.
    private static let threadBesidePanel = 400.0
    /// How far the top bar's content starts from the window's left edge while the sidebar is
    /// hidden: past the window's buttons and the ones beside them.
    private static let pastWindowButtons = 240.0
    private static let panelButtonsInset = 10.0

    @Environment(AppStore.self) private var store
    @AppStorage(MainView.sidebarHiddenKey) private var sidebarHidden = false
    @AppStorage(MainView.sidebarWidthKey) private var sidebarWidth = 280.0
    @AppStorage("panel.width") private var panelWidth = 460.0

    var body: some View {
        GeometryReader { window in
            let widths = Self.sidebarWidths(in: Double(window.size.width))
            let shownWidth = min(widths.upperBound, max(widths.lowerBound, sidebarWidth))
            let panelOpen = store.sidePanel.isOpen
            let rest = Double(window.size.width) - (sidebarHidden ? 0 : shownWidth + 1)
            let maximized = store.sidePanel.isMaximized
            let fits = panelOpen && rest - 1 - Self.threadBesidePanel >= SidePanel.widths.lowerBound
            let beside = fits && !maximized
            let panelWidths = SidePanel.widths.lowerBound...max(SidePanel.widths.lowerBound, fits ? rest - 1 - Self.threadBesidePanel : rest - 1)
            let shownPanel = min(panelWidths.upperBound, max(panelWidths.lowerBound, panelWidth))
            HStack(spacing: 0) {
                SidebarView()
                    .frame(width: shownWidth)
                    .overlay(alignment: .topTrailing) {
                        projectButtons
                            .frame(height: window.safeAreaInsets.top)
                            .offset(y: -window.safeAreaInsets.top)
                            .padding(.trailing, SidebarView.rowInset - ToolbarButton.margin)
                    }
                    .frame(width: sidebarHidden ? 0 : shownWidth, alignment: .leading)
                    .putAway(sidebarHidden)
                if !sidebarHidden {
                    PaneDivider(width: $sidebarWidth, widths: widths)
                        .zIndex(1)
                }
                ThreadPane(titleInset: sidebarHidden ? Self.pastWindowButtons : 20, besidePanel: beside)
                    // Behind the maximized panel the thread keeps the width it has beside it,
                    // so the transcript isn't laid out again for a width nobody sees.
                    .frame(width: fits ? rest - 1 - shownPanel : nil)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .putAway(maximized)
                    .overlay(alignment: .trailing) {
                        // One panel for all the places it has, so that it is the same one in each.
                        HStack(spacing: 0) {
                            if !maximized {
                                PaneDivider(width: $panelWidth, widths: panelWidths, growsLeft: true)
                                    .zIndex(1)
                            }
                            SidePanelView(topInset: window.safeAreaInsets.top, tabInset: maximized && sidebarHidden ? Self.pastWindowButtons : 0)
                                .frame(width: maximized ? rest : shownPanel)
                        }
                        .background {
                            if panelOpen && !fits && !maximized {
                                Color.themeBackground
                                    .ignoresSafeArea()
                                    .shadow(color: .black.opacity(0.18), radius: 14, x: -4)
                            }
                        }
                        .environment(\.panelInView, panelOpen)
                        .putAway(!panelOpen)
                    }
            }
            .frame(width: window.size.width, alignment: .leading)
            .overlay(alignment: .topTrailing) {
                panelButtons
                    .frame(height: window.safeAreaInsets.top)
                    .offset(y: -window.safeAreaInsets.top)
                    .padding(.trailing, Self.panelButtonsInset)
            }
            .toolbar {
                if store.settings == nil {
                    ToolbarItem(placement: .navigation) {
                        HStack(spacing: 0) {
                            ToolbarButton(symbol: .panelLeft, help: sidebarHidden ? "Show the sidebar (⌃⌘S)" : "Hide the sidebar (⌃⌘S)") {
                                sidebarHidden.toggle()
                            }
                            if sidebarHidden {
                                projectButtons
                            }
                        }
                        // The system places an item by its width, so it is the same shown and hidden.
                        .frame(width: 3 * ToolbarButton.width, alignment: .leading)
                    }
                    .withoutSystemGlass()
                }
            }
        }
    }

    /// Drawn over the top bar, not as a toolbar item: the system places an item by its width, which
    /// moved the side panel's button when the one beside it came and went.
    private var panelButtons: some View {
        let open = store.sidePanel.isOpen
        let maximized = store.sidePanel.isMaximized
        return HStack(spacing: 0) {
            if open {
                ToolbarButton(
                    symbol: maximized ? .minimize2 : .maximize2,
                    help: maximized ? "Restore the side panel (⇧⌥⌘B)" : "Maximize the side panel (⇧⌥⌘B)"
                ) {
                    store.sidePanel.toggleMaximized()
                }
                .disabled(!store.sidePanel.canMaximize)
            }
            ToolbarButton(symbol: .panelRight, help: open ? "Hide the side panel (⌥⌘B)" : "Show the side panel (⌥⌘B)") {
                store.sidePanel.isOpen.toggle()
            }
        }
    }

    /// Drawn by the sidebar in the top bar while it is shown, so that they end where its rows do
    /// in the same layout pass. A toolbar item sized to the sidebar follows it a frame late.
    private var projectButtons: some View {
        HStack(spacing: 0) {
            ToolbarButton(symbol: .folderPlus, help: "Add a project") {
                store.addProject()
            }
            ToolbarButton(symbol: .squarePen, help: "New thread (⌘N). ⇧-click starts one in this project") {
                guard NSApp.currentEvent?.modifierFlags.contains(.shift) == true else { return store.newThread() }
                store.startNewThread(in: store.composerProject)
            }
        }
    }
}

/// The line between the thread and what is beside it. Dragging it makes the sidebar or the side
/// panel wider or narrower. It is grabbed mostly from the thread's side: the pane's rows light up
/// close to the line, and the grab must not take their clicks.
struct PaneDivider: View {
    private static let threadReach: CGFloat = 8
    private static let paneReach: CGFloat = 4

    @Binding var width: Double
    let widths: ClosedRange<Double>
    /// The pane is on the right of the line, so it grows when the line goes left.
    var growsLeft = false
    @State private var widthAtStart: Double?

    var body: some View {
        Rectangle()
            .fill(Color.themeBorder)
            .frame(width: 1)
            .ignoresSafeArea()
            .overlay {
                Color.clear
                    .frame(width: Self.threadReach + 1 + Self.paneReach)
                    .contentShape(Rectangle())
                    .alignmentGuide(HorizontalAlignment.center) { grab in
                        grab[HorizontalAlignment.center] + (growsLeft ? 1 : -1) * (Self.threadReach - Self.paneReach) / 2
                    }
                    .onHover { inside in
                        if inside { NSCursor.resizeLeftRight.push() } else { NSCursor.pop() }
                    }
                    .gesture(
                        DragGesture(minimumDistance: 1, coordinateSpace: .global)
                            .onChanged { drag in
                                let start = widthAtStart ?? min(widths.upperBound, max(widths.lowerBound, width))
                                widthAtStart = start
                                let moved = growsLeft ? -drag.translation.width : drag.translation.width
                                width = min(widths.upperBound, max(widths.lowerBound, start + moved))
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
            window?.backgroundColor = Theme.background
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

/// The client's icon. Its tile is 824 of the image's 1024 points; the frame makes the tile `size`
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
#endif
