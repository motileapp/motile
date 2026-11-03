import SwiftUI

/// What the Proxy page says, on both clients.
enum ProxyText {
    static let absent = "Runs on a machine that is always on"
    /// What an account's switch says on a phone, where it has a row of its own.
    static let useAccount = "Use this account"
    /// How many requests "Capture the next" captures.
    static let captureNext = 20
    /// How many threads the capture menu offers.
    static let captureThreads = 12

    static func accounts(of agent: Agent) -> String { "\(agent.name) Accounts" }
    static func addAccount(of agent: Agent) -> String { "Add \(agent.name) Account" }
}

extension ProxyAccount.Status {
    var tone: Color? {
        switch self {
        case .signInAgain: .themeWarning
        case .resting: .themePending
        case .ready, .off: nil
        }
    }
}

extension ProxyState {
    /// The page's search targets for each agent's accounts.
    static func accountsID(of agent: Agent) -> String {
        agent == .claude ? "proxy-accounts" : "proxy-accounts-\(agent.rawValue)"
    }
}

/// An account's agent and name, and under it how it stands, who is signed in, its plan and the threads
/// on it.
struct ProxyAccountLabel: View {
    let account: ProxyAccount

    /// The badges stand beside the name, or after the plan under it where that would cut the name short.
    var body: some View {
        ViewThatFits(in: .horizontal) {
            lines(badgesBeside: true)
            lines(badgesBeside: false)
        }
    }

    private func lines(badgesBeside: Bool) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                AgentIcon(agent: account.agent, size: .md)
                name
                if badgesBeside { chips }
            }
            HStack(spacing: 6) {
                // Its length never decides where the badges go: it is cut short instead.
                description
                    .frame(minWidth: 0, idealWidth: 0, alignment: .leading)
                if !badgesBeside { chips }
            }
        }
    }

    private var name: some View {
        Text(account.name)
            .font(Self.titleFont)
            .foregroundStyle(Color.themeForeground)
            .lineLimit(1)
    }

    @ViewBuilder private var chips: some View {
        if let status = account.statusText {
            Chip(status, tone: account.status.tone, muted: account.status.tone == nil)
        }
        if account.next {
            Chip("Next", tone: .themePrimary)
        }
    }

    @ViewBuilder private var description: some View {
        if let description = account.description {
            Text(description)
                .font(Self.descriptionFont)
                .foregroundStyle(Color.themeMutedForeground)
                .lineLimit(1)
        }
    }

    #if os(macOS)
    private static let titleFont = Font.ui(size: 13, weight: .medium)
    private static let descriptionFont = Font.ui(size: 11.5)
    #else
    private static let titleFont = Font.body
    private static let descriptionFont = Font.footnote
    #endif
}

/// An item of the page's menus, with its symbol.
struct ProxyMenuItem: View {
    let title: String
    let symbol: Symbol
    var role: ButtonRole?
    let action: () -> Void

    init(_ title: String, symbol: Symbol, role: ButtonRole? = nil, action: @escaping () -> Void) {
        self.title = title
        self.symbol = symbol
        self.role = role
        self.action = action
    }

    var body: some View {
        Button(role: role, action: action) {
            if role == .destructive {
                // Red like its words, as every destructive item is.
                Label {
                    Text(title)
                } icon: {
                    Image(platform: TintedSymbol.image(symbol, size: .md, color: Theme.destructive))
                }
            } else {
                Label(title, symbol: symbol)
            }
        }
    }
}

/// What to do with an account: rename it, set its priority among its agent's `count`, sign it in
/// again, remove it.
struct ProxyAccountActions: View {
    @Environment(AppStore.self) private var store
    let account: ProxyAccount
    let count: Int
    let rename: () -> Void
    let remove: () -> Void

    var body: some View {
        ProxyMenuItem("Rename Account", symbol: .pencil, action: rename)
        Menu {
            ForEach(1...max(count, account.priority + 1), id: \.self) { number in
                Toggle("Priority \(number)", isOn: Binding { account.priority == number - 1 } set: { _ in
                    store.setProxyAccountPriority(account, to: number - 1)
                })
            }
        } label: {
            Label(account.priorityText, symbol: .listFilter)
        }
        ProxyMenuItem("Sign In Again", symbol: .circleUser) { store.startProxySignIn(account.agent) }
        Divider()
        ProxyMenuItem("Remove Account", symbol: .trash2, role: .destructive, action: remove)
    }
}

