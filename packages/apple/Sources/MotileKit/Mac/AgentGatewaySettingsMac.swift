#if os(macOS)
import SwiftUI

/// Settings → Agent Gateway: the account's gateway, the accounts it shares with every server with
/// their limits, and the requests it passed on. The core follows the gateway while the page shows.
struct AgentGatewaySettings: View {
    @Environment(AppStore.self) private var store
    @State private var renamed: GatewayAccount?
    @State private var removal: Removal?
    @State private var captured: GatewayCapturedEntry?
    @State private var log = GatewayLog()
    @State private var width: CGFloat = 0

    var body: some View {
        let gateway = store.gateway
        VStack(alignment: .leading, spacing: SettingsGroupTitle.groupGap) {
            GatewayHeader(caption: GatewayText.agentCaption)
            if gateway.exists {
                ForEach(Agent.allCases, id: \.self) { agent in
                    accounts(of: agent, in: gateway)
                }
                requests(gateway)
            }
        }
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
        .onAppear { store.watchGateway(true) }
        .onDisappear { store.watchGateway(false) }
        .onChange(of: gateway.exists, initial: true) {
            guard gateway.exists else { return }
            log.start(store)
        }
        .onChange(of: gateway.logHead) { log.refresh() }
        .confirmsRemoval($removal)
        .modal(item: signIn) { _ in GatewaySignInSheet() }
        .modal(item: $renamed) { GatewayRenameSheet(account: $0) }
        .modal(item: $captured) { GatewayCapturedSheet(entry: $0.id) }
    }

    /// The sign-in under way, whose sheet drops it when it closes.
    private var signIn: Binding<GatewaySignIn?> {
        Binding { store.gateway.signIn } set: { if $0 == nil { store.cancelGatewaySignIn() } }
    }

    /// The way to add an agent's account, and its accounts, a card each with its limits.
    private func accounts(of agent: Agent, in gateway: GatewayState) -> some View {
        let accounts = gateway.accounts(of: agent)
        return SettingsGroup(GatewayState.accountsID(of: agent), GatewayText.accounts(of: agent), carded: false) {
            ActionButton(GatewayText.addAccount(of: agent), icon: .plus, variant: .outline, size: .large, pending: store.gatewaySignInStarting == agent) {
                store.startGatewaySignIn(agent)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            ForEach(accounts) { account in
                card(account, count: accounts.count)
            }
        }
    }

    private func card(_ account: GatewayAccount, count: Int) -> some View {
        VStack(spacing: 0) {
            SettingsRow {
                // As far from the card's top as its limits' numbers are from theirs.
                GatewayAccountLabel(account: account)
                    .padding(.vertical, 6)
            } trailing: {
                if account.status == .signInAgain {
                    ActionButton("Sign In Again", variant: .outline, size: .large, pending: store.gatewaySignInStarting == account.agent) {
                        store.startGatewaySignIn(account.agent, again: account.id)
                    }
                }
                Switch(isOn: Binding { account.enabled } set: { store.setGatewayAccount(account, enabled: $0) })
                ActionMenu(icon: .ellipsis, help: "What to do with \(account.name)", size: .large) {
                    GatewayAccountActions(account: account, count: count, rename: { renamed = account }, remove: { removal = .gatewayAccount(account) })
                }
            }
            ForEach(account.windows) { window in
                ThemeDivider()
                LimitRow(window: window, beside: width >= LimitsView.besideWidth)
                    .opacity(.disabled, when: !account.enabled)
            }
            if let note = account.note {
                ThemeDivider()
                Text(note)
                    .font(.ui(size: 12))
                    .foregroundStyle(Color.themeMutedForeground)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, settingsInset)
                    .padding(.vertical, 12)
            }
        }
        .card()
    }

    /// What is being captured and the way to capture more, then the requests, newest first.
    private func requests(_ gateway: GatewayState) -> some View {
        SettingsGroup("gateway-log", "Log", carded: false) {
            SettingsRow {
                SettingsLabel(gateway.capture.text)
            } trailing: {
                HStack(spacing: 0) {
                    if gateway.capture.on {
                        ActionButton("Stop", variant: .outline, size: .large, margin: .beside(trailing: settingsHalfGap)) { store.setGatewayCapture(.off) }
                    }
                    ActionMenu("Capture", variant: .outline, size: .large, margin: .beside(leading: gateway.capture.on ? settingsHalfGap : 0)) {
                        Button("Capture the Next \(GatewayText.captureNext) Requests") { store.setGatewayCapture(.next(GatewayText.captureNext)) }
                        Menu("Capture a Thread") { GatewayCaptureThreads() }
                    }
                }
            }
            .card()
            logCard
        }
    }

