#if os(iOS)
import SwiftUI

/// Settings → MCP Servers: the MCPs the account's gateway signs in to, a group each with its tools,
/// and the way to the calls agents made to them. The core follows the gateway while the page shows.
struct McpGatewaySettings: View {
    @Environment(AppStore.self) private var store
    @State private var adding = false
    @State private var sheet: McpServerSheet?
    @State private var removal: Removal?
    @State private var unfolded: Set<String> = []
    @State private var search = ""
    @State private var showsActivity = false

    var body: some View {
        let gateway = store.gateway
        ScrollViewReader { scroll in
            SettingsList {
                GatewayHeader(caption: GatewayText.mcpCaption)
                if gateway.exists {
                    header(gateway)
                }
                if gateway.exists, let mcpServers = gateway.mcpServers, !mcpServers.isEmpty {
                    ForEach(mcpServers) { mcpServer in
                        group(mcpServer)
                    }
                    Section {
                        Button { showsActivity = true } label: {
                            HStack {
                                Text("Activity")
                                    .foregroundStyle(Color.themeForeground)
                                Spacer()
                                Image(.chevronRight, size: .md)
                                    .foregroundStyle(Color.themeMutedStrongerForeground)
                            }
                        }
                    }
                    .id("mcp-activity")
                }
            }
            .onChange(of: store.settingsTarget, initial: true) {
                guard let target = store.settingsTarget else { return }
                withAnimation { scroll.scrollTo(target, anchor: .center) }
                store.settingsTarget = nil
            }
        }
        .navigationDestination(isPresented: $showsActivity) { McpServerActivity().glassBackButton() }
        .onAppear { store.watchGateway(true) }
        .onDisappear { store.watchGateway(false) }
        .onChange(of: store.mcpStep) { follow(store.mcpStep) }
        .confirmsRemoval($removal)
        .modal(isPresented: $adding) { McpServerAddSheet(search: search) }
        .modal(item: $sheet) { sheet in
            switch sheet.kind {
            case .tryTool: McpServerTrySheet(mcpServer: sheet.mcpServer)
            case .rename: McpServerRenameSheet(mcpServer: sheet.mcpServer)
            case .key: McpServerKeySheet(mcpServer: sheet.mcpServer)
            }
        }
    }

    /// Does what `do.sh` asked, as a finger would.
    private func follow(_ step: String?) {
        guard let step, step != "call" else { return }
        store.mcpStep = nil
        let words = step.split(separator: " ", maxSplits: 1).map(String.init)
        let named = words.count > 1 ? store.gateway.mcpServers?.first { $0.name == words[1] } : nil
        switch words.first {
        case "add":
            search = words.count > 1 ? words[1] : ""
            adding = true
        case "activity": showsActivity = true
        case "unfold": if let named { unfolded.insert(named.id) }
        case "try": sheet = named.map { McpServerSheet(kind: .tryTool, mcpServer: $0) }
        case "rename": sheet = named.map { McpServerSheet(kind: .rename, mcpServer: $0) }
        case "key": sheet = named.map { McpServerSheet(kind: .key, mcpServer: $0) }
        case "remove": removal = named.map { .mcpServer($0) }
        default: break
        }
    }

    /// The way to add an MCP server, or why there can't be any yet.
    @ViewBuilder private func header(_ gateway: GatewayState) -> some View {
        Section {
            if let mcpServers = gateway.mcpServers {
                if mcpServers.isEmpty {
                    note(McpServerText.none, McpServerText.noneDetail)
                }
                Button { adding = true } label: {
                    SettingsActionLabel("Add MCP Server", symbol: .plus)
                }
            } else if gateway.connection == .connected {
                note(McpServerText.updateGateway, gateway.version.map { "Your gateway runs version \($0)" })
            } else {
                note(gateway.statusText, gateway.error)
            }
        } header: {
            Text("MCP Servers")
        }
        .id("mcp-servers")
    }