/// The threads whose every request the proxy can capture: the active ones of the account's servers.
struct ProxyCaptureThreads: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        let threads = store.activeThreads.filter { !store.isRemoved($0.serverID) }.prefix(ProxyText.captureThreads)
        if threads.isEmpty {
            Text("No thread is working")
        }
        ForEach(threads) { thread in
            Button(thread.title) { store.setProxyCapture(.thread(server: thread.serverID, thread: thread.id)) }
        }
    }
}

/// A request the proxy passed on: when, for which thread, how long it took and how it ended, what
/// it carried, and why it failed.
struct ProxyLogRowView: View {
    let row: ProxyLogRow

    var body: some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 3) {
                #if os(macOS)
                HStack(spacing: 8) {
                    time
                    place.frame(maxWidth: .infinity, alignment: .leading)
                    latency
                    status
                }
                #else
                HStack(spacing: 8) {
                    time
                    latency.frame(maxWidth: .infinity, alignment: .leading)
                    status
                }
                place
                #endif
                Text(dotted: row.details)
                    .font(.ui(size: 11.5, design: .monospaced))
                    .foregroundStyle(Color.themeMutedForeground)
                    .lineLimit(Self.detailLines)
                if let error = row.error {
                    Text(error)
                        .font(.ui(size: 11.5))
                        .foregroundStyle(Color.themeWarning)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            if row.captured {
                Image(.chevronRight, size: .md)
                    .foregroundStyle(Color.themeMutedForeground)
            }
        }
        .contentShape(Rectangle())
    }

    #if os(macOS)
    private static let detailLines = 1
    #else
    private static let detailLines = 2
    #endif

    private var time: some View {
        Text(row.time)
            .font(.ui(size: 12))
            .monospacedDigit()
            .foregroundStyle(Color.themeMutedForeground)
    }

    private var place: some View {
        Text(dotted: "\(row.server) · \(row.thread)")
            .font(.ui(size: 13, weight: .medium))
            .foregroundStyle(Color.themeForeground)
            .lineLimit(1)
    }

    private var latency: some View {
        Text(row.latency)
            .font(.ui(size: 12))
            .monospacedDigit()
            .foregroundStyle(Color.themeMutedForeground)
    }

    private var status: some View {
        Chip("\(row.status)", tone: row.ok ? nil : .themeWarning, monospaced: true)
    }
}

extension ProxyLog {
    /// Said where the log has no rows.
    var emptyText: String? {
        guard rows.isEmpty, !loading else { return nil }
        if let error { return error }
        return errorsOnly ? "No errors" : "No requests yet"
    }
}

