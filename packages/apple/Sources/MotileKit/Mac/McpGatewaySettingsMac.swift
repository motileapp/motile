#if os(macOS)
import SwiftUI

/// Settings → MCP Gateway: the MCP servers the account's gateway signs in to, a card each with its tools,
/// and the calls agents made to them. The core follows the gateway while the page shows.
struct McpGatewaySettings: View {
    @Environment(AppStore.self) private var store
    @State private var adding = false
    @State private var sheet: McpServerSheet?
    @State private var removal: Removal?
    @State private var call: McpServerCallEntry?
    @State private var unfolded: Set<String> = []
    @State private var calls = McpServerCalls()

    var body: some View {
        let gateway = store.gateway
        VStack(alignment: .leading, spacing: SettingsGroupTitle.groupGap) {
            GatewayHeader(caption: GatewayText.mcpCaption)
            if gateway.exists {
                mcpServers(gateway)
            }
            if gateway.exists, gateway.mcpServers?.isEmpty == false {
                activity
            }
        }
        .onAppear { store.watchGateway(true) }
        .onDisappear { store.watchGateway(false) }
        .onChange(of: gateway.mcpServers != nil, initial: true) {
            guard store.gateway.mcpServers != nil else { return }
            calls.start(store)
        }
        .onChange(of: gateway.callsHead) { calls.refresh() }
        .confirmsRemoval($removal)
        .modal(isPresented: $adding) { McpServerAddSheet() }
        .modal(item: $sheet) { sheet in
            switch sheet.kind {
            case .tryTool: McpServerTrySheet(mcpServer: sheet.mcpServer)
            case .rename: McpServerRenameSheet(mcpServer: sheet.mcpServer)
            case .key: McpServerKeySheet(mcpServer: sheet.mcpServer)
            }
        }
        .modal(item: $call) { McpServerCallSheet(entry: $0.id) }
    }

    /// The way to add an MCP server and a card for each, or why there can't be any yet.
    @ViewBuilder private func mcpServers(_ gateway: GatewayState) -> some View {
        SettingsGroup("mcp-servers", "MCP Servers", carded: false) {
            if let mcpServers = gateway.mcpServers {
                ActionButton("Add MCP Server", icon: .plus, variant: .outline, size: .large) { adding = true }
                    .frame(maxWidth: .infinity, alignment: .leading)
                if mcpServers.isEmpty {
                    SettingsRow {
                        SettingsLabel(McpServerText.none, description: McpServerText.noneDetail, icon: .plug)
                    } trailing: {
                        EmptyView()
                    }
                    .card()
                }
                // Lazy, so that a group of a hundred tools only builds the rows on screen.
                LazyVStack(spacing: 0) {
                    ForEach(Self.items(of: mcpServers, unfolded: unfolded)) { item in
                        self.item(item)
                            .clipShape(Self.slice(top: item.top, bottom: item.bottom))
                            .cardSlice(top: item.top, bottom: item.bottom)
                            .padding(.top, item.top && item.mcpServer.id != mcpServers.first?.id ? Self.cardGap : 0)
                    }
                }
            } else {
                SettingsRow {
                    SettingsLabel(
                        gateway.connection == .connected ? McpServerText.updateGateway : gateway.statusText,
                        description: gateway.connection == .connected ? gateway.version.map { "Your gateway runs version \($0)" } : gateway.error,
                        icon: .waypoints
                    )
                } trailing: {
                    if gateway.connection == .connecting {
                        Spinner().foregroundStyle(Color.themeMutedForeground)
                    }
                }
                .card()
            }
        }
    }

    /// The MCP servers' cards as one list of slices: each card's header, its groups, and the tools
    /// of the groups that are unfolded.
    private static func items(of mcpServers: [McpServerRow], unfolded: Set<String>) -> [McpCardItem] {
        mcpServers.flatMap { mcpServer in
            var items = [McpCardItem(id: mcpServer.id, mcpServer: mcpServer, kind: .header)]
            for group in mcpServer.groups {
                let key = "\(mcpServer.id)/\(group.id)"
                let open = unfolded.contains(key)
                items.append(McpCardItem(id: key, mcpServer: mcpServer, kind: .group(group, open: open)))
                guard open else { continue }
                items += group.tools.map { McpCardItem(id: "\(key)/\($0.id)", mcpServer: mcpServer, kind: .tool($0)) }
            }
            items[items.count - 1].bottom = true
            return items
        }
    }

    /// A slice's corners: the card's top ones on its header, its bottom ones on its last row.
    private static func slice(top: Bool, bottom: Bool) -> UnevenRoundedRectangle {
        let radius = Radius.lg
        return UnevenRoundedRectangle(
            topLeadingRadius: top ? radius : 0, bottomLeadingRadius: bottom ? radius : 0,
            bottomTrailingRadius: bottom ? radius : 0, topTrailingRadius: top ? radius : 0, style: .continuous)
    }

