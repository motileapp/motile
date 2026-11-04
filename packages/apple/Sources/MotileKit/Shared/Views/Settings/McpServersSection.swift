import SwiftUI

/// What the MCP Servers page says, on both clients.
enum McpServerText {
    static let updateGateway = "Update your gateway to use MCP servers"
    static let search = "Search, or paste an MCP address"
    static let runsForReal = "It changes things for real, as an agent's call would."
    static let none = "No MCP servers yet"
    static let noneDetail = "Add Linear, GitHub, Notion or any MCP by its address."
}

/// An MCP server's picture: its first letter on a wash of its colour.
struct McpServerTile: View {
    let monogram: String
    let tint: String
    var size: CGFloat = scaled(28)

    var body: some View {
        let color = Color.mcpServerTint(tint)
        Text(monogram)
            .font(.ui(size: size * 0.5, weight: .semibold))
            .foregroundStyle(color)
            .frame(width: size, height: size)
            .background(color.wash(), in: RoundedRectangle(cornerRadius: size <= scaled(18) ? Radius.xs : Radius.sm, style: .continuous))
    }
}

/// What agents may do with a group of tools, or one: a menu of the three, "Custom" while a group's
/// tools differ. `wordless`, it shows only the permission's symbol.
struct McpPermissionMenu: View {
    let permission: McpToolPermission?
    var wordless = false
    var size: ControlSize = .regular
    let pick: (McpToolPermission) -> Void

    var body: some View {
        let title = permission?.title ?? "Custom"
        ActionMenu(wordless ? nil : title, icon: permission?.symbol ?? .ellipsis, help: title, variant: .outline, size: size) {
            ForEach(McpToolPermission.allCases) { option in
                Toggle(isOn: Binding { permission == option } set: { _ in pick(option) }) {
                    Label(option.title, symbol: option.symbol)
                }
            }
        }
    }
}

/// A group of an MCP server's tools: its title and how many, which fold it, and its permission.
struct McpToolGroupRow: View {
    @Environment(AppStore.self) private var store
    let mcpServer: McpServerRow
    let group: McpToolGroup
    /// None where the group doesn't fold, as in the add sheet.
    var open: Bool?
    /// How far its content stands in from the edges its light reaches.
    var inset = EdgeInsets()
    var toggle: () -> Void = {}
    @State private var hovered = false

    var body: some View {
        let row = HStack(spacing: 8) {
            title(open: open)
                .frame(maxWidth: .infinity, alignment: .leading)
                .allowsHitTesting(false)
            McpPermissionMenu(permission: group.permission) { store.setMcpServerGroup(mcpServer, group, to: $0) }
        }
        .padding(inset)
        if open == nil {
            row
        } else {
            row
                .background {
                    Button(action: toggle) { Color.clear }
                        .buttonStyle(.highlight(radius: 0, hovered: hovered))
                        .accessibilityLabel(group.title)
                }
                .onHover { hovered = $0 }
        }
    }

    private func title(open: Bool?) -> some View {
        HStack(spacing: 8) {
            if let open {
                Image(open ? .chevronDown : .chevronRight, size: .sm)
                    .foregroundStyle(Color.themeMutedForeground)
            }
            Text(group.title)
                .font(.ui(size: 13))
                .foregroundStyle(Color.themeForeground)
                .lineLimit(1)
            Chip("\(group.tools.count)", muted: true)
        }
    }
}

/// An MCP server's tool: what it is, what it does, and what agents may do with it.
struct McpServerToolRow: View {
    @Environment(AppStore.self) private var store
    let mcpServer: McpServerRow
    let tool: McpServerTool

    var body: some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                Text(tool.title)
                    .font(.ui(size: 13))
                    .foregroundStyle(Color.themeForeground)
                    .lineLimit(1)
                if let summary = tool.summary {
                    Text(summary)
                        .font(.ui(size: 11.5))
                        .foregroundStyle(Color.themeMutedForeground)
                        .lineLimit(1)
                }
            }
            .opacity(.disabled, when: tool.permission == .block)
            Spacer(minLength: 8)
            #if os(macOS)
            Segmented(
                symbols: McpToolPermission.allCases.map { ($0.title, $0.symbol, $0) },
                selection: Binding { tool.permission } set: { store.setMcpServerTool(mcpServer, tool, to: $0) },
                size: .small
            )
            #else
            McpPermissionMenu(permission: tool.permission, wordless: true) { store.setMcpServerTool(mcpServer, tool, to: $0) }
            #endif
        }
    }
}

