import SwiftUI

/// What the settings take away once the user says so: a server, every copy of a server's threads,
/// a project, an agent's account, the account's gateway, one of its accounts or one of its MCP servers.
enum Removal: Identifiable {
    case server(Server)
    case serverCompletely(Server)
    case project(Project)
    case account(AgentAccount, on: Server)
    case gateway(name: String)
    case gatewayAccount(GatewayAccount)
    case mcpServer(McpServerRow)
    case backup

    var id: String {
        switch self {
        case .server(let server): "server/\(server.id)"
        case .serverCompletely(let server): "server-completely/\(server.id)"
        case .project(let project): "project/\(project.id)"
        case .account(let account, let server): "account/\(server.id)/\(account.id)"
        case .gateway: "gateway"
        case .gatewayAccount(let account): "gateway-account/\(account.id)"
        case .mcpServer(let mcpServer): "mcp-server/\(mcpServer.id)"
        case .backup: "backup"
        }
    }

    var title: String {
        switch self {
        case .server(let server): "Unlink “\(server.name)”?"
        case .serverCompletely(let server): "Delete “\(server.name)”?"
        case .project(let project): "Remove “\(project.name)”?"
        case .account(let account, _): "Remove “\(account.agent.name) · \(account.name)”?"
        case .gateway: "Remove Your Gateway?"
        case .gatewayAccount(let account): "Remove “\(account.name)” from Your Gateway?"
        case .mcpServer(let mcpServer): "Remove the “\(mcpServer.name)” MCP Server?"
        case .backup: "Turn Off the Backup?"
        }
    }

    func message(_ store: AppStore) -> String {
        switch self {
        case .server(let server):
            "Your account lets go of \(server.name). Its threads stay readable, and can go on, from the copies your other servers keep. If it is your only server, nothing is kept. What is on it stays there."
        case .serverCompletely(let server):
            "\(server.name) leaves your account and every copy of its threads is deleted, on all your servers and clients\(store.backup.bucket == nil ? "" : " and in your backup"). What is on \(server.name) itself stays there."
        case .project(let project):
            "\(project.name) leaves your clients. Its folder stays on \(store.server(project.serverID)?.name ?? "its server") with everything in it."
        case .account(_, let server):
            "Its threads move to the default account on \(server.name). Its folder and its sign-in stay there."
        case .gateway(let name):
            "“\(name)” leaves your account, and your servers stop using it. Its threads move to your default accounts."
        case .gatewayAccount:
            "Your gateway forgets its sign-in, and its threads move to your other accounts."
        case .mcpServer:
            "Agents won't get its tools anymore, and your gateway forgets its login."
        case .backup:
            "Your servers stop backing up to the bucket and sync through your clients again. What is in the bucket stays there."
        }
    }

    /// The button that goes ahead.
    var answer: String {
        switch self {
        case .server: "Unlink"
        case .serverCompletely: "Delete"
        case .backup: "Turn Off"
        default: "Remove"
        }
    }

    func run(_ store: AppStore) {
        switch self {
        case .server(let server): store.removeServer(server)
        case .serverCompletely(let server): store.deleteServerCompletely(server)
        case .project(let project): store.removeProject(project)
        case .account(let account, let server): store.removeAgentAccount(account, on: server)
        case .gateway: store.removeGateway()
        case .gatewayAccount(let account): store.removeGatewayAccount(account)
        case .mcpServer(let mcpServer): store.removeMcpServer(mcpServer)
        case .backup: store.setBackup(nil)
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
        content.alert(
            removal?.title ?? "", isPresented: Binding { removal != nil } set: { if !$0 { removal = nil } }, presenting: removal
        ) { removal in
            Button("Cancel", role: .cancel) {}
            Button(removal.answer, role: .destructive) { removal.run(store) }
        } message: { removal in
            Text(removal.message(store))
        }
        #endif
    }
}
