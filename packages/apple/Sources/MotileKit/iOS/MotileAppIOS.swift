#if os(iOS)
import Network
import SwiftUI
import UIKit

/// What the iOS app's executable runs.
public func runMotile() {
    MotileApp.main()
}

struct MotileApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @State private var store = AppStore()
    @State private var drawer = Drawer()
    @State private var lifecycle = Lifecycle()
    @AppStorage("appearance") private var appearance = Appearance.system
    @Environment(\.scenePhase) private var phase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(store)
                .environment(drawer)
                .foregroundStyle(Color.themeText)
                .onAppear {
                    guard !delegate.started else { return }
                    delegate.started = true
                    appearance.apply()
                    // A phone opens on the thread, not on the panel that was pushed over it last time.
                    if UIDevice.current.userInterfaceIdiom == .phone { store.sidePanel.isOpen = false }
                    store.start()
                    lifecycle.start(store)
                    DemoDriver.startIfRequested(store: store, drawer: drawer)
                }
                .onChange(of: appearance) { appearance.apply() }
                .onChange(of: phase) { lifecycle.sceneChanged(to: phase) }
        }
        .commands { MotileCommands(store: store, drawer: drawer) }
    }
}

final class AppDelegate: NSObject, UIApplicationDelegate {
    var started = false

    func application(_ application: UIApplication, didFinishLaunchingWithOptions options: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        RowTextView.warmUp()
        return true
    }
}

/// Tells the core when the client comes to the front, goes to the back or changes networks. iOS
/// suspends an app in the background and its connections die there without a word, so the core
/// dials again the moment the client is back instead of waiting to find out.
final class Lifecycle {
    private weak var store: AppStore?
    private let monitor = NWPathMonitor()
    private var path: String?
    private var background = UIBackgroundTaskIdentifier.invalid
    private var leftAt: Date?

    func start(_ store: AppStore) {
        self.store = store
        monitor.pathUpdateHandler = { [weak self] path in
            let name = path.status == .satisfied ? path.availableInterfaces.first.map { "\($0.type)" } ?? "up" : "down"
            DispatchQueue.main.async { self?.pathChanged(to: name) }
        }
        monitor.start(queue: DispatchQueue(label: "app.motile.network"))
    }

    private func pathChanged(to name: String) {
        defer { path = name }
        guard path != nil, path != name else { return }
        store?.core.send("network_changed")
    }

    func sceneChanged(to phase: ScenePhase) {
        switch phase {
        case .background:
            leftAt = leftAt ?? Date()
            // What is on its way, like a message or an upload, gets the time iOS allows to finish.
            guard background == .invalid else { return }
            background = UIApplication.shared.beginBackgroundTask { [weak self] in self?.endBackground() }
        case .active:
            endBackground()
            guard let left = leftAt else { return }
            leftAt = nil
            store?.core.send("foreground", ["away_secs": Int(Date().timeIntervalSince(left))])
        default:
            break
        }
    }

    private func endBackground() {
        guard background != .invalid else { return }
        UIApplication.shared.endBackgroundTask(background)
        background = .invalid
    }
}

/// What a keyboard with keys can do, on an iPad.
struct MotileCommands: Commands {
    let store: AppStore
    let drawer: Drawer

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("New Thread…") { store.newThread() }
                .keyboardShortcut("n")
            Button("New Thread in This Project") { store.startNewThread(in: store.composerProject) }
                .keyboardShortcut("n", modifiers: [.command, .shift])
                .disabled(store.composerProject == nil)
            Button("Go to Thread…") { store.openPanel(.threads) }
                .keyboardShortcut("p")
            Button("Commands…") { store.openPanel(.commands) }
                .keyboardShortcut("k")
        }
        CommandGroup(replacing: .sidebar) {
            Button(drawer.isOpen ? "Hide Sidebar" : "Show Sidebar") { drawer.isOpen.toggle() }
                .keyboardShortcut("s", modifiers: [.command, .control])
            Button(store.sidePanel.isOpen ? "Hide Side Panel" : "Show Side Panel") { store.sidePanel.isOpen.toggle() }
                .keyboardShortcut("b", modifiers: [.command, .option])
            Button(store.sidePanel.isMaximized ? "Restore Side Panel" : "Maximize Side Panel") { store.sidePanel.toggleMaximized() }
                .keyboardShortcut("b", modifiers: [.command, .option, .shift])
                .disabled(!store.sidePanel.isOpen)
            Button("Show Changes") { store.sidePanel.showDiff() }
                .keyboardShortcut("d")
                .disabled(store.panelUnavailable != nil || store.panelTarget?.repository != true)
            Button("Show Files") { store.sidePanel.open(.files) }
                .keyboardShortcut("e", modifiers: [.command, .shift])
                .disabled(store.panelUnavailable != nil)
            Button("Show Agents") { store.sidePanel.open(.agents) }
                .keyboardShortcut("a", modifiers: [.command, .shift])
                .disabled(store.panelUnavailable != nil)
            PanelTabCommands(store: store)
        }
        CommandMenu("Thread") {
            Button(store.selectedThread?.isDone == true ? "Mark Undone" : "Mark Done") { store.toggleDone() }
                .keyboardShortcut("d", modifiers: [.command, .shift])
                .disabled(store.selectedThread == nil)
            Button("Stop") { store.stop() }
                .keyboardShortcut(".")
                .disabled(!store.activity.busy)
            Button("Close Tab") { _ = store.sidePanel.closeActive() }
                .keyboardShortcut("w")
                .disabled(!store.sidePanel.isOpen)
            Divider()
            Button("Add a Project…") { store.addProject() }
                .disabled(store.servers.isEmpty)
            Button("Add a Server…") { store.showsAddServer = true }
                .disabled(!store.account.signedIn)
            Button("Settings…") { store.showsSettings = true }
                .keyboardShortcut(",")
        }
    }
}