/// What can be done with an MCP server, in its menu.
struct McpServerActions: View {
    @Environment(AppStore.self) private var store
    let mcpServer: McpServerRow
    let tryTool: () -> Void
    let rename: () -> Void
    let changeKey: () -> Void
    let remove: () -> Void

    var body: some View {
        if !mcpServer.tools.isEmpty {
            MenuItem("Try a Tool", symbol: .play, action: tryTool)
        }
        MenuItem("Rename", symbol: .pencil, action: rename)
        switch mcpServer.auth {
        case .signIn: MenuItem("Sign In Again", symbol: .circleUser) { store.signMcpServerInAgain(mcpServer) }
        case .key: MenuItem("Change Key", symbol: .keyRound, action: changeKey)
        case .none: EmptyView()
        }
        MenuItem("Ask for Its Tools Again", symbol: .refreshCw) { store.refreshMcpServer(mcpServer) }
        MenuItem("Copy Address", symbol: .copy) { Platform.copy(mcpServer.url) }
        Divider()
        MenuItem("Remove MCP Server", symbol: .trash2, role: .destructive, action: remove)
    }
}

/// A call an agent made: the MCP server and its tool, where it came from, how long it took, and
/// what went wrong.
struct McpServerCallRowView: View {
    let row: McpServerCallRow

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            McpServerTile(monogram: row.monogram, tint: row.tint, size: scaled(18))
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    if !row.ok {
                        Image(.circleAlert, size: .sm)
                            .foregroundStyle(Color.themeWarning)
                    }
                    Text(row.mcpServer)
                        .font(.ui(size: 13, weight: .medium))
                        .foregroundStyle(Color.themeForeground)
                    Text(row.tool)
                        .font(.ui(size: 12, design: .monospaced))
                        .foregroundStyle(Color.themeForeground)
                        .lineLimit(1)
                    Spacer(minLength: 8)
                    Text(dotted: "\(row.latency) · \(row.time)")
                        .font(.ui(size: 12))
                        .monospacedDigit()
                        .foregroundStyle(Color.themeMutedForeground)
                        .lineLimit(1)
                }
                Text(row.source)
                    .font(.ui(size: 11.5))
                    .foregroundStyle(Color.themeMutedForeground)
                    .lineLimit(1)
                if let error = row.error {
                    Text(error)
                        .font(.ui(size: 11.5))
                        .foregroundStyle(Color.themeWarning)
                        .lineLimit(2)
                }
            }
        }
        .contentShape(Rectangle())
    }
}

/// A call opened from the activity.
struct McpServerCallEntry: Identifiable {
    let id: Int
}

/// What a sheet asks of the page: an MCP server to try, rename or give a key.
struct McpServerSheet: Identifiable {
    enum Kind {
        case tryTool, rename, key
    }

    let kind: Kind
    let mcpServer: McpServerRow

    var id: String { "\(kind)/\(mcpServer.id)" }
}

/// Text that can be selected and scrolled, in the code font: a call's arguments or a tool's answer.
struct McpServerTextBox: View {
    let text: String
    var height: CGFloat? = nil

    private var content: some View {
        Text(text)
            .font(.ui(size: 12, design: .monospaced))
            .foregroundStyle(Color.themeForeground)
            .textSelection(.enabled)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .topLeading)
            .padding(10)
    }

    /// As tall as the text, up to `height`, past which it scrolls.
    var body: some View {
        ScrollView { content }
            .scrollBounceBehavior(.basedOnSize)
            .frame(maxWidth: .infinity, maxHeight: height, alignment: .topLeading)
            .fixedSize(horizontal: false, vertical: true)
        .background(Color.themeBackground, in: RoundedRectangle(cornerRadius: Radius.md, style: .continuous))
        .overlay { RoundedRectangle(cornerRadius: Radius.md, style: .continuous).strokeBorder(Color.themeBorder, lineWidth: 1) }
    }
}

