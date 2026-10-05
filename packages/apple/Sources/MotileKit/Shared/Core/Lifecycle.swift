import Foundation
import Network

#if os(macOS)
import AppKit
#endif

/// Tells the core when the client comes to the front, wakes or changes networks, so that it dials
/// again the moment the client is back instead of waiting to find out.
final class Lifecycle {
    private weak var store: AppStore?
    private let monitor = NWPathMonitor()
    private var path: String?

    func start(_ store: AppStore) {
        self.store = store
        monitor.pathUpdateHandler = { [weak self] path in
            let name = path.status == .satisfied ? path.availableInterfaces.first.map { "\($0.type)" } ?? "up" : "down"
            DispatchQueue.main.async { self?.pathChanged(to: name) }
        }
        monitor.start(queue: DispatchQueue(label: "app.motile.network"))
        NotificationCenter.default.addObserver(forName: Platform.becameActive, object: nil, queue: .main) { [weak self] _ in
            self?.store?.core.send("foreground")
        }
        #if os(macOS)
        let workspace = NSWorkspace.shared.notificationCenter
        workspace.addObserver(forName: NSWorkspace.didWakeNotification, object: nil, queue: .main) { [weak self] _ in
            self?.store?.core.send("foreground")
        }
        #endif
    }

    private func pathChanged(to name: String) {
        defer { path = name }
        guard path != nil, path != name else { return }
        store?.core.send("network_changed")
    }
}
