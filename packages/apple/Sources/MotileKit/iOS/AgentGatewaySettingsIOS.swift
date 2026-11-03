#if os(iOS)
import SwiftUI

/// Settings → Agent Gateway: the account's gateway, the accounts it shares with every server with their
/// limits, and the way to its requests. The core follows the gateway while the page shows.
struct AgentGatewaySettings: View {
    /// How far an inset group's rows stand from the list's sides.
    private static let listInset: CGFloat = 20

    @Environment(AppStore.self) private var store
    @State private var renamed: GatewayAccount?
    @State private var name = ""
    @State private var removal: Removal?
    @State private var width: CGFloat = 0

    var body: some View {
        let gateway = store.gateway
        ScrollViewReader { scroll in
            SettingsList {
                GatewayHeader(caption: GatewayText.agentCaption)
                if gateway.exists {
                    ForEach(Agent.allCases, id: \.self) { agent in
                        accounts(of: agent, in: gateway)
                    }
                    requests(gateway)
                }
            }
            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
            .onChange(of: store.settingsTarget, initial: true) {
                guard let target = store.settingsTarget else { return }
                withAnimation { scroll.scrollTo(target, anchor: .center) }
                store.settingsTarget = nil
            }
        }
        .onAppear { store.watchGateway(true) }
        .onDisappear { store.watchGateway(false) }
        .confirmsRemoval($removal)
        .modal(item: signIn) { _ in GatewaySignInSheet() }
        .alert("Rename Account", isPresented: renaming, presenting: renamed) { account in
            TextField("Work", text: $name)
            Button("Cancel", role: .cancel) {}
            Button("Save") {
                let trimmed = name.trimmingCharacters(in: .whitespaces)
                guard !trimmed.isEmpty else { return }
                store.renameGatewayAccount(account, to: trimmed)
            }
        }
    }

    /// The sign-in under way, whose sheet drops it when it closes.
    private var signIn: Binding<GatewaySignIn?> {
        Binding { store.gateway.signIn } set: { if $0 == nil { store.cancelGatewaySignIn() } }
    }

    private var renaming: Binding<Bool> {
        Binding { renamed != nil } set: { if !$0 { renamed = nil } }
    }

    private func rename(_ account: GatewayAccount) {
        name = account.name
        renamed = account
    }

    /// The way to add an agent's account, and its accounts, a group each with its limits.
    @ViewBuilder private func accounts(of agent: Agent, in gateway: GatewayState) -> some View {
        let accounts = gateway.accounts(of: agent)
        Section {
            Button { store.startGatewaySignIn(agent) } label: {
                HStack {
                    SettingsActionLabel(GatewayText.addAccount(of: agent), symbol: .plus)
                    Spacer()
                    if store.gatewaySignInStarting == agent { Spinner(size: .xl) }
                }
            }
            .disabled(store.gatewaySignInStarting != nil)
        } header: {
            Text(GatewayText.accounts(of: agent))
        }
        .id(GatewayState.accountsID(of: agent))
        ForEach(accounts) { account in
            Section {
                rows(of: account, count: accounts.count)
            }
            .id("gateway-account/\(account.id)")
        }
    }

    @ViewBuilder private func rows(of account: GatewayAccount, count: Int) -> some View {
        HStack(spacing: 10) {
            GatewayAccountLabel(account: account)
                .frame(maxWidth: .infinity, alignment: .leading)
            ActionMenu(icon: .ellipsis, help: "What to do with \(account.name)") {
                GatewayAccountActions(account: account, count: count, rename: { rename(account) }, remove: { removal = .gatewayAccount(account) })
            }
        }
        .padding(.vertical, 2)
        .alignmentGuide(.listRowSeparatorLeading) { $0[.leading] }
        // On a row of its own under the name, which a phone has no room beside.
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(GatewayText.inRotation)
                    .foregroundStyle(Color.themeForeground)
                Text(GatewayText.rotation)
                    .font(.footnote)
                    .foregroundStyle(Color.themeMutedForeground)
            }
            Spacer()
            Switch(isOn: Binding { account.enabled } set: { store.setGatewayAccount(account, enabled: $0) })
        }
        .alignmentGuide(.listRowSeparatorLeading) { $0[.leading] }
        if account.status == .signInAgain {
            Button { store.startGatewaySignIn(account.agent, again: account.id) } label: {
                HStack {
                    SettingsActionLabel("Sign In Again", symbol: .circleUser)
                    Spacer()
                    if store.gatewaySignInStarting == account.agent { Spinner(size: .xl) }
                }
            }
            .disabled(store.gatewaySignInStarting != nil)
        }
        if !account.windows.isEmpty {
            VStack(spacing: 0) {
                ForEach(account.windows) { window in
                    ThemeDivider().padding(.horizontal, LimitRow.inset)
                    LimitRow(window: window, beside: width - 2 * Self.listInset >= LimitsView.besideWidth)
                }
            }
            .opacity(.disabled, when: !account.enabled)
            .listRowInsets(EdgeInsets())
            .listRowSeparator(.hidden)
        }
        if let note = account.note {
            Text(note)
                .font(.footnote)
                .foregroundStyle(Color.themeMutedForeground)
        }
    }

    /// What is being captured, the ways to capture more, and the way to the log.
    private func requests(_ gateway: GatewayState) -> some View {
        Section {
            NavigationLink {
                GatewayRequestLog().glassBackButton()
            } label: {
                Text("Request Log")
                    .foregroundStyle(Color.themeForeground)
            }
            Button { store.setGatewayCapture(.next(GatewayText.captureNext)) } label: {
                SettingsActionLabel("Capture the Next \(GatewayText.captureNext) Requests", symbol: .circleDot)
            }
            Menu {
                GatewayCaptureThreads()
            } label: {
                SettingsActionLabel("Capture a Thread", symbol: .messageSquareText)
            }
            if gateway.capture.on {
                Button("Stop Capturing", role: .destructive) { store.setGatewayCapture(.off) }
                    .foregroundStyle(Color.themeDestructive)
            }
        } header: {
            Text("Log")
        } footer: {
            Text(gateway.capture.text)
                .foregroundStyle(Color.themeMutedForeground)
        }
        .id("gateway-log")
    }
}

