import SwiftUI

/// What the usage route shows: the tab, the period and the servers picked, which it keeps for
/// next time, and what was last read for them.
@Observable
final class UsageModel {
    enum Tab: String, CaseIterable, Identifiable {
        case cost, tokens, limits

        var id: Self { self }

        var label: String {
            switch self {
            case .cost: "Cost"
            case .tokens: "Tokens"
            case .limits: "Limits"
            }
        }
    }

    enum Period: String, CaseIterable, Identifiable {
        case day, week, month, quarter

        var id: Self { self }

        var label: String {
            switch self {
            case .day: "24 hours"
            case .week: "7 days"
            case .month: "30 days"
            case .quarter: "90 days"
            }
        }

        var bucketSeconds: Int { self == .day ? 3600 : 86400 }

        var buckets: Int {
            switch self {
            case .day: 24
            case .week: 7
            case .month: 30
            case .quarter: 90
            }
        }
    }

    private static let tabKey = "usage.tab"
    private static let periodKey = "usage.period"
    private static let serversKey = "usage.servers"

    var tab = Tab(rawValue: UserDefaults.standard.string(forKey: tabKey) ?? "") ?? .limits {
        didSet {
            UserDefaults.standard.set(tab.rawValue, forKey: Self.tabKey)
            guard (oldValue == .limits) != (tab == .limits) else { return }
            load()
        }
    }

    var period = Period(rawValue: UserDefaults.standard.string(forKey: periodKey) ?? "") ?? .week {
        didSet {
            UserDefaults.standard.set(period.rawValue, forKey: Self.periodKey)
            load()
        }
    }

    /// The servers counted, all of them when `nil`.
    var servers = UserDefaults.standard.stringArray(forKey: serversKey).map(Set.init) {
        didSet {
            UserDefaults.standard.set(servers.map(Array.init), forKey: Self.serversKey)
            load()
        }
    }

    private(set) var limits: Loaded<LimitsReport> = .loading
    private(set) var spending: Loaded<UsageReport> = .loading
    private(set) var refreshing = false
    @ObservationIgnored private weak var store: AppStore?
    /// Counts what was asked, so that only the last answer is shown.
    @ObservationIgnored private var asked = 0

    func start(_ store: AppStore) {
        self.store = store
        load()
    }

    /// Reads again what the tab shows, the limits from the agents themselves.
    func refresh() {
        load(refresh: true)
    }

    func counts(_ server: Server) -> Bool {
        servers?.contains(server.id) ?? true
    }

    /// Counts the server or leaves it out. Counting every server is counting all of them, and the
    /// last one counted stays.
    func set(_ server: Server, counted: Bool, among all: [Server]) {
        let everyone = Set(all.map(\.id))
        var picked = (servers ?? everyone).intersection(everyone)
        if counted { picked.insert(server.id) } else { picked.remove(server.id) }
        guard !picked.isEmpty else { return }
        servers = picked == everyone ? nil : picked
    }

    /// "All servers", the one server's name, or how many.
    func serversLabel(among all: [Server]) -> String {
        guard let servers else { return "All servers" }
        let picked = all.filter { servers.contains($0.id) }
        guard !picked.isEmpty, picked.count < all.count else { return "All servers" }
        return picked.count == 1 ? picked[0].name : "\(picked.count) servers"
    }

    private func load(refresh: Bool = false) {
        guard let store else { return }
        asked += 1
        let number = asked
        refreshing = true
        let picked = servers.map { $0.intersection(store.servers.map(\.id)) }.flatMap { $0.isEmpty ? nil : $0 }
        if tab == .limits {
            store.loadLimits(refresh: refresh, servers: picked) { [weak self] result in
                guard let self, number == asked else { return }
                refreshing = false
                limits = Self.loaded(result)
            }
        } else {
            store.loadUsage(bucketSeconds: period.bucketSeconds, buckets: period.buckets, servers: picked) { [weak self] result in
                guard let self, number == asked else { return }
                refreshing = false
                spending = Self.loaded(result)
            }
        }
    }