    private var logCard: some View {
        VStack(spacing: 0) {
            Segmented([("All", false), ("Errors", true)], selection: $log.errorsOnly)
                .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, settingsInset)
            .padding(.vertical, 10)
            if let empty = log.emptyText {
                ThemeDivider()
                Text(empty)
                    .font(.ui(size: 12))
                    .foregroundStyle(Color.themeMutedForeground)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, settingsInset)
                    .padding(.vertical, 14)
            }
            FadingScroll(maxHeight: settingsListHeight) {
                LazyVStack(spacing: 0) {
                    ForEach(log.rows) { row in
                        ThemeDivider()
                        logRow(row)
                    }
                    if log.more || (log.loading && log.rows.isEmpty) {
                        ThemeDivider()
                        Spinner()
                            .foregroundStyle(Color.themeMutedForeground)
                            .frame(maxWidth: .infinity, minHeight: 44)
                            .task(id: log.rows.count) { log.loadMore() }
                    }
                }
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: Radius.lg, style: .continuous))
        .card()
    }

    @ViewBuilder private func logRow(_ row: GatewayLogRow) -> some View {
        let shown = GatewayLogRowView(row: row)
            .padding(.horizontal, settingsInset)
            .padding(.vertical, 10)
        if row.captured {
            Button { captured = GatewayCapturedEntry(id: row.id) } label: { shown }
                .buttonStyle(.highlight(radius: 0))
        } else {
            shown
        }
    }
}

/// An account's new name.
private struct GatewayRenameSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.closeModal) private var close
    let account: GatewayAccount
    @State private var name = ""
    @FocusState private var focused: Bool

    private var trimmed: String { name.trimmingCharacters(in: .whitespaces) }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Rename Account")
                .font(.ui(size: 15, weight: .semibold))
            InputField("Work", text: $name, focus: $focused)
                .onSubmit(save)
            HStack(spacing: 0) {
                Spacer()
                ActionButton("Cancel", margin: .beside(trailing: 4)) { close() }
                    .keyboardShortcut(.cancelAction)
                ActionButton("Save", variant: .primary, margin: .beside(leading: 4), action: save)
                    .keyboardShortcut(.defaultAction)
                    .disabled(trimmed.isEmpty)
            }
        }
        .padding(20)
        .frame(width: 380)
        .onAppear {
            name = account.name
            focused = true
        }
    }

    private func save() {
        guard !trimmed.isEmpty else { return }
        store.renameGatewayAccount(account, to: trimmed)
        close()
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
        SettingsGroup("gateway", "Gateway", caption: caption) {
            if gateway.exists {
                let name = gateway.name ?? "Gateway"
                SettingsRow {
                    SettingsLabel(name, description: [gateway.statusText, gateway.version.map { "version \($0)" }].compactMap { $0 }.joined(separator: " · "), icon: .waypoints) {
                        if let error = gateway.error {
                            Text(error)
                                .font(.ui(size: 11.5))
                                .foregroundStyle(Color.themeWarning)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                } trailing: {
                    ActionMenu(icon: .ellipsis, help: "What to do with \(name)", size: .large) {
                        MenuItem("Replace with a New Gateway", symbol: .refreshCw) { adding = true }
                        Divider()
                        MenuItem("Remove Gateway", symbol: .trash2, role: .destructive) { removal = .gateway(name: name) }
                    }
                }
            } else {
                SettingsRow {
                    SettingsLabel("No gateway", description: GatewayText.absent, icon: .waypoints)
                } trailing: {
                    ActionButton("Add a Gateway", icon: .plus, variant: .outline, size: .large) { adding = true }
                }
            }
        }
        .confirmsRemoval($removal)
        .modal(isPresented: $adding) { GatewayAddSheet(replacing: store.gateway.key) }
    }
}
#endif