    @ViewBuilder private func item(_ item: McpCardItem) -> some View {
        let mcpServer = item.mcpServer
        switch item.kind {
        case .header:
            header(mcpServer)
        case .group(let group, let open):
            VStack(spacing: 0) {
                ThemeDivider()
                McpToolGroupRow(
                    mcpServer: mcpServer, group: group, open: open,
                    inset: EdgeInsets(top: Self.groupEdge, leading: settingsInset, bottom: Self.groupEdge, trailing: Self.groupEdge)
                ) {
                    let key = "\(mcpServer.id)/\(group.id)"
                    if open { unfolded.remove(key) } else { unfolded.insert(key) }
                }
            }
            .opacity(.disabled, when: !mcpServer.enabled)
        case .tool(let tool):
            VStack(spacing: 0) {
                ThemeDivider()
                McpServerToolRow(mcpServer: mcpServer, tool: tool)
                    .padding(.leading, Self.toolIndent)
            }
            .opacity(.disabled, when: !mcpServer.enabled)
        }
    }

    private func header(_ mcpServer: McpServerRow) -> some View {
        SettingsRow {
            HStack(spacing: 10) {
                McpServerTile(monogram: mcpServer.monogram, tint: mcpServer.tint)
                VStack(alignment: .leading, spacing: 2) {
                    Text(mcpServer.name)
                        .font(.ui(.sm, weight: .medium))
                        .foregroundStyle(Color.themeForeground)
                    Text(mcpServer.statusText)
                        .font(.ui(.xs))
                        .foregroundStyle(mcpServer.troubled ? Color.themeWarning : Color.themeMutedForeground)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .padding(.vertical, 4)
            .opacity(.disabled, when: !mcpServer.enabled)
        } trailing: {
            if mcpServer.state == .signInAgain {
                ActionButton("Sign In Again", variant: .outline, size: .large, pending: store.mcpServerSigningIn == mcpServer.id) {
                    store.signMcpServerInAgain(mcpServer)
                }
            }
            Switch(isOn: Binding { mcpServer.enabled } set: { store.setMcpServer(mcpServer, enabled: $0) })
            ActionMenu(icon: .ellipsis, help: "What to do with \(mcpServer.name)", size: .large) {
                McpServerActions(
                    mcpServer: mcpServer,
                    tryTool: { sheet = McpServerSheet(kind: .tryTool, mcpServer: mcpServer) },
                    rename: { sheet = McpServerSheet(kind: .rename, mcpServer: mcpServer) },
                    changeKey: { sheet = McpServerSheet(kind: .key, mcpServer: mcpServer) },
                    remove: { removal = .mcpServer(mcpServer) }
                )
            }
        }
    }

    /// The room between one MCP server's card and the next.
    private static let cardGap: CGFloat = 12
    /// How far a group's tools stand in from its title, under it past its chevron.
    private static let toolIndent: CGFloat = 20
    /// How far a group's permission stands from its row's top, bottom and right, in a row as tall as the others.
    private static let groupEdge = (settingsRowHeight - ControlSize.regular.height) / 2

    /// The calls agents made, newest first, all of them or only those that failed.
    private var activity: some View {
        SettingsGroup("mcp-activity", "Activity", carded: false) {
            VStack(spacing: 0) {
                Segmented([("All", false), ("Errors", true)], selection: $calls.errorsOnly)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, settingsInset)
                    .padding(.vertical, 10)
                if let empty = calls.emptyText {
                    ThemeDivider()
                    Text(empty)
                        .font(.ui(.xs))
                        .foregroundStyle(Color.themeMutedForeground)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, settingsInset)
                        .padding(.vertical, 14)
                }
                FadingScroll(maxHeight: settingsListHeight) {
                    callRows
                }
            }
            .clipShape(RoundedRectangle(cornerRadius: Radius.lg, style: .continuous))
            .card()
        }
    }

    private var callRows: some View {
        LazyVStack(spacing: 0) {
            ForEach(calls.rows) { row in
                ThemeDivider()
                Button { call = McpServerCallEntry(id: row.id) } label: {
                    McpServerCallRowView(row: row)
                        .padding(.horizontal, settingsInset)
                        .padding(.vertical, 10)
                }
                .buttonStyle(.highlight(radius: 0))
            }
            if calls.more || (calls.loading && calls.rows.isEmpty) {
                ThemeDivider()
                Spinner()
                    .foregroundStyle(Color.themeMutedForeground)
                    .frame(maxWidth: .infinity, minHeight: 44)
                    .task(id: calls.rows.count) { calls.loadMore() }
            }
        }
    }
}

/// A slice of an MCP server's card, which the page's lazy list builds only while it is on screen.
private struct McpCardItem: Identifiable {
    enum Kind {
        case header
        case group(McpToolGroup, open: Bool)
        case tool(McpServerTool)
    }

    let id: String
    let mcpServer: McpServerRow
    let kind: Kind
    var bottom = false

    var top: Bool {
        guard case .header = kind else { return false }
        return true
    }
}
#endif