/// A small heading over a part of a sheet.
private struct SheetLabel: View {
    let text: String

    init(_ text: String) {
        self.text = text
    }

    var body: some View {
        Text(text)
            .font(.ui(size: 12, weight: .medium))
            .foregroundStyle(Color.themeMutedForeground)
    }
}

/// Adds an MCP server: picks a known one or takes an address and its name, then signs it in, takes
/// its key, and shows its tools.
struct McpServerAddSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.closeModal) private var close
    @State private var adder = McpServerAdder()
    @State private var search: String
    @State private var header = ""
    @State private var key = ""
    @State private var name = ""
    @FocusState private var searchFocused: Bool
    @FocusState private var keyFocused: Bool

    init(search: String = "") {
        _search = State(initialValue: search)
    }

    /// Two columns where there is room for a host beside a host; one on a phone.
    #if os(macOS)
    private static let knownColumns = 2
    #else
    private static let knownColumns = 1
    #endif

    private var address: String? {
        let text = search.trimmingCharacters(in: .whitespacesAndNewlines)
        return (text.hasPrefix("https://") || text.hasPrefix("http://")) && text.count > "https://".count ? text : nil
    }

    private var known: [KnownMcpServer] {
        let search = search.trimmingCharacters(in: .whitespaces)
        return search.isEmpty ? store.gateway.known : store.gateway.known.filter { $0.matches(search) }
    }

    private var title: String {
        switch adder.step {
        case .pick: "Add an MCP Server"
        case .signingIn(let name): "Sign In to \(name)"
        case .key(let name, _, _): "\(name) Takes a Key"
        case .added(let id): "Connected to \(store.mcpServer(id)?.name ?? "It")"
        }
    }

    @ViewBuilder private var content: some View {
        switch adder.step {
        case .pick: pick
        case .signingIn: signingIn
        case .key(let name, let url, let given): keyStep(name: name, url: url, given: given)
        case .added(let id): added(id)
        }
        if let error = adder.error {
            Text(error)
                .font(.ui(size: 12))
                .foregroundStyle(Color.themeWarning)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    @ViewBuilder private var pick: some View {
        InputField(McpServerText.search, text: $search, icon: address == nil ? .search : .link, clearable: true, focus: $searchFocused)
            .autocorrectionDisabled()
            #if os(iOS)
            .textInputAutocapitalization(.never)
            .keyboardType(.URL)
            #endif
            .onSubmit(continueWithAddress)
            .onAppear { searchFocused = true }
        if let address {
            VStack(alignment: .leading, spacing: 6) {
                SheetLabel("Name")
                InputField(McpServerAdder.hostName(address), text: $name)
                    .onSubmit(continueWithAddress)
                if let slug = McpServerRow.slug(of: name) {
                    AgentName(text: "mcp__\(slug)__…")
                }
            }
            Text("Your gateway reaches it there, and signs in or asks you for a key if it needs one.")
                .font(.ui(size: 12.5))
                .foregroundStyle(Color.themeMutedForeground)
                .fixedSize(horizontal: false, vertical: true)
        } else if known.isEmpty {
            Text("Nothing known by that name. Paste its address instead.")
                .font(.ui(size: 12.5))
                .foregroundStyle(Color.themeMutedForeground)
        } else {
            LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 6), count: Self.knownColumns), spacing: 6) {
                ForEach(known) { known in
                    knownTile(known)
                }
            }
        }
    }

    private func knownTile(_ known: KnownMcpServer) -> some View {
        Button { adder.add(url: known.url, name: known.name) } label: {
            HStack(spacing: 10) {
                McpServerTile(monogram: known.monogram, tint: known.tint)
                VStack(alignment: .leading, spacing: 1) {
                    Text(known.name)
                        .font(.ui(size: 13, weight: .medium))
                        .foregroundStyle(Color.themeForeground)
                        .lineLimit(1)
                    Text(known.host)
                        .font(.ui(size: 11.5))
                        .foregroundStyle(Color.themeMutedForeground)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                Spacer(minLength: 4)
                if adder.adding == known.url {
                    Spinner(size: .md)
                        .foregroundStyle(Color.themeMutedForeground)
                }
            }
            .padding(8)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .buttonStyle(.highlight())
        .overlay { RoundedRectangle(cornerRadius: Radius.md, style: .continuous).strokeBorder(Color.themeBorder, lineWidth: 1) }
        .disabled(adder.adding != nil)
    }

    private var signingIn: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Approve Motile in the browser.")
                .font(.ui(size: 13))
                .foregroundStyle(Color.themeMutedForeground)
                .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 8) {
                Spinner(size: .md)
                Text("Waiting for you to approve it")
                    .font(.ui(size: 13))
            }
            .foregroundStyle(Color.themeMutedForeground)
        }
    }

    @ViewBuilder private func keyStep(name: String, url: String, given: String?) -> some View {
        Text("Paste a key from \(name)'s settings.")
            .font(.ui(size: 13))
            .foregroundStyle(Color.themeMutedForeground)
            .fixedSize(horizontal: false, vertical: true)
        InputField("Key", text: $key, monospaced: true, secure: true, focus: $keyFocused)
            .onSubmit { addWithKey(url: url, given: given) }
            .onAppear { keyFocused = true }
        VStack(alignment: .leading, spacing: 6) {
            SheetLabel("Header")
            InputField("Authorization", text: $header, monospaced: true)
        }
    }

    private func addWithKey(url: String, given: String?) {
        guard !key.trimmingCharacters(in: .whitespaces).isEmpty else { return }
        adder.add(url: url, name: given, header: header, key: key)
    }

    @ViewBuilder private func added(_ id: String) -> some View {
        if let mcpServer = store.mcpServer(id) {
            Text(mcpServer.toolsText)
                .font(.ui(size: 13))
                .foregroundStyle(Color.themeMutedForeground)
            if !mcpServer.groups.isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    SheetLabel("Tool permissions")
                    ForEach(mcpServer.groups) { group in
                        McpToolGroupRow(mcpServer: mcpServer, group: group)
                    }
                }
            }
        } else {
            Spinner()
                .foregroundStyle(Color.themeMutedForeground)
                .frame(maxWidth: .infinity)
        }
    }

    private func continueWithAddress() {
        guard let address else { return }
        let name = name.trimmingCharacters(in: .whitespaces)
        adder.add(url: address, name: name.isEmpty ? nil : name)
    }

    @ViewBuilder private var buttons: some View {
        switch adder.step {
        case .pick:
            ActionButton("Cancel", margin: .beside(trailing: address == nil ? 0 : 4)) { close() }
                .keyboardShortcut(.cancelAction)
            if let address {
                ActionButton("Continue", variant: .primary, pending: adder.adding == address, margin: .beside(leading: 4)) {
                    continueWithAddress()
                }
                .keyboardShortcut(.defaultAction)
            }
        case .signingIn:
            ActionButton("Back") { adder.back() }
                .keyboardShortcut(.cancelAction)
        case .key(_, let url, let given):
            ActionButton("Back", margin: .beside(trailing: 4)) { adder.back() }
                .keyboardShortcut(.cancelAction)
            ActionButton("Add", variant: .primary, pending: adder.adding != nil, margin: .beside(leading: 4)) {
                addWithKey(url: url, given: given)
            }
            .keyboardShortcut(.defaultAction)
            .disabled(key.trimmingCharacters(in: .whitespaces).isEmpty)
        case .added:
            ActionButton("Done", variant: .primary) { close() }
                .keyboardShortcut(.defaultAction)
        }
    }

    #if os(macOS)
    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(title)
                .font(.ui(size: 15, weight: .semibold))
            content
            HStack(spacing: 0) {
                Spacer()
                buttons
            }
        }
        .padding(20)
        .frame(width: 520)
        .onAppear { adder.start(store) }
    }
    #else
    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    content
                    if case .pick = adder.step, let address {
                        ActionButton("Continue", variant: .primary, pending: adder.adding == address, fills: true) {
                            continueWithAddress()
                        }
                    }
                    if case .key = adder.step {
                        HStack(spacing: 0) { Spacer(); buttons }
                    }
                    if case .added = adder.step {
                        ActionButton("Done", variant: .primary, fills: true) { close() }
                    }
                }
                .padding(20)
            }
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                GlassToolbarItem(placement: .cancellationAction) { SheetCloseButton() }
            }
        }
        .presentationDragIndicator(.visible)
        .onAppear { adder.start(store) }
    }
    #endif
}

