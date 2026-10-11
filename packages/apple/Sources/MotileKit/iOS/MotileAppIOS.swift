#if os(iOS)
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
    @State private var backgroundTime = BackgroundTime()
    @AppStorage("appearance") private var appearance = Appearance.system
    @Environment(\.scenePhase) private var phase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(store)
                .environment(drawer)
                .foregroundStyle(Color.themeForeground)
                .onAppear {
                    guard !delegate.started else { return }
                    delegate.started = true
                    appearance.apply()
                    // A phone opens on the thread, not on the panel that was open last time.
                    if UIDevice.current.userInterfaceIdiom == .phone { store.sidePanel.isOpen = false }
                    store.start()
                    DemoDriver.startIfRequested(store: store, drawer: drawer)
                }
                .onChange(of: appearance) { appearance.apply() }
                .onChange(of: phase) { backgroundTime.sceneChanged(to: phase) }
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

/// Gives what is on its way when the client goes to the back, like a message or an upload, the
/// time iOS allows to finish.
final class BackgroundTime {
    private var background = UIBackgroundTaskIdentifier.invalid

    func sceneChanged(to phase: ScenePhase) {
        switch phase {
        case .background:
            guard background == .invalid else { return }
            background = UIApplication.shared.beginBackgroundTask { [weak self] in self?.endBackground() }
        case .active:
            endBackground()
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

/// What a keyboard with keys can do, on an iPad, on the keys the shortcuts give them.
struct MotileCommands: Commands {
    let store: AppStore
    let drawer: Drawer

    var body: some Commands {
        let keys = store.shortcuts
        CommandGroup(replacing: .newItem) {
            Button("New Thread") { store.newThread() }
                .shortcut("chat.new", in: keys)
            Button("New Thread in This Project") { store.startNewThread(in: store.composerProject) }
                .shortcut("chat.newLocal", in: keys)
                .disabled(store.composerProject == nil)
            Button("Go to Thread") { store.openPanel(.threads) }
                .shortcut("threadPicker.toggle", in: keys)
            Button("Commands") { store.openPanel(.commands) }
                .shortcut("commandPalette.toggle", in: keys)
        }
        CommandGroup(replacing: .sidebar) {
            Button(drawer.isOpen ? "Hide Sidebar" : "Show Sidebar") { drawer.isOpen.toggle() }
                .shortcut("sidebar.toggle", in: keys)
            Button(store.sidePanel.isOpen ? "Hide Side Panel" : "Show Side Panel") { store.sidePanel.isOpen.toggle() }
                .shortcut("rightPanel.toggle", in: keys)
            Button(store.sidePanel.isMaximized ? "Restore Side Panel" : "Maximize Side Panel") { store.sidePanel.toggleMaximized() }
                .shortcut("rightPanel.toggleMaximized", in: keys)
                .disabled(!store.sidePanel.canMaximize)
            Button("Show Changes") { store.sidePanel.showDiff() }
                .shortcut("rightPanel.diff", in: keys)
                .disabled(store.panelUnavailable != nil || store.panelTarget?.repository != true)
            Button("Show Files") { store.sidePanel.open(.files) }
                .shortcut("rightPanel.files", in: keys)
                .disabled(store.panelUnavailable != nil)
            Button("Show Agents") { store.sidePanel.open(.agents) }
                .shortcut("rightPanel.agents", in: keys)
                .disabled(store.panelUnavailable != nil)
            Button("Show Pull Request") { store.sidePanel.open(.pullRequest) }
                .shortcut("rightPanel.pullRequest", in: keys)
                .disabled(store.panelUnavailable != nil || store.pullRequestsUnavailable != nil)
            Button("Show All Pull Requests") { store.sidePanel.open(.pullRequests) }
                .shortcut("rightPanel.pullRequests", in: keys)
                .disabled(store.panelUnavailable != nil || store.pullRequestsUnavailable != nil || !store.pullRequestsExtended)
            Button("Show Linear") { store.sidePanel.open(.linear) }
                .shortcut("rightPanel.linear", in: keys)
                .disabled(store.panelUnavailable != nil || store.linearUnavailable != nil)
            PanelTabCommands(store: store)
        }
        CommandMenu("Thread") {
            Button(store.selectedThread?.isDone == true ? "Mark Undone" : "Mark Done") { store.toggleDone() }
                .shortcut("thread.done", in: keys)
                .disabled(store.selectedThread == nil)
            Button("Stop") { store.stop() }
                .shortcut("thread.stop", in: keys)
                .disabled(!store.activity.busy)
            Button("Previous Thread") { _ = store.perform("thread.previous") }
                .shortcut("thread.previous", in: keys)
                .disabled(store.jumpThreads.isEmpty)
            Button("Next Thread") { _ = store.perform("thread.next") }
                .shortcut("thread.next", in: keys)
                .disabled(store.jumpThreads.isEmpty)
            Button("Close Tab") { _ = store.sidePanel.closeActive() }
                .shortcut("rightPanel.close", in: keys)
                .disabled(!store.sidePanel.isOpen)
            Divider()
            Button("Add a Project") { store.addProject() }
                .disabled(store.servers.isEmpty)
            Button("Add a Server") { store.showsAddServer = true }
                .disabled(!store.account.signedIn)
            Button("Settings") { store.openSettings() }
                .shortcut("settings.open", in: keys)
            Button("Usage") { store.openUsage() }
                .shortcut("usage.open", in: keys)
                .disabled(!store.account.signedIn)
        }
    }
}

extension AppStore {
    /// An iPad's menus run the commands of its keys themselves.
    func performOnThisSystem(_ command: String) -> Bool {
        false
    }
}

/// What is shown over the client, one at a time. What is asked for last comes over what was there.
private enum RootSheet: Identifiable {
    case commit(Project)
    case addServer
    case panel(PanelPage)
    case settings
    case usage
    case threadSettings

    var id: String {
        switch self {
        case .commit(let project): "commit-\(project.id)"
        case .addServer: "add-server"
        case .panel: "panel"
        case .settings: "settings"
        case .usage: "usage"
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
        .overlay { EscapeKey() }
        .dropdowns()
        .sheet(item: sheet) { sheet in
            Group {
                switch sheet {
                case .commit(let project):
                    CommitSheet(project: project)
                case .addServer:
                    NavigationStack {
                        ConnectServerView(isFirst: false)
                            .navigationTitle("Add a Server")
                            .navigationBarTitleDisplayMode(.inline)
                            .toolbar { ToolbarItem(placement: .topBarTrailing) { SheetCloseButton() } }
                    }
                    .presentationDragIndicator(.visible)
                case .panel(let page):
                    CommandPanel(start: page)
                case .settings:
                    SettingsSheet()
                case .usage:
                    UsageSheet()
                case .threadSettings:
                    ThreadSettingsSheet()
                }
            }
            .sheetSurface()
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
            if let project = store.committingProject { return .commit(project) }
            if store.showsAddServer { return .addServer }
            if let page = store.panel { return .panel(page) }
            if store.settings != nil { return .settings }
            if store.showsUsage { return .usage }
            if store.showsThreadSettings { return .threadSettings }
            return nil
        } set: { new in
            guard new == nil else { return }
            if store.committingProject != nil { return store.committingProject = nil }
            if store.showsAddServer { return store.showsAddServer = false }
            if store.panel != nil { return store.closePanel() }
            if store.settings != nil { return store.closeSettings() }
            if store.showsUsage { return store.showsUsage = false }
            store.showsThreadSettings = false
            store.showsBranches = false
        }
    }
}

/// The sidebar, the thread and its panel. A narrow window has the sidebar and the panel under
/// the thread; a wide one has all three side by side.
struct MainScreen: View {
    static let sidebarWidth: CGFloat = 320
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
        let (store, drawer) = (store, drawer)
        return DrawerView(drawer: drawer, sidePanel: store.sidePanel) {
            SidebarScreen()
                .environment(store)
                .environment(drawer)
                .foregroundStyle(Color.themeForeground)
        } content: {
            NavigationStack {
                ThreadScreen()
            }
            .environment(store)
            .environment(drawer)
            .foregroundStyle(Color.themeForeground)
        } panel: {
            NavigationStack {
                PanelScreen()
            }
            .environment(store)
            .foregroundStyle(Color.themeForeground)
        }
        .ignoresSafeArea()
    }

    private func wide(_ width: CGFloat) -> some View {
        let open = store.sidePanel.isOpen
        let rest = width - Self.sidebarWidth - 1
        let covers = open && (store.sidePanel.isMaximized || rest - 1 - Self.panelWidth < Self.threadBesidePanel)
        return HStack(spacing: 0) {
            SidebarScreen(underThread: false)
                .frame(width: Self.sidebarWidth)
                .background(Color.themeBackground.ignoresSafeArea())
            Self.line
            if covers {
                PanelScreen(beside: true)
            } else {
                NavigationStack {
                    ThreadScreen(overSidebar: false)
                }
                if open {
                    Self.line
                    PanelScreen(beside: true)
                        .frame(width: Self.panelWidth)
                }
            }
        }
        .onAppear { drawer.isOpen = false }
    }

    static var line: some View {
        Rectangle()
            .fill(Color.themeBorder)
            .frame(width: 1)
            .ignoresSafeArea()
    }
}
#endif