/// How to run a new proxy and the command that links it to the account. It watches the account,
/// and closes once a proxy other than `replacing` joins it.
struct ProxyAddSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.closeModal) private var close
    @State private var enroll: Loaded<ProxyEnroll> = .loading
    @State private var pending = false
    @State private var replacing: String?

    init(replacing: String?) {
        _replacing = State(initialValue: replacing)
    }

    private var joined: Bool { store.proxy.exists && store.proxy.key != replacing }
    private var title: String { replacing == nil ? "Add a Proxy" : "Replace Your Proxy" }

    var body: some View {
        content
            .onAppear {
                store.watchAccount(true)
                makeCode()
            }
            .onDisappear { store.watchAccount(false) }
            .onChange(of: joined) {
                guard joined else { return }
                close()
            }
    }

    private func makeCode() {
        guard !pending else { return }
        pending = true
        store.addProxy { result in
            pending = false
            switch result {
            case .success(let made): enroll = .ready(made)
            case .failure(let error): enroll = .failed(error.message)
            }
        }
    }

    /// How long the code still works while the proxy is awaited, or the way to a new one.
    private var status: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            status(at: context.date)
        }
    }

    @ViewBuilder private func status(at date: Date) -> some View {
        if let enroll = enroll.value {
            if enroll.isExpired(at: date) {
                HStack(spacing: 10) {
                    Text("The code has expired")
                        .font(.ui(size: 13))
                        .foregroundStyle(Color.themeMutedForeground)
                    ActionButton("New Code", pending: pending) { makeCode() }
                }
            } else {
                HStack(spacing: 8) {
                    Spinner(size: .md)
                    Text(replacing == nil ? "Waiting for your proxy" : "Waiting for your new proxy")
                        .font(.ui(size: 13))
                    Text(enroll.timeLeft(at: date))
                        .font(.ui(size: 12))
                        .monospacedDigit()
                        .foregroundStyle(Color.themeMutedStrongerForeground)
                }
                .foregroundStyle(Color.themeMutedForeground)
            }
        }
    }

    private var failure: String? {
        guard case .failed(let message) = enroll else { return nil }
        return message
    }

    #if os(macOS)
    private var content: some View {
        ViewThatFits(in: .vertical) {
            form
            ScrollView { form }
        }
        .frame(width: 540)
    }

    private var form: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(title)
                .font(.ui(size: 15, weight: .semibold))
            if let enroll = enroll.value {
                steps(enroll)
            } else if let failure {
                Text(failure)
                    .font(.ui(size: 13))
                    .foregroundStyle(Color.themeDestructive)
                    .fixedSize(horizontal: false, vertical: true)
                ActionButton("Try Again", pending: pending) { makeCode() }
            } else {
                Spinner()
                    .foregroundStyle(Color.themeMutedForeground)
                    .frame(maxWidth: .infinity, minHeight: 120)
            }
            HStack {
                Spacer()
                ActionButton("Cancel") { close() }
                    .keyboardShortcut(.cancelAction)
            }
        }
        .padding(20)
    }

    private func steps(_ enroll: ProxyEnroll) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 6) {
                ForEach(Array(enroll.steps.enumerated()), id: \.offset) { index, step in
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text("\(index + 1).")
                            .foregroundStyle(Color.themeMutedStrongerForeground)
                            .frame(width: 16, alignment: .leading)
                        Text(step)
                            .foregroundStyle(Color.themeMutedForeground)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .font(.ui(size: 13))
                }
            }
            CommandBox(command: enroll.command, spans: enroll.spans)
            status.padding(.horizontal, 4)
        }
    }
    #else
    private var content: some View {
        NavigationStack {
            SettingsList {
                Section {
                    if enroll.value != nil {
                        status
                    } else if failure == nil {
                        Spinner(size: .xl)
                            .foregroundStyle(Color.themeMutedForeground)
                            .frame(maxWidth: .infinity)
                    } else {
                        Button("Try Again") { makeCode() }
                            .disabled(pending)
                    }
                } footer: {
                    if let failure {
                        Text(failure).foregroundStyle(Color.themeDestructive)
                    }
                }
                if let enroll = enroll.value {
                    Section("Steps") {
                        ForEach(Array(enroll.steps.enumerated()), id: \.offset) { index, step in
                            HStack(alignment: .firstTextBaseline, spacing: 12) {
                                Text("\(index + 1)")
                                    .monospacedDigit()
                                    .foregroundStyle(Color.themeMutedForeground)
                                Text(step)
                                    .foregroundStyle(Color.themeForeground)
                            }
                        }
                    }
                    Section("Command") {
                        CommandBox(command: enroll.command, spans: enroll.spans)
                            .listRowInsets(EdgeInsets())
                            .listRowBackground(Color.clear)
                    }
                }
            }
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                GlassToolbarItem(placement: .cancellationAction) { SheetCloseButton() }
            }
        }
        .presentationDragIndicator(.visible)
    }
    #endif
}