/// How agents will name an MCP server's tools, under its name.
private struct AgentName: View {
    let text: String

    var body: some View {
        Text(text)
            .font(.ui(size: 11.5, design: .monospaced))
            .foregroundStyle(Color.themeMutedStrongerForeground)
            .lineLimit(1)
            .truncationMode(.middle)
            .textSelection(.enabled)
    }
}

/// Names an MCP server, which names it for agents too.
struct McpServerRenameSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.closeModal) private var close
    let mcpServer: McpServerRow
    @State private var name = ""
    @FocusState private var focused: Bool

    private var fields: some View {
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 6) {
                SheetLabel("Name")
                InputField(mcpServer.name, text: $name, focus: $focused)
                    .onSubmit(save)
                AgentName(text: mcpServer.example(named: name))
            }
        }
    }

    private func save() {
        store.renameMcpServer(mcpServer, name: name)
        close()
    }

    #if os(macOS)
    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Rename \(mcpServer.name)")
                .font(.ui(size: 15, weight: .semibold))
            fields
            HStack(spacing: 0) {
                Spacer()
                ActionButton("Cancel", margin: .beside(trailing: 4)) { close() }
                    .keyboardShortcut(.cancelAction)
                ActionButton("Save", variant: .primary, margin: .beside(leading: 4), action: save)
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 400)
        .onAppear(perform: start)
    }
    #else
    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    fields
                    ActionButton("Save", variant: .primary, fills: true, action: save)
                }
                .padding(20)
            }
            .navigationTitle("Rename \(mcpServer.name)")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                GlassToolbarItem(placement: .cancellationAction) { SheetCloseButton() }
            }
        }
        .presentationDragIndicator(.visible)
        .onAppear(perform: start)
    }
    #endif

    private func start() {
        name = mcpServer.name
        focused = true
    }
}

