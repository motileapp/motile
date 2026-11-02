#if os(macOS)
import SwiftUI

/// Settings → Proxy: the account's proxy, the accounts it shares with every server with their
/// limits, and the requests it passed on. The core follows the proxy while the page shows.
struct ProxySettings: View {
    @Environment(AppStore.self) private var store
    @State private var adding = false
    @State private var renamed: ProxyAccount?
    @State private var removal: Removal?
    @State private var captured: ProxyCapturedEntry?
    @State private var log = ProxyLog()
    @State private var width: CGFloat = 0

    var body: some View {
        let proxy = store.proxy
        VStack(alignment: .leading, spacing: SettingsGroupTitle.groupGap) {
            header(proxy)
            if proxy.exists {
                ForEach(Agent.allCases, id: \.self) { agent in
                    accounts(of: agent, in: proxy)
                }
                requests(proxy)
            }
        }
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
        .onAppear { store.watchProxy(true) }
        .onDisappear { store.watchProxy(false) }
        .onChange(of: proxy.exists, initial: true) {
            guard proxy.exists else { return }
            log.start(store)
        }
        .onChange(of: proxy.logHead) { log.refresh() }
        .confirmsRemoval($removal)
        .modal(isPresented: $adding) { ProxyAddSheet(replacing: store.proxy.key) }
        .modal(item: signIn) { _ in ProxySignInSheet() }
        .modal(item: $renamed) { ProxyRenameSheet(account: $0) }
        .modal(item: $captured) { ProxyCapturedSheet(entry: $0.id) }
    }

    /// The sign-in under way, whose sheet drops it when it closes.
    private var signIn: Binding<ProxySignIn?> {
        Binding { store.proxy.signIn } set: { if $0 == nil { store.cancelProxySignIn() } }
    }

    /// The proxy and how the client stands with it, or what one is and the way to add it.
    private func header(_ proxy: ProxyState) -> some View {
        SettingsGroup("proxy", "Proxy") {
            if proxy.exists {
                let name = proxy.name ?? "Proxy"
                SettingsRow {
                    SettingsLabel(name, description: [proxy.statusText, proxy.version.map { "version \($0)" }].compactMap { $0 }.joined(separator: " · "), icon: .waypoints) {
                        if let error = proxy.error {
                            Text(error)
                                .font(.ui(size: 11.5))
                                .foregroundStyle(Color.themeWarning)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                } trailing: {
                    ActionMenu(icon: .ellipsis, help: "What to do with \(name)", size: .large) {
                        ProxyMenuItem("Replace with a New Proxy", symbol: .refreshCw) { adding = true }
                        Divider()
                        ProxyMenuItem("Remove Proxy", symbol: .trash2, role: .destructive) { removal = .proxy(name: name) }
                    }
                }
            } else {
                SettingsRow {
                    SettingsLabel("No proxy", description: ProxyText.absent, icon: .waypoints)
                } trailing: {
                    ActionButton("Add a Proxy", icon: .plus, variant: .outline, size: .large) { adding = true }
                }
            }
        }
    }

    /// An agent's accounts, a card each with its limits, and the way to add another.
    private func accounts(of agent: Agent, in proxy: ProxyState) -> some View {
        let accounts = proxy.accounts(of: agent)
        return SettingsGroup(ProxyState.accountsID(of: agent), ProxyText.accounts(of: agent), carded: false) {
            ForEach(accounts) { account in
                card(account, count: accounts.count)
            }
            ActionButton(ProxyText.addAccount(of: agent), icon: .plus, variant: .outline, size: .large, pending: store.proxySignInStarting == agent) {
                store.startProxySignIn(agent)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func card(_ account: ProxyAccount, count: Int) -> some View {
        VStack(spacing: 0) {
            SettingsRow {
                ProxyAccountLabel(account: account)
            } trailing: {
                if account.status == .signInAgain {
                    ActionButton("Sign In Again", variant: .outline, size: .large, pending: store.proxySignInStarting == account.agent) {
                        store.startProxySignIn(account.agent)
                    }
                }
                Switch(isOn: Binding { account.enabled } set: { store.setProxyAccount(account, enabled: $0) })
                ActionMenu(icon: .ellipsis, help: "What to do with \(account.name)", size: .large) {
                    ProxyAccountActions(account: account, count: count, rename: { renamed = account }, remove: { removal = .proxyAccount(account) })
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
    private func requests(_ proxy: ProxyState) -> some View {
        SettingsGroup("proxy-log", "Log", carded: false) {
            SettingsRow {
                SettingsLabel(proxy.capture.text)
            } trailing: {
                if proxy.capture.on {
                    ActionButton("Stop", variant: .outline, size: .large) { store.setProxyCapture(.off) }
                }
                ActionMenu("Capture", variant: .outline, size: .large) {
                    Button("Capture the Next \(ProxyText.captureNext) Requests") { store.setProxyCapture(.next(ProxyText.captureNext)) }
                    Menu("Capture a Thread") { ProxyCaptureThreads() }
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
        .card()
    }

    @ViewBuilder private func logRow(_ row: ProxyLogRow) -> some View {
        let shown = ProxyLogRowView(row: row)
            .padding(.horizontal, settingsInset)
            .padding(.vertical, 10)
        if row.captured {
            Button { captured = ProxyCapturedEntry(id: row.id) } label: { shown }
                .buttonStyle(.highlight(radius: 0))
        } else {
            shown
        }
    }
}

/// An account's new name.
private struct ProxyRenameSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.closeModal) private var close
    let account: ProxyAccount
    @State private var name = ""
    @FocusState private var focused: Bool

    private var trimmed: String { name.trimmingCharacters(in: .whitespaces) }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Rename Account")
                .font(.ui(size: 15, weight: .semibold))
            InputField("Work", text: $name, focus: $focused)
                .onSubmit(save)
            HStack(spacing: 8) {
                Spacer()
                ActionButton("Cancel") { close() }
                    .keyboardShortcut(.cancelAction)
                ActionButton("Save", variant: .primary, action: save)
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
        store.renameProxyAccount(account, to: trimmed)
        close()
    }
}
#endif
