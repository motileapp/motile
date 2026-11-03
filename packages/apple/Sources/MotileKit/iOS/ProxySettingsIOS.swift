#if os(iOS)
import SwiftUI

/// Settings → Proxy: the account's proxy, the accounts it shares with every server with their
/// limits, and the way to its requests. The core follows the proxy while the page shows.
struct ProxySettings: View {
    /// How far an inset group's rows stand from the list's sides.
    private static let listInset: CGFloat = 20

    @Environment(AppStore.self) private var store
    @State private var adding = false
    @State private var renamed: ProxyAccount?
    @State private var name = ""
    @State private var removal: Removal?
    @State private var width: CGFloat = 0

    var body: some View {
        let proxy = store.proxy
        ScrollViewReader { scroll in
            SettingsList {
                header(proxy)
                if proxy.exists {
                    ForEach(Agent.allCases, id: \.self) { agent in
                        accounts(of: agent, in: proxy)
                    }
                    requests(proxy)
                }
            }
            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
            .onChange(of: store.settingsTarget, initial: true) {
                guard let target = store.settingsTarget else { return }
                withAnimation { scroll.scrollTo(target, anchor: .center) }
                store.settingsTarget = nil
            }
        }
        .onAppear { store.watchProxy(true) }
        .onDisappear { store.watchProxy(false) }
        .confirmsRemoval($removal)
        .modal(isPresented: $adding) { ProxyAddSheet(replacing: store.proxy.key) }
        .modal(item: signIn) { _ in ProxySignInSheet() }
        .alert("Rename Account", isPresented: renaming, presenting: renamed) { account in
            TextField("Work", text: $name)
            Button("Cancel", role: .cancel) {}
            Button("Save") {
                let trimmed = name.trimmingCharacters(in: .whitespaces)
                guard !trimmed.isEmpty else { return }
                store.renameProxyAccount(account, to: trimmed)
            }
        }
    }

    /// The sign-in under way, whose sheet drops it when it closes.
    private var signIn: Binding<ProxySignIn?> {
        Binding { store.proxy.signIn } set: { if $0 == nil { store.cancelProxySignIn() } }
    }

    private var renaming: Binding<Bool> {
        Binding { renamed != nil } set: { if !$0 { renamed = nil } }
    }

    private func rename(_ account: ProxyAccount) {
        name = account.name
        renamed = account
    }

