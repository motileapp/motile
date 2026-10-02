import AppKit
import SwiftUI

@main
struct MotileApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @State private var store = AppStore()
    @AppStorage("appearance") private var appearance = Appearance.system

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
            SidebarCommands()
            CommandGroup(replacing: .newItem) {
                Button("New Thread") { store.startNewThread() }
                    .keyboardShortcut("n")
            }
            CommandMenu("Thread") {
                Button(store.selectedThread?.isDone == true ? "Mark Undone" : "Mark Done") { store.toggleDone() }
                    .keyboardShortcut("d", modifiers: [.command, .shift])
                    .disabled(store.selectedThread == nil)
                Button("Stop") { store.stop() }
                    .keyboardShortcut(".")
                    .disabled(!store.activity.running)
                Divider()
                Button("Add a Host…") { store.showsAddHost = true }
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
            } else if store.hosts.isEmpty {
                ConnectHostView(isFirst: true)
            } else {
                MainView()
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color.themeBackground)
        .sheet(isPresented: $store.showsAddHost) {
            ConnectHostView(isFirst: false)
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

struct MainView: View {
    var body: some View {
        NavigationSplitView {
            SidebarView()
                .navigationSplitViewColumnWidth(min: 230, ideal: 280, max: 420)
        } detail: {
            ThreadPane()
        }
    }
}

/// The app's mark: a rounded square with an "m" that is one continuous stroke.
struct LogoView: View {
    let size: CGFloat

    var body: some View {
        RoundedRectangle(cornerRadius: size * 0.23, style: .continuous)
            .fill(
                LinearGradient(
                    colors: [Color(red: 0.31, green: 0.486, blue: 1), Color(red: 0.118, green: 0.247, blue: 0.839)],
                    startPoint: .top,
                    endPoint: .bottom
                )
            )
            .overlay(
                LogoStroke().stroke(.white, style: StrokeStyle(lineWidth: size * 0.09, lineCap: .round, lineJoin: .round))
            )
            .frame(width: size, height: size)
    }
}

private struct LogoStroke: Shape {
    func path(in rect: CGRect) -> Path {
        let unit = rect.width / 100
        func point(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: rect.minX + x * unit, y: rect.minY + y * unit) }
        var path = Path()
        path.move(to: point(27, 69))
        path.addLine(to: point(27, 46))
        path.addArc(center: point(38.5, 46), radius: 11.5 * unit, startAngle: .degrees(180), endAngle: .degrees(360), clockwise: false)
        path.addLine(to: point(50, 69))
        path.move(to: point(50, 46))
        path.addArc(center: point(61.5, 46), radius: 11.5 * unit, startAngle: .degrees(180), endAngle: .degrees(360), clockwise: false)
        path.addLine(to: point(73, 69))
        return path
    }
}