/// The requests the gateway passed on, newest first, all of them or only those that failed. A
/// captured one opens in full.
private struct GatewayRequestLog: View {
    @Environment(AppStore.self) private var store
    @State private var log = GatewayLog()
    @State private var captured: GatewayCapturedEntry?

    var body: some View {
        SettingsList {
            Section {
                if let empty = log.emptyText {
                    Text(empty)
                        .foregroundStyle(Color.themeMutedForeground)
                }
                ForEach(log.rows) { row in
                    if row.captured {
                        Button { captured = GatewayCapturedEntry(id: row.id) } label: { GatewayLogRowView(row: row) }
                    } else {
                        GatewayLogRowView(row: row)
                    }
                }
                if log.more || (log.loading && log.rows.isEmpty) {
                    Spinner(size: .xl)
                        .foregroundStyle(Color.themeMutedForeground)
                        .frame(maxWidth: .infinity)
                        .task(id: log.rows.count) { log.loadMore() }
                }
            } header: {
                Segmented([("All", false), ("Errors", true)], selection: $log.errorsOnly, fills: true)
                    .textCase(nil)
                    .listRowInsets(EdgeInsets(top: 0, leading: 0, bottom: 12, trailing: 0))
            }
        }
        .refreshable { log.reload() }
        .navigationTitle("Request Log")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear {
            store.watchGateway(true)
            log.start(store)
        }
        .onDisappear { store.watchGateway(false) }
        .onChange(of: store.gateway.logHead) { log.refresh() }
        .modal(item: $captured) { GatewayCapturedSheet(entry: $0.id) }
    }
}

/// The gateway and how the client stands with it, or what one is and the way to add it, at the top
/// of both its pages.
struct GatewayHeader: View {
    @Environment(AppStore.self) private var store
    let caption: String
    @State private var adding = false
    @State private var removal: Removal?

    var body: some View {
        let gateway = store.gateway
        Section {
            if gateway.exists {
                let name = gateway.name ?? "Gateway"
                HStack(spacing: 12) {
                    SettingsTile(symbol: .waypoints)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(name)
                            .foregroundStyle(Color.themeForeground)
                        Text(gateway.statusText)
                            .font(.footnote)
                            .foregroundStyle(Color.themeMutedForeground)
                        if let error = gateway.error {
                            Text(error)
                                .font(.footnote)
                                .foregroundStyle(Color.themeWarning)
                        }
                    }
                    Spacer(minLength: 8)
                    ActionMenu(icon: .ellipsis, help: "What to do with \(name)") {
                        MenuItem("Replace with a New Gateway", symbol: .refreshCw) { adding = true }
                        Divider()
                        MenuItem("Remove Gateway", symbol: .trash2, role: .destructive) { removal = .gateway(name: name) }
                    }
                }
                .padding(.vertical, 2)
                if let version = gateway.version {
                    LabeledContent("Version", value: version)
                }
            } else {
                HStack(spacing: 12) {
                    SettingsTile(symbol: .waypoints)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("No gateway")
                            .foregroundStyle(Color.themeForeground)
                        Text(GatewayText.absent)
                            .font(.footnote)
                            .foregroundStyle(Color.themeMutedForeground)
                    }
                }
                .padding(.vertical, 2)
                Button { adding = true } label: {
                    SettingsActionLabel("Add a Gateway", symbol: .plus)
                }
            }
        } header: {
            Text("Gateway")
        } footer: {
            Text(caption)
                .foregroundStyle(Color.themeMutedForeground)
        }
        .id("gateway")
        .confirmsRemoval($removal)
        .modal(isPresented: $adding) { GatewayAddSheet(replacing: store.gateway.key) }
    }
}
#endif