/// Gives an MCP server a new key.
struct McpServerKeySheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.closeModal) private var close
    let mcpServer: McpServerRow
    @State private var key = ""
    @State private var header = ""
    @State private var pending = false
    @State private var error: String?
    @FocusState private var focused: Bool

    private var fields: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Paste a new key from \(mcpServer.name)'s settings.")
                .font(.ui(size: 13))
                .foregroundStyle(Color.themeMutedForeground)
                .fixedSize(horizontal: false, vertical: true)
            InputField("Key", text: $key, monospaced: true, secure: true, focus: $focused)
                .onSubmit(save)
            VStack(alignment: .leading, spacing: 6) {
                SheetLabel("Header")
                InputField("Authorization", text: $header, monospaced: true)
            }
            if let error {
                Text(error)
                    .font(.ui(size: 12))
                    .foregroundStyle(Color.themeWarning)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private var empty: Bool { key.trimmingCharacters(in: .whitespaces).isEmpty }

    private func save() {
        guard !empty, !pending else { return }
        pending = true
        error = nil
        store.setMcpServerKey(mcpServer, header: header, key: key) { result in
            pending = false
            switch result {
            case .success: close()
            case .failure(let failure): error = failure.message
            }
        }
    }

    #if os(macOS)
    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("New Key for \(mcpServer.name)")
                .font(.ui(size: 15, weight: .semibold))
            fields
            HStack(spacing: 0) {
                Spacer()
                ActionButton("Cancel", margin: .beside(trailing: 4)) { close() }
                    .keyboardShortcut(.cancelAction)
                ActionButton("Save", variant: .primary, pending: pending, margin: .beside(leading: 4), action: save)
                    .keyboardShortcut(.defaultAction)
                    .disabled(empty)
            }
        }
        .padding(20)
        .frame(width: 400)
        .onAppear { focused = true }
    }
    #else
    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    fields
                    ActionButton("Save", variant: .primary, pending: pending, fills: true, action: save)
                        .disabled(empty)
                }
                .padding(20)
            }
            .navigationTitle("New Key for \(mcpServer.name)")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                GlassToolbarItem(placement: .cancellationAction) { SheetCloseButton() }
            }
        }
        .presentationDragIndicator(.visible)
        .onAppear { focused = true }
    }
    #endif
}

