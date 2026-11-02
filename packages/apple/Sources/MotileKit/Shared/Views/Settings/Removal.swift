import SwiftUI

/// What the settings take away once the user says so: a server, every copy of a server's threads,
/// a project, an agent's account, the account's proxy or one of its accounts.
enum Removal: Identifiable {
    case server(Server)
    case serverCompletely(Server)
    case project(Project)
    case account(AgentAccount, on: Server)
    case proxy(name: String)
    case proxyAccount(ProxyAccount)

    var id: String {
        switch self {
        case .server(let server): "server/\(server.id)"
        case .serverCompletely(let server): "server-completely/\(server.id)"
        case .project(let project): "project/\(project.id)"
        case .account(let account, let server): "account/\(server.id)/\(account.id)"
        case .proxy: "proxy"
        case .proxyAccount(let account): "proxy-account/\(account.id)"
        }
    }

    var title: String {
        switch self {
        case .server(let server): "Unlink “\(server.name)”?"
        case .serverCompletely(let server): "Delete “\(server.name)”?"
        case .project(let project): "Remove “\(project.name)”?"
        case .account(let account, _): "Remove “\(account.agent.name) · \(account.name)”?"
        case .proxy(let name): "Remove “\(name)”?"
        case .proxyAccount(let account): "Remove “\(account.name)”?"
        }
    }

    func message(_ store: AppStore) -> String {
        switch self {
        case .server(let server):
            "Your account lets go of \(server.name). Its threads stay readable, and can go on, from the copies your other servers keep. If it is your only server, nothing is kept. What is on it stays there."
        case .serverCompletely(let server):
            "\(server.name) leaves your account and every copy of its threads is deleted, on all your servers and clients. What is on \(server.name) itself stays there."
        case .project(let project):
            "\(project.name) leaves your clients. Its folder stays on \(store.server(project.serverID)?.name ?? "its server") with everything in it."
        case .account(_, let server):
            "Its threads move to the default account on \(server.name). Its folder and its sign-in stay there."
        case .proxy:
            "Its accounts go with it."
        case .proxyAccount:
            "Its threads move to your other accounts."
        }
    }

    /// The button that goes ahead.
    var answer: String {
        switch self {
        case .server: "Unlink"
        case .serverCompletely: "Delete"
        default: "Remove"
        }
    }

    func run(_ store: AppStore) {
        switch self {
        case .server(let server): store.removeServer(server)
        case .serverCompletely(let server): store.deleteServerCompletely(server)
        case .project(let project): store.removeProject(project)
        case .account(let account, let server): store.removeAgentAccount(account, on: server)
        case .proxy: store.removeProxy()
        case .proxyAccount(let account): store.removeProxyAccount(account)
        }
    }
}

extension View {
    /// Asks before the removal, and goes ahead only when the user says so.
    func confirmsRemoval(_ removal: Binding<Removal?>) -> some View {
        modifier(RemovalConfirmation(removal: removal))
    }
}

private struct RemovalConfirmation: ViewModifier {
    @Environment(AppStore.self) private var store
    @Binding var removal: Removal?

    func body(content: Content) -> some View {
        #if os(macOS)
        content.asking($removal) { removal in
            Confirmation(title: removal.title, message: removal.message(store), answers: [Answer(title: removal.answer, variant: .danger) { removal.run(store) }])
        }
        #else
        content.confirmationDialog(
            removal?.title ?? "", isPresented: Binding { removal != nil } set: { if !$0 { removal = nil } }, presenting: removal
        ) { removal in
            Button(removal.answer, role: .destructive) { removal.run(store) }
        } message: { removal in
            Text(removal.message(store))
        }
        #endif
    }
}