/// What is shown over the client, one at a time. What is asked for last comes over what was there.
private enum RootSheet: Identifiable {
    case icon(Project)
    case commit(Project)
    case addServer
    case panel(PanelPage)
    case settings
    case threadSettings

    var id: String {
        switch self {
        case .icon(let project): "icon-\(project.id)"
        case .commit(let project): "commit-\(project.id)"
        case .addServer: "add-server"
        case .panel: "panel"
        case .settings: "settings"
        case .threadSettings: "thread-settings"
        }
    }
}

struct RootView: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        Group {
            if !store.ready {
                Color.clear
            } else if !store.account.signedIn {
                SignInView()
            } else if store.servers.isEmpty {
                ConnectServerView(isFirst: true)
            } else {
                MainScreen()
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color.themeBackground.ignoresSafeArea())
        .sheet(item: sheet) { sheet in
            Group {
                switch sheet {
                case .icon(let project):
                    if let server = store.server(project.serverID) {
                        FolderPicker(server: server, iconFor: project)
                            .frame(maxHeight: .infinity, alignment: .top)
                            .presentationDragIndicator(.visible)
                    }
                case .commit(let project):
                    CommitSheet(project: project)
                case .addServer:
                    ConnectServerView(isFirst: false)
                        .frame(maxHeight: .infinity, alignment: .top)
                        .presentationDragIndicator(.visible)
                case .panel(let page):
                    CommandPanel(start: page)
                        .presentationDetents([.large])
                        .presentationDragIndicator(.visible)
                case .settings:
                    SettingsScreen()
                case .threadSettings:
                    ThreadSettingsSheet()
                }
            }
            .presentationBackground(Color.themeSheet)
        }
        .fullScreenCover(isPresented: Binding(get: { store.viewing != nil }, set: { if !$0 { store.closeViewer() } })) {
            if let viewing = store.viewing {
                MediaViewer(viewing: viewing)
                    .environment(store)
            }
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

    private var sheet: Binding<RootSheet?> {
        Binding {
            if let project = store.iconProject { return .icon(project) }
            if let project = store.committingProject { return .commit(project) }
            if store.showsAddServer { return .addServer }
            if let page = store.panel { return .panel(page) }
            if store.showsSettings { return .settings }
            if store.showsThreadSettings { return .threadSettings }
            return nil
        } set: { new in
            guard new == nil else { return }
            if store.iconProject != nil { return store.iconProject = nil }
            if store.committingProject != nil { return store.committingProject = nil }
            if store.showsAddServer { return store.showsAddServer = false }
            if store.panel != nil { return store.closePanel() }
            if store.showsSettings { return store.showsSettings = false }
            store.showsThreadSettings = false
            store.showsBranches = false
        }
    }
}

/// The settings, as a sheet.
private struct SettingsScreen: View {
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            SettingsView()
                .background(Color.themeSheet)
                .navigationTitle("Settings")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done") { dismiss() }
                    }
                }
        }
    }
}

/// The sidebar, the thread and its panel. A narrow window has the sidebar under the thread and
/// pushes the panel over it; a wide one has all three side by side.
struct MainScreen: View {
    private static let sidebarWidth: CGFloat = 320
    private static let panelWidth: CGFloat = 420
    /// A window at least this wide has the sidebar beside the thread.
    private static let wide: CGFloat = 1000
    /// What the panel leaves to the thread. Without that much room, it covers the thread.
    private static let threadBesidePanel: CGFloat = 400

    @Environment(AppStore.self) private var store
    @Environment(Drawer.self) private var drawer

    var body: some View {
        GeometryReader { window in
            if window.size.width >= Self.wide {
                wide(window.size.width)
            } else {
                narrow
            }
        }
    }

    private var narrow: some View {
        @Bindable var panel = store.sidePanel
        let (store, drawer) = (store, drawer)
        return DrawerView(drawer: drawer) {
            SidebarScreen()
                .environment(store)
                .environment(drawer)
                .foregroundStyle(Color.themeText)
        } content: {
            NavigationStack {
                ThreadScreen()
                    .navigationDestination(isPresented: $panel.isOpen) {
                        PanelScreen()
                    }
            }
            .environment(store)
            .environment(drawer)
            .foregroundStyle(Color.themeText)
        }
        .ignoresSafeArea()
        .onChange(of: panel.isOpen, initial: true) { drawer.isEnabled = !panel.isOpen }
    }

    private func wide(_ width: CGFloat) -> some View {
        let open = store.sidePanel.isOpen
        let rest = width - Self.sidebarWidth - 1
        let covers = open && (store.sidePanel.isMaximized || rest - 1 - Self.panelWidth < Self.threadBesidePanel)
        return HStack(spacing: 0) {
            SidebarScreen(underThread: false)
                .frame(width: Self.sidebarWidth)
                .background(Color.themeBackground.ignoresSafeArea())
            line
            if covers {
                PanelScreen(beside: true)
            } else {
                NavigationStack {
                    ThreadScreen(overSidebar: false)
                }
                if open {
                    line
                    PanelScreen(beside: true)
                        .frame(width: Self.panelWidth)
                }
            }
        }
        .onAppear { drawer.isOpen = false }
    }

    private var line: some View {
        Rectangle()
            .fill(Color.themeBorder)
            .frame(width: 1)
            .ignoresSafeArea()
    }
}
#endif