/// Calls one of an MCP server's tools with what its fields hold, and shows what it answered.
struct McpServerTrySheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.closeModal) private var close
    let mcpServer: McpServerRow
    @State private var tool = ""
    @State private var fields: Loaded<[McpServerField]> = .loading
    @State private var values: [String: String] = [:]
    @State private var running = false
    @State private var answer: McpServerAnswer?
    @State private var error: String?
    @State private var showsJSON = false

    private var picked: McpServerTool? {
        mcpServer.tools.first { $0.name == tool }
    }

    private func pick(_ name: String) {
        tool = name
        answer = nil
        error = nil
        fields = .loading
        store.loadMcpServerFields(mcpServer.id, tool: name) { result in
            guard tool == name else { return }
            switch result {
            case .success(let made):
                values = Dictionary(uniqueKeysWithValues: made.map { ($0.name, $0.value) })
                fields = .ready(made)
            case .failure(let failure):
                fields = .failed(failure.message)
            }
        }
    }

    private func run() {
        guard !running, !tool.isEmpty else { return }
        running = true
        error = nil
        store.tryMcpServerTool(mcpServer.id, tool: tool, values: values) { result in
            running = false
            switch result {
            case .success(let answered):
                answer = answered
                error = answered.error
            case .failure(let failure):
                error = failure.message
            }
        }
    }

    private func binding(_ name: String) -> Binding<String> {
        Binding { values[name] ?? "" } set: { values[name] = $0 }
    }

    private var toolMenu: some View {
        ActionMenu(picked?.title ?? "Pick a Tool", variant: .outline) {
            ForEach(mcpServer.tools) { tool in
                Button(tool.title) { pick(tool.name) }
            }
        }
    }

    @ViewBuilder private func field(_ field: McpServerField) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 4) {
                Text(field.name)
                    .font(.ui(size: 12, weight: .medium, design: .monospaced))
                    .foregroundStyle(Color.themeForeground)
                if field.required {
                    Text("required")
                        .font(.ui(size: 11.5))
                        .foregroundStyle(Color.themeMutedStrongerForeground)
                }
                Spacer()
                if field.kind == .toggle {
                    Switch(isOn: Binding { values[field.name] == "true" } set: { values[field.name] = $0 ? "true" : "false" })
                }
            }
            if let description = field.description {
                Text(description)
                    .font(.ui(size: 11.5))
                    .foregroundStyle(Color.themeMutedForeground)
                    .fixedSize(horizontal: false, vertical: true)
            }
            switch field.kind {
            case .text: InputField(field.name, text: binding(field.name))
            case .number: InputField("0", text: binding(field.name), numeric: true)
            case .json: TextArea("{}", text: binding(field.name), monospaced: true, lines: 2, maxLines: 8)
            case .choice:
                ActionMenu(values[field.name].flatMap { $0.isEmpty ? nil : $0 } ?? "Pick One", variant: .outline) {
                    Button("None") { values[field.name] = "" }
                    ForEach(field.choices, id: \.self) { choice in
                        Button(choice) { values[field.name] = choice }
                    }
                }
            case .toggle: EmptyView()
            }
        }
    }

    @ViewBuilder private var form: some View {
        if let summary = picked?.summary {
            Text(summary)
                .font(.ui(size: 12.5))
                .foregroundStyle(Color.themeMutedForeground)
                .fixedSize(horizontal: false, vertical: true)
        }
        switch fields {
        case .loading:
            if !tool.isEmpty {
                Spinner().foregroundStyle(Color.themeMutedForeground).frame(maxWidth: .infinity)
            }
        case .failed(let message):
            Text(message).font(.ui(size: 12)).foregroundStyle(Color.themeWarning)
        case .ready(let made):
            ForEach(made) { self.field($0) }
        }
        if let picked, picked.kind != .read {
            HStack(spacing: 6) {
                Image(.triangleAlert, size: .sm)
                Text(McpServerText.runsForReal)
                    .font(.ui(size: 12))
            }
            .foregroundStyle(Color.themeWarning)
        }
        if let error {
            Text(error)
                .font(.ui(size: 12))
                .foregroundStyle(Color.themeWarning)
                .fixedSize(horizontal: false, vertical: true)
        }
        if let answer {
            HStack {
                SheetLabel("Answer")
                Spacer()
                Segmented([("Text", false), ("JSON", true)], selection: $showsJSON, size: .small)
            }
            McpServerTextBox(text: showsJSON ? answer.json : answer.text, height: scaled(240))
        }
    }

    #if os(macOS)
    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Text("Try a Tool")
                    .font(.ui(size: 15, weight: .semibold))
                Spacer()
                toolMenu
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    form
                }
                .padding(.vertical, 2)
            }
            .frame(maxHeight: 520)
            .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 0) {
                Spacer()
                ActionButton("Done", margin: .beside(trailing: 4)) { close() }
                    .keyboardShortcut(.cancelAction)
                ActionButton("Run", icon: .play, variant: .primary, pending: running, margin: .beside(leading: 4), action: run)
                    .keyboardShortcut(.defaultAction)
                    .disabled(tool.isEmpty)
            }
        }
        .padding(20)
        .frame(width: 560)
        .onAppear { if let first = mcpServer.tools.first { pick(first.name) } }
    }
    #else
    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    toolMenu
                    form
                    ActionButton("Run", icon: .play, variant: .primary, pending: running, fills: true, action: run)
                        .disabled(tool.isEmpty)
                }
                .padding(20)
            }
            .navigationTitle("Try a Tool")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                GlassToolbarItem(placement: .cancellationAction) { SheetCloseButton() }
            }
        }
        .presentationDragIndicator(.visible)
        .onAppear { if let first = mcpServer.tools.first { pick(first.name) } }
    }
    #endif
}