    /// The proxy and how the client stands with it, or what one is and the way to add it.
    @ViewBuilder private func header(_ proxy: ProxyState) -> some View {
        Section {
            if proxy.exists {
                let name = proxy.name ?? "Proxy"
                HStack(spacing: 12) {
                    SettingsTile(symbol: .waypoints)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(name)
                            .foregroundStyle(Color.themeForeground)
                        Text(proxy.statusText)
                            .font(.footnote)
                            .foregroundStyle(Color.themeMutedForeground)
                        if let error = proxy.error {
                            Text(error)
                                .font(.footnote)
                                .foregroundStyle(Color.themeWarning)
                        }
                    }
                    Spacer(minLength: 8)
                    ActionMenu(icon: .ellipsis, help: "What to do with \(name)") {
                        MenuItem("Replace with a New Proxy", symbol: .refreshCw) { adding = true }
                        Divider()
                        MenuItem("Remove Proxy", symbol: .trash2, role: .destructive) { removal = .proxy(name: name) }
                    }
                }
                .padding(.vertical, 2)
                if let version = proxy.version {
                    LabeledContent("Version", value: version)
                }
            } else {
                HStack(spacing: 12) {
                    SettingsTile(symbol: .waypoints)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("No proxy")
                            .foregroundStyle(Color.themeForeground)
                        Text(ProxyText.absent)
                            .font(.footnote)
                            .foregroundStyle(Color.themeMutedForeground)
                    }
                }
                .padding(.vertical, 2)
                Button { adding = true } label: {
                    SettingsActionLabel("Add a Proxy", symbol: .plus)
                }
            }
        }
        .id("proxy")
    }

    /// The way to add an agent's account, and its accounts, a group each with its limits.
    @ViewBuilder private func accounts(of agent: Agent, in proxy: ProxyState) -> some View {
        let accounts = proxy.accounts(of: agent)
        Section {
            Button { store.startProxySignIn(agent) } label: {
                HStack {
                    SettingsActionLabel(ProxyText.addAccount(of: agent), symbol: .plus)
                    Spacer()
                    if store.proxySignInStarting == agent { Spinner(size: .xl) }
                }
            }
            .disabled(store.proxySignInStarting != nil)
        } header: {
            Text(ProxyText.accounts(of: agent))
        }
        .id(ProxyState.accountsID(of: agent))
        ForEach(accounts) { account in
            Section {
                rows(of: account, count: accounts.count)
            }
            .id("proxy-account/\(account.id)")
        }
    }

    @ViewBuilder private func rows(of account: ProxyAccount, count: Int) -> some View {
        HStack(spacing: 10) {
            ProxyAccountLabel(account: account)
                .frame(maxWidth: .infinity, alignment: .leading)
            ActionMenu(icon: .ellipsis, help: "What to do with \(account.name)") {
                ProxyAccountActions(account: account, count: count, rename: { rename(account) }, remove: { removal = .proxyAccount(account) })
            }
        }
        .padding(.vertical, 2)
        .alignmentGuide(.listRowSeparatorLeading) { $0[.leading] }
        // On a row of its own under the name, which a phone has no room beside.
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(ProxyText.inRotation)
                    .foregroundStyle(Color.themeForeground)
                Text(ProxyText.rotation)
                    .font(.footnote)
                    .foregroundStyle(Color.themeMutedForeground)
            }
            Spacer()
            Switch(isOn: Binding { account.enabled } set: { store.setProxyAccount(account, enabled: $0) })
        }
        .alignmentGuide(.listRowSeparatorLeading) { $0[.leading] }
        if account.status == .signInAgain {
            Button { store.startProxySignIn(account.agent) } label: {
                HStack {
                    SettingsActionLabel("Sign In Again", symbol: .circleUser)
                    Spacer()
                    if store.proxySignInStarting == account.agent { Spinner(size: .xl) }
                }
            }
            .disabled(store.proxySignInStarting != nil)
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
    private func requests(_ proxy: ProxyState) -> some View {
        Section {
            NavigationLink {
                ProxyRequestLog().glassBackButton()
            } label: {
                Text("Request Log")
                    .foregroundStyle(Color.themeForeground)
            }
            Button { store.setProxyCapture(.next(ProxyText.captureNext)) } label: {
                SettingsActionLabel("Capture the Next \(ProxyText.captureNext) Requests", symbol: .circleDot)
            }
            Menu {
                ProxyCaptureThreads()
            } label: {
                SettingsActionLabel("Capture a Thread", symbol: .messageSquareText)
            }
            if proxy.capture.on {
                Button("Stop Capturing", role: .destructive) { store.setProxyCapture(.off) }
                    .foregroundStyle(Color.themeDestructive)
            }
        } header: {
            Text("Log")
        } footer: {
            Text(proxy.capture.text)
                .foregroundStyle(Color.themeMutedForeground)
        }
        .id("proxy-log")
    }
}

/// The requests the proxy passed on, newest first, all of them or only those that failed. A
/// captured one opens in full.
private struct ProxyRequestLog: View {
    @Environment(AppStore.self) private var store
    @State private var log = ProxyLog()
    @State private var captured: ProxyCapturedEntry?

    var body: some View {
        SettingsList {
            Section {
                if let empty = log.emptyText {
                    Text(empty)
                        .foregroundStyle(Color.themeMutedForeground)
                }
                ForEach(log.rows) { row in
                    if row.captured {
                        Button { captured = ProxyCapturedEntry(id: row.id) } label: { ProxyLogRowView(row: row) }
                    } else {
                        ProxyLogRowView(row: row)
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
            store.watchProxy(true)
            log.start(store)
        }
        .onDisappear { store.watchProxy(false) }
        .onChange(of: store.proxy.logHead) { log.refresh() }
        .modal(item: $captured) { ProxyCapturedSheet(entry: $0.id) }
    }
}
#endif