    private static func loaded<Value>(_ result: Result<Value, CoreBridge.CoreError>) -> Loaded<Value> {
        switch result {
        case .success(let value): .ready(value)
        case .failure(let error): .failed(error.message)
        }
    }
}

/// The route's title on two lines, as the thread's is: the servers counted, which picks them,
/// over "Usage".
struct UsageTitle: View {
    @Environment(AppStore.self) private var store
    let model: UsageModel

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ActionMenu(model.serversLabel(among: store.servers), help: "The servers counted", size: .small) {
                UsageServerPicks(model: model)
            }
            .fixedSize()
            .padding(.leading, -ControlSize.small.padding)
            Text("Usage")
                .font(.ui(size: 13, weight: .semibold))
                .foregroundStyle(Color.themeText)
        }
        .lineLimit(1)
    }
}

/// The choices of the servers counted: all of them, or each on its own.
struct UsageServerPicks: View {
    @Environment(AppStore.self) private var store
    let model: UsageModel

    var body: some View {
        Toggle("All servers", isOn: Binding(get: { model.servers == nil }, set: { if $0 { model.servers = nil } }))
        Divider()
        ForEach(store.servers) { server in
            Toggle(
                server.name, isOn: Binding(get: { model.counts(server) }, set: { model.set(server, counted: $0, among: store.servers) }))
        }
    }
}

/// The tabs, the period of the cost and the tokens, and the way to read it all again.
struct UsageControls: View {
    @Bindable var model: UsageModel

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 8) {
                tabs
                Segmented(UsageModel.Period.allCases.map { ($0.label, $0) }, selection: $model.period)
                    .disabled(model.tab == .limits)
                refresh
            }
            HStack(spacing: 8) {
                tabs
                ActionMenu(model.period.label, variant: .secondary) {
                    Picker("Period", selection: $model.period) {
                        ForEach(UsageModel.Period.allCases) { Text($0.label).tag($0) }
                    }
                    .pickerStyle(.inline)
                }
                .disabled(model.tab == .limits)
                refresh
            }
        }
    }

    private var tabs: some View {
        Segmented(UsageModel.Tab.allCases.map { ($0.label, $0) }, selection: $model.tab)
    }

    private var refresh: some View {
        ActionButton(icon: .refreshCw, help: "Refresh", pending: model.refreshing) { model.refresh() }
    }
}

/// What the open tab shows, in a column that doesn't grow past a width.
struct UsageContent: View {
    private static let width: CGFloat = 880

    @Environment(AppStore.self) private var store
    let model: UsageModel

    var body: some View {
        ScrollView {
            Group {
                switch model.tab {
                case .limits: limits
                case .cost: spending(.cost)
                case .tokens: spending(.tokens)
                }
            }
            .frame(maxWidth: Self.width, alignment: .leading)
            .padding(.horizontal, 24)
            .padding(.vertical, 20)
            .frame(maxWidth: .infinity)
        }
        .onAppear { model.start(store) }
    }

    @ViewBuilder private var limits: some View {
        switch model.limits {
        case .loading: loading
        case .failed(let message): UsageNote(text: message)
        case .ready(let report): LimitsView(report: report)
        }
    }

    @ViewBuilder private func spending(_ measure: SpendingView.Measure) -> some View {
        switch model.spending {
        case .loading: loading
        case .failed(let message): UsageNote(text: message)
        case .ready(let report): SpendingView(report: report, measure: measure, period: model.period)
        }
    }

    private var loading: some View {
        Spinner()
            .foregroundStyle(Color.themeSecondary)
            .frame(maxWidth: .infinity, minHeight: 240)
    }
}

/// Why a tab has nothing to show.
struct UsageNote: View {
    let text: String

    var body: some View {
        Text(text)
            .font(.ui(size: 12))
            .foregroundStyle(Color.themeSecondary)
            .multilineTextAlignment(.center)
            .frame(maxWidth: .infinity, minHeight: 240)
    }
}