/// A call in full: what the agent sent and what came back.
struct McpServerCallSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.closeModal) private var close
    let entry: Int
    @State private var call: Loaded<McpServerCallView?> = .loading

    @ViewBuilder private var content: some View {
        switch call {
        case .loading:
            Spinner()
                .foregroundStyle(Color.themeMutedForeground)
                .frame(maxWidth: .infinity, minHeight: 120)
        case .failed(let message):
            Text(message).font(.ui(size: 13)).foregroundStyle(Color.themeDestructive)
        case .ready(nil):
            Text("Your gateway no longer has this call")
                .font(.ui(size: 13))
                .foregroundStyle(Color.themeMutedForeground)
        case .ready(let call?):
            Text(dotted: call.row.summary)
                .font(.ui(size: 12))
                .foregroundStyle(Color.themeMutedForeground)
                .lineLimit(2)
            SheetLabel("Arguments")
            McpServerTextBox(text: call.arguments, height: scaled(180))
            if let error = call.error {
                SheetLabel("Error")
                Text(error)
                    .font(.ui(size: 12.5))
                    .foregroundStyle(Color.themeWarning)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
            }
            SheetLabel("Result")
            McpServerTextBox(text: call.result, height: scaled(280))
        }
    }

    private func load() {
        store.loadMcpServerCall(entry) { result in
            switch result {
            case .success(let loaded): call = .ready(loaded)
            case .failure(let failure): call = .failed(failure.message)
            }
        }
    }

    #if os(macOS)
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Tool Call")
                .font(.ui(size: 15, weight: .semibold))
            content
            HStack {
                Spacer()
                ActionButton("Done", variant: .primary) { close() }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 640)
        .onAppear(perform: load)
    }
    #else
    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    content
                }
                .padding(20)
            }
            .navigationTitle("Tool Call")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                GlassToolbarItem(placement: .cancellationAction) { SheetCloseButton() }
            }
        }
        .presentationDragIndicator(.visible)
        .onAppear(perform: load)
    }
    #endif
}