    private func note(_ title: String, _ detail: String?) -> some View {
        HStack(spacing: 12) {
            SettingsTile(symbol: .plug)
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .foregroundStyle(Color.themeForeground)
                if let detail {
                    Text(detail)
                        .font(.footnote)
                        .foregroundStyle(Color.themeMutedForeground)
                }
            }
        }
        .padding(.vertical, 2)
    }

    @ViewBuilder private func group(_ mcpServer: McpServerRow) -> some View {
        let open = unfolded.contains(mcpServer.id)
        Section {
            HStack(spacing: 12) {
                McpServerTile(monogram: mcpServer.monogram, tint: mcpServer.tint, size: SettingsTile.size)
                VStack(alignment: .leading, spacing: 2) {
                    Text(mcpServer.name)
                        .foregroundStyle(Color.themeForeground)
                    Text(mcpServer.statusText)
                        .font(.footnote)
                        .foregroundStyle(mcpServer.troubled ? Color.themeWarning : Color.themeMutedForeground)
                }
                .opacity(.disabled, when: !mcpServer.enabled)
                Spacer(minLength: 8)
                ActionMenu(icon: .ellipsis, help: "What to do with \(mcpServer.name)") {
                    McpServerActions(
                        mcpServer: mcpServer,
                        tryTool: { sheet = McpServerSheet(kind: .tryTool, mcpServer: mcpServer) },
                        rename: { sheet = McpServerSheet(kind: .rename, mcpServer: mcpServer) },
                        changeKey: { sheet = McpServerSheet(kind: .key, mcpServer: mcpServer) },
                        remove: { removal = .mcpServer(mcpServer) }
                    )
                }
            }
            .padding(.vertical, 2)
            .alignmentGuide(.listRowSeparatorLeading) { $0[.leading] }
            HStack {
                Text("Agents get it")
                    .foregroundStyle(Color.themeForeground)
                Spacer()
                Switch(isOn: Binding { mcpServer.enabled } set: { store.setMcpServer(mcpServer, enabled: $0) })
            }
            .alignmentGuide(.listRowSeparatorLeading) { $0[.leading] }
            if mcpServer.state == .signInAgain {
                Button { store.signMcpServerInAgain(mcpServer) } label: {
                    HStack {
                        SettingsActionLabel("Sign In Again", symbol: .circleUser)
                        Spacer()
                        if store.mcpServerSigningIn == mcpServer.id { Spinner(size: .xl) }
                    }
                }
                .disabled(store.mcpServerSigningIn != nil)
            }
            if !mcpServer.tools.isEmpty {
                Button {
                    withAnimation { if open { unfolded.remove(mcpServer.id) } else { unfolded.insert(mcpServer.id) } }
                } label: {
                    HStack {
                        Text(mcpServer.toolsText)
                            .foregroundStyle(Color.themeForeground)
                        Spacer()
                        if open {
                            McpServerToolsActions(mcpServer: mcpServer)
                        }
                        Image(open ? .chevronDown : .chevronRight, size: .md)
                            .foregroundStyle(Color.themeMutedForeground)
                    }
                }
                if open {
                    ForEach(mcpServer.tools) { tool in
                        McpServerToolRow(mcpServer: mcpServer, tool: tool)
                            .opacity(.disabled, when: !mcpServer.enabled)
                    }
                }
            }
        } footer: {
            if open {
                Text(mcpServer.example)
                    .font(.footnote.monospaced())
                    .foregroundStyle(Color.themeMutedForeground)
            }
        }
        .id("mcp-server/\(mcpServer.id)")
    }
}

/// The calls agents made to the MCP servers, newest first, all of them or only those that failed.
private struct McpServerActivity: View {
    @Environment(AppStore.self) private var store
    @State private var calls = McpServerCalls()
    @State private var call: McpServerCallEntry?

    var body: some View {
        SettingsList {
            Section {
                if let empty = calls.emptyText {
                    Text(empty)
                        .foregroundStyle(Color.themeMutedForeground)
                }
                ForEach(calls.rows) { row in
                    Button { call = McpServerCallEntry(id: row.id) } label: { McpServerCallRowView(row: row) }
                }
                if calls.more || (calls.loading && calls.rows.isEmpty) {
                    Spinner(size: .xl)
                        .foregroundStyle(Color.themeMutedForeground)
                        .frame(maxWidth: .infinity)
                        .task(id: calls.rows.count) { calls.loadMore() }
                }
            } header: {
                Segmented([("All", false), ("Errors", true)], selection: $calls.errorsOnly, fills: true)
                    .textCase(nil)
                    .listRowInsets(EdgeInsets(top: 0, leading: 0, bottom: 12, trailing: 0))
            }
        }
        .refreshable { calls.refresh() }
        .navigationTitle("Activity")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear {
            store.watchGateway(true)
            calls.start(store)
        }
        .onDisappear { store.watchGateway(false) }
        .onChange(of: store.gateway.callsHead) { calls.refresh() }
        .onChange(of: store.mcpStep) {
            guard store.mcpStep == "call", let first = calls.rows.first else { return }
            store.mcpStep = nil
            call = McpServerCallEntry(id: first.id)
        }
        .modal(item: $call) { McpServerCallSheet(entry: $0.id) }
    }
}
#endif
