import SwiftUI

/// What the settings take away once the user says so: a server, a project or an agent's account.
enum Removal: Identifiable {
    case server(Server)
    case project(Project)
    case account(AgentAccount, on: Server)

    var id: String {
        switch self {
        case .server(let server): "server/\(server.id)"
        case .project(let project): "project/\(project.id)"
        case .account(let account, let server): "account/\(server.id)/\(account.id)"
        }
    }

    var title: String {
        switch self {
        case .server(let server): "Remove “\(server.name)”?"
        case .project(let project): "Remove “\(project.name)”?"
        case .account(let account, _): "Remove “\(account.agent.name) · \(account.name)”?"
        }
    }

    func message(_ store: AppStore) -> String {
        switch self {
        case .server(let server):
            "Your account lets go of \(server.name) and its threads leave your clients. What is on it stays there."
        case .project(let project):
            "\(project.name) leaves your clients. Its folder stays on \(store.server(project.serverID)?.name ?? "its server") with everything in it."
        case .account(_, let server):
            "Its threads move to the default account on \(server.name). Its folder and its sign-in stay there."
        }
    }

    func run(_ store: AppStore) {
        switch self {
        case .server(let server): store.removeServer(server)
        case .project(let project): store.removeProject(project)
        case .account(let account, let server): store.removeAgentAccount(account, on: server)
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
        content.confirmationDialog(
            removal?.title ?? "", isPresented: Binding { removal != nil } set: { if !$0 { removal = nil } }, presenting: removal
        ) { removal in
            Button("Remove", role: .destructive) { removal.run(store) }
        } message: { removal in
            Text(removal.message(store))
        }
    }
}
