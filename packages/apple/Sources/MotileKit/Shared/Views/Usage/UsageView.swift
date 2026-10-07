import SwiftUI

/// What the usage route shows: the tab, the period and the servers picked, which it keeps for
/// next time, and what was last read for them. It shows what the core kept at once, and reads it
/// again.
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

    /// The servers counted among these, or nil for all of them, also when none of those picked
    /// is one of them any more.
    private func picked(among all: [Server]) -> Set<String>? {
        guard let servers else { return nil }
        let picked = servers.intersection(all.map(\.id))
        guard !picked.isEmpty, picked.count < all.count else { return nil }
        return picked
    }

    /// Counts the server or leaves it out. Counting every server is counting all of them, and the
    /// last one counted stays.
    func set(_ server: Server, counted: Bool, among all: [Server]) {
        let everyone = Set(all.map(\.id))
        var chosen = picked(among: all) ?? everyone
        if counted { chosen.insert(server.id) } else { chosen.remove(server.id) }
        guard !chosen.isEmpty else { return }
        servers = chosen == everyone ? nil : chosen
    }

    /// "All servers", the one server's name, or how many.
    func serversLabel(among all: [Server]) -> String {
        guard let chosen = picked(among: all) else { return "All servers" }
        let named = all.filter { chosen.contains($0.id) }
        return named.count == 1 ? named[0].name : "\(named.count) servers"
    }

    /// All of them, then each on its own.
    func serverChecks(among all: [Server]) -> [[Check]] {
        let chosen = picked(among: all)
        let everyone = Check(id: "all", title: "All servers", checked: chosen == nil) { [weak self] in self?.servers = nil }
        let each = all.map { server in
            let counted = chosen?.contains(server.id) ?? true
            return Check(id: server.id, title: server.name, checked: counted) { [weak self] in
                self?.set(server, counted: !counted, among: all)
            }
        }
        return [[everyone], each]
    }

    private func load(refresh: Bool = false) {
        guard let store else { return }
        asked += 1
        let number = asked
        refreshing = true
        let picked = servers.map { $0.intersection(store.servers.map(\.id)) }.flatMap { $0.isEmpty ? nil : $0 }
        if tab == .limits {
            store.loadLimits(refresh: refresh, kept: !refresh, servers: picked) { [weak self] result in
                guard let self, number == asked else { return }
                guard case .success(let report) = result, report.stale else {
                    refreshing = false
                    limits = Self.loaded(result, over: limits)
                    return
                }
                if !report.sections.isEmpty { limits = .ready(report) }
                store.loadLimits(refresh: false, kept: false, servers: picked) { [weak self] result in
                    guard let self, number == asked else { return }
                    refreshing = false
                    limits = Self.loaded(result, over: limits)
                }
            }
        } else {
            let (seconds, buckets) = (period.bucketSeconds, period.buckets)
            store.loadUsage(bucketSeconds: seconds, buckets: buckets, kept: true, servers: picked) { [weak self] kept in
                guard let self, number == asked else { return }
                if case .success(let report) = kept { spending = .ready(report) }
                store.loadUsage(bucketSeconds: seconds, buckets: buckets, kept: false, servers: picked) { [weak self] result in
                    guard let self, number == asked else { return }
                    refreshing = false
                    spending = Self.loaded(result, over: spending)
                }
            }
        }
    }

    /// What was read, or what is shown when reading it failed.
    private static func loaded<Value>(_ result: Result<Value, CoreBridge.CoreError>, over shown: Loaded<Value>) -> Loaded<Value> {
        switch result {
        case .success(let value): .ready(value)
        case .failure where shown.value != nil: shown
        case .failure(let error): .failed(error.message)
        }
    }
}

/// The route's title as a path: "Usage", then the servers counted, which picks them.
struct UsageTitle: View {
    @Environment(AppStore.self) private var store
    let model: UsageModel

    private static let size = ControlSize.regular

    var body: some View {
        HStack(spacing: Self.size.padding) {
            Text("Usage")
                .foregroundStyle(Color.themeSecondary)
            Text("/")
                .foregroundStyle(Color.themeTertiary)
            CheckMenu(
                model.serversLabel(among: store.servers), help: "The servers counted", size: Self.size, tint: .themeText,
                groups: model.serverChecks(among: store.servers)
            )
            .padding(.leading, -Self.size.padding)
        }
        .font(Self.size.font)
        .lineLimit(1)
    }
}

/// The servers counted, as a menu that picks them.
struct UsageServersMenu: View {
    @Environment(AppStore.self) private var store
    let model: UsageModel

    var body: some View {
        CheckMenu(
            model.serversLabel(among: store.servers), icon: .server, help: "The servers counted", variant: .secondary,
            groups: model.serverChecks(among: store.servers)
        )
    }
}

/// The tabs, the period of the cost and the tokens, and the way to read it all again, in a row.
struct UsageControls: View {
    @Bindable var model: UsageModel

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 8) {
                UsageTabs(model: model)
                Segmented(UsageModel.Period.allCases.map { ($0.label, $0) }, selection: $model.period)
                    .disabled(model.tab == .limits)
                UsageRefreshButton(model: model)
            }
            HStack(spacing: 8) {
                UsageTabs(model: model)
                UsagePeriodMenu(model: model)
                UsageRefreshButton(model: model)
            }
        }
    }
}

struct UsageTabs: View {
    @Bindable var model: UsageModel
    var fills = false

    var body: some View {
        Segmented(UsageModel.Tab.allCases.map { ($0.label, $0) }, selection: $model.tab, fills: fills)
    }
}

struct UsagePeriodMenu: View {
    @Bindable var model: UsageModel

    var body: some View {
        ActionMenu(model.period.label, variant: .secondary) {
            Picker("Period", selection: $model.period) {
                ForEach(UsageModel.Period.allCases) { Text($0.label).tag($0) }
            }
            .pickerStyle(.inline)
        }
        .disabled(model.tab == .limits)
    }
}

struct UsageRefreshButton: View {
    let model: UsageModel

    var body: some View {
        ActionButton(icon: .refreshCw, help: "Refresh", pending: model.refreshing) { model.refresh() }
    }
}

/// What the open tab shows, in a column that doesn't grow past a width.
struct UsageContent: View {
    private static let width: CGFloat = 880

    @Environment(AppStore.self) private var store
    let model: UsageModel
    var margin: CGFloat = 24
    var top: CGFloat = 20

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
            .padding(.horizontal, margin)
            .padding(.top, top)
            .padding(.bottom, 20)
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