/// An account's sign-in on the proxy, as the core says it goes: Claude's page shows a code to paste
/// here, Codex's takes the code shown here and adds the account once the user approves it. Closing
/// it drops the sign-in.
struct ProxySignInSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.closeModal) private var close
    @State private var code = ""
    @FocusState private var codeFocused: Bool

    private var canFinish: Bool {
        guard let signIn = store.proxy.signIn else { return false }
        return signIn.needsCode && !signIn.waiting && !code.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private func finish() {
        guard canFinish else { return }
        store.finishProxySignIn(code: code)
    }

    /// Codex's sign-in is over once it fails, and starts again; Claude's waits for another code.
    private func startsAgain(_ signIn: ProxySignIn) -> Bool {
        signIn.error != nil && !signIn.needsCode
    }

    private func userCode(_ userCode: String) -> some View {
        HStack(spacing: 10) {
            Text(userCode)
                .font(.ui(size: 26, weight: .semibold, design: .monospaced))
                .foregroundStyle(Color.themeForeground)
                .textSelection(.enabled)
            CopyButton(help: "Copy the code") { Platform.copy(userCode) }
        }
        .frame(maxWidth: .infinity)
    }

    private var waiting: some View {
        HStack(spacing: 8) {
            Spinner(size: .md)
            Text("Waiting for you to approve it")
                .font(.ui(size: 13))
        }
        .foregroundStyle(Color.themeMutedForeground)
    }

    /// What to do, the link, and the code: the same on every system.
    @ViewBuilder private func steps(_ signIn: ProxySignIn) -> some View {
        Text(signIn.text)
            .font(.ui(size: 13))
            .foregroundStyle(Color.themeMutedForeground)
            .fixedSize(horizontal: false, vertical: true)
        if let url = signIn.url {
            CommandBox(command: url.absoluteString, link: url)
        }
        if let user = signIn.userCode {
            userCode(user).padding(.vertical, 6)
        }
        if signIn.needsCode {
            HStack(spacing: 6) {
                InputField("Paste the code", text: $code, monospaced: true, focus: $codeFocused)
                    .onSubmit(finish)
                    .onAppear { codeFocused = true }
                ActionButton("Sign In", variant: .primary, pending: signIn.waiting, action: finish)
                    .disabled(!canFinish && !signIn.waiting)
            }
        } else if signIn.waiting && signIn.error == nil {
            waiting
        }
        if let error = signIn.error {
            Text(error)
                .font(.ui(size: 12))
                .foregroundStyle(Color.themeDestructive)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    #if os(macOS)
    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            if let signIn = store.proxy.signIn {
                Text("Sign In to \(signIn.agent.name)")
                    .font(.ui(size: 15, weight: .semibold))
                steps(signIn)
                HStack(spacing: 0) {
                    Spacer()
                    ActionButton("Cancel", margin: .beside(trailing: startsAgain(signIn) ? 4 : 0)) { close() }
                        .keyboardShortcut(.cancelAction)
                    if startsAgain(signIn) {
                        ActionButton("Start Again", variant: .primary, pending: store.proxySignInStarting != nil, margin: .beside(leading: 4)) {
                            store.startProxySignIn(signIn.agent)
                        }
                    }
                }
            }
        }
        .padding(20)
        .frame(width: 460)
    }
    #else
    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    if let signIn = store.proxy.signIn {
                        steps(signIn)
                        if startsAgain(signIn) {
                            ActionButton("Start Again", variant: .primary, pending: store.proxySignInStarting != nil, fills: true) {
                                store.startProxySignIn(signIn.agent)
                            }
                        }
                    }
                }
                .padding(20)
            }
            .navigationTitle("Sign In to \(store.proxy.signIn?.agent.name ?? "")")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                GlassToolbarItem(placement: .cancellationAction) { SheetCloseButton() }
            }
        }
        .presentationDragIndicator(.visible)
    }
    #endif
}

/// A captured request in full: its request or its response, each its headers then its body, in
/// the code view, which draws only the lines on screen.
struct ProxyCapturedSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.closeModal) private var close
    let entry: Int
    @State private var captured: Loaded<ProxyCaptured?> = .loading
    @State private var response = false

    private var tabs: some View {
        Segmented([("Request", false), ("Response", true)], selection: $response)
    }

    @ViewBuilder private var document: some View {
        switch captured {
        case .loading:
            Spinner()
                .foregroundStyle(Color.themeMutedForeground)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        case .failed(let message):
            note(message, color: .themeDestructive)
        case .ready(nil):
            note("Your proxy no longer has this request", color: .themeMutedForeground)
        case .ready(let captured?):
            CodeViewRepresentable(document: response ? captured.response : captured.request)
        }
    }

    private func note(_ text: String, color: Color) -> some View {
        Text(text)
            .font(.ui(size: 13))
            .foregroundStyle(color)
            .multilineTextAlignment(.center)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    @ViewBuilder private var summary: some View {
        if let row = captured.value??.row {
            Text(dotted: row.summary)
                .font(.ui(size: 12))
                .foregroundStyle(Color.themeMutedForeground)
                .lineLimit(1)
        }
    }

    private func load() {
        store.loadProxyCaptured(entry) { result in
            switch result {
            case .success(let request): captured = .ready(request)
            case .failure(let error): captured = .failed(error.message)
            }
        }
    }

    #if os(macOS)
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 12) {
                Text("Captured Request")
                    .font(.ui(size: 15, weight: .semibold))
                Spacer()
                tabs
            }
            summary
            document
                .frame(height: 560)
                .clipShape(RoundedRectangle(cornerRadius: Radius.md, style: .continuous))
                .overlay { RoundedRectangle(cornerRadius: Radius.md, style: .continuous).strokeBorder(Color.themeBorder, lineWidth: 1) }
            HStack {
                Spacer()
                ActionButton("Done", variant: .primary) { close() }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 820)
        .onAppear(perform: load)
    }
    #else
    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 10) {
                tabs
                summary
                document
            }
            .padding(.top, 8)
            .padding(.horizontal, 16)
            .background(Color.themeBackground.ignoresSafeArea())
            .navigationTitle("Captured Request")
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
