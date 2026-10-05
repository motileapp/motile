import SwiftUI

/// The Linear workspace the server is connected to, or how to connect it.
struct LinearSurface: View {
    @Environment(AppStore.self) private var store
    let target: PanelTarget

    var body: some View {
        if let reason = store.linearUnavailable {
            PanelMessage(text: reason)
        } else {
            content
                .panelTask(id: target.serverID) { store.readLinear(target.serverID) }
        }
    }

    @ViewBuilder
    private var content: some View {
        if let connection = store.linear[target.serverID] {
            VStack(spacing: 0) {
                PanelBar {
                    Text(connection.workspace)
                        .font(.ui(size: 12.5, weight: .medium))
                        .foregroundStyle(Color.themeText)
                        .lineLimit(1)
                    Spacer(minLength: 4)
                    ActionMenu(icon: .ellipsis, help: "More") {
                        Button("Disconnect Linear", role: .destructive) { store.disconnectLinear(target.serverID) }
                    }
                }
                PanelMessage(text: "Connected to \(connection.workspace) as \(connection.user).")
            }
        } else {
            connect
        }
    }

    private var connect: some View {
        VStack(spacing: 16) {
            Image(.linear, size: 28)
                .foregroundStyle(Color.themeText)
            VStack(spacing: 6) {
                Text("Connect Linear")
                    .font(.ui(size: 15, weight: .semibold))
                    .foregroundStyle(Color.themeText)
                Text("See your issues here and hand them to your agents. You approve Motile in your browser, and the connection is kept on \(store.server(target.serverID)?.name ?? "your server").")
                    .font(.ui(size: 12.5))
                    .foregroundStyle(Color.themeSecondary)
                    .multilineTextAlignment(.center)
            }
            ActionButton("Connect Linear", variant: .primary, pending: store.connectingLinear == target.serverID) {
                store.connectLinear(target.serverID)
            }
            if let error = store.linearError {
                Text(error)
                    .font(.ui(size: 12.5))
                    .foregroundStyle(Color.themeDanger)
                    .multilineTextAlignment(.center)
                    .textSelection(.enabled)
            }
        }
        .frame(maxWidth: 320)
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
