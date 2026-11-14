import SwiftUI

/// What the usage route shows: the tab, the period and the servers picked, which it keeps for
/// next time, and what was last read for them. It shows what the core kept at once, and reads it
/// again.
@Observable
final class UsageModel {
    enum Tab: String, CaseIterable, Identifiable {
        case activity, cost, tokens, limits

        var id: Self { self }

        /// What the tab asks the servers for; the cost and the tokens are read together.
        var reads: Int {
            switch self {
            case .activity: 0
            case .cost, .tokens: 1
            case .limits: 2
            }
        }

        var label: String {
            switch self {
            case .activity: "Activity"
            case .cost: "Cost"
            case .tokens: "Tokens"
            case .limits: "Limits"
            }
        }

        var symbol: Symbol {
            switch self {
            case .activity: .activity
            case .cost: .circleDollarSign
            case .tokens: .coins
            case .limits: .gauge
            }
        }
    }

    enum Period: String, CaseIterable, Identifiable {
        case day, week, month, quarter

        var id: Self { self }

        var label: String {
            switch self {
            case .day: "24h"
            case .week: "7d"
            case .month: "30d"
            case .quarter: "90d"
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

    var tab = Tab(rawValue: UserDefaults.standard.string(forKey: tabKey) ?? "") ?? .activity {
        didSet {
            UserDefaults.standard.set(tab.rawValue, forKey: Self.tabKey)
            guard oldValue.reads != tab.reads else { return }
            load()
        }
    }

    var period = Period(rawValue: UserDefaults.standard.string(forKey: periodKey) ?? "") ?? .month {
        didSet {
            UserDefaults.standard.set(period.rawValue, forKey: Self.periodKey)
            load()
        }
    }

    /// The servers picked. None picked counts all of them, as does every one.
    var servers = Set(UserDefaults.standard.stringArray(forKey: serversKey) ?? []) {
        didSet {
            UserDefaults.standard.set(Array(servers), forKey: Self.serversKey)
            load()
        }
    }

    private(set) var limits: Loaded<LimitsReport> = .loading
    private(set) var spending: Loaded<UsageReport> = .loading
    private(set) var activity: Loaded<ActivityReport> = .loading
    private(set) var refreshing = false {
        didSet {
            guard !refreshing else { return }
            waiting.forEach { $0.resume() }
            waiting = []
        }
    }
    /// What waits for the reading that runs to end.
    @ObservationIgnored private var waiting: [CheckedContinuation<Void, Never>] = []
    @ObservationIgnored private weak var store: AppStore?
    /// Counts what was asked, so that only the last answer is shown.
    @ObservationIgnored private var asked = 0

    func start(_ store: AppStore) {
        self.store = store
        load()
    }

    /// Asks the servers again for what the tab shows.
    func refresh() {
        load()
    }

    /// Asks the servers again, and returns once they answered.
    @MainActor func refreshed() async {
        load()
        guard refreshing else { return }
        await withCheckedContinuation { waiting.append($0) }
    }

    /// The servers counted among these, or nil for all of them.
    private func picked(among all: [Server]) -> Set<String>? {
        let picked = servers.intersection(all.map(\.id))
        guard !picked.isEmpty, picked.count < all.count else { return nil }
        return picked
    }

    func toggle(_ server: Server, among all: [Server]) {
        var picked = servers.intersection(all.map(\.id))
        if picked.remove(server.id) == nil { picked.insert(server.id) }
        servers = picked
    }

    /// "All servers", the one server's name, or how many.
    func serversLabel(among all: [Server]) -> String {
        guard let chosen = picked(among: all) else { return "All servers" }
        let named = all.filter { chosen.contains($0.id) }
        return named.count == 1 ? named[0].name : "\(named.count) servers"
    }

    func serverChecks(among all: [Server]) -> [Check] {
        all.map { server in
            Check(id: server.id, title: server.name, checked: servers.contains(server.id)) { [weak self] in
                self?.toggle(server, among: all)
            }
        }
    }

    /// Shows what the core kept for the tab at once, then asks the servers.
    private func load() {
        guard let store else { return }
        asked += 1
        let number = asked
        refreshing = true
        let counted = picked(among: store.usageServers)
        if tab == .limits {
            store.loadLimits(kept: true, servers: counted) { [weak self] kept in
                guard let self, number == asked else { return }
                if case .success(let report) = kept, !report.sections.isEmpty { limits = .ready(report) }
                store.loadLimits(kept: false, servers: counted) { [weak self] result in
                    guard let self, number == asked else { return }
                    refreshing = false
                    limits = Self.loaded(result, over: limits)
                }
            }
        } else if tab == .activity {
            let (seconds, buckets) = (period.bucketSeconds, period.buckets)
            store.loadActivity(bucketSeconds: seconds, buckets: buckets, kept: true, servers: counted) { [weak self] kept in
                guard let self, number == asked else { return }
                if case .success(let report) = kept { activity = .ready(report) }
                store.loadActivity(bucketSeconds: seconds, buckets: buckets, kept: false, servers: counted) { [weak self] result in
                    guard let self, number == asked else { return }
                    refreshing = false
                    activity = Self.loaded(result, over: activity.value?.loading == true ? .loading : activity)
                }
            }
        } else {
            let (seconds, buckets) = (period.bucketSeconds, period.buckets)
            store.loadUsage(bucketSeconds: seconds, buckets: buckets, kept: true, servers: counted) { [weak self] kept in
                guard let self, number == asked else { return }
                if case .success(let report) = kept { spending = .ready(report) }
                store.loadUsage(bucketSeconds: seconds, buckets: buckets, kept: false, servers: counted) { [weak self] result in
                    guard let self, number == asked else { return }
                    refreshing = false
                    spending = Self.loaded(result, over: spending.value?.loading == true ? .loading : spending)
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
            HStack(spacing: Self.size.padding) {
                Text("Usage")
                    .foregroundStyle(Color.themeMutedForeground)
                Text("/")
                    .foregroundStyle(Color.themeMutedStrongerForeground)
            }
            .allowsHitTesting(false)
            CheckMenu(
                model.serversLabel(among: store.usageServers), help: "The servers counted", size: Self.size, tint: .themeForeground,
                checks: model.serverChecks(among: store.usageServers)
            )
            .padding(.leading, -Self.size.padding)
        }
        .font(Self.size.font)
        .lineLimit(1)
    }
}

/// The tabs, the period of the activity, the cost and the tokens, and the way to read it all again,
/// in a row: the period as a menu where they don't fit, and the tabs too where even that doesn't.
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
                HStack(spacing: 0) {
                    UsagePeriodMenu(model: model, margin: .beside(trailing: 4))
                    UsageRefreshButton(model: model, margin: .beside(leading: 4))
                }
            }
            HStack(spacing: 0) {
                UsageTabsMenu(model: model, margin: .beside(trailing: 4))
                UsagePeriodMenu(model: model, margin: .beside(leading: 4, trailing: 4))
                UsageRefreshButton(model: model, margin: .beside(leading: 4))
            }
        }
    }
}

/// The tabs as a menu, where a narrow window has no room for them side by side.
struct UsageTabsMenu: View {
    @Bindable var model: UsageModel
    var margin = EdgeInsets()

    var body: some View {
        ActionMenu(model.tab.label, variant: .outline, margin: margin) {
            Picker("Tab", selection: $model.tab) {
                ForEach(UsageModel.Tab.allCases) { Text($0.label).tag($0) }
            }
            .pickerStyle(.inline)
        }
    }
}

struct UsageTabs: View {
    @Bindable var model: UsageModel

    var body: some View {
        Segmented(UsageModel.Tab.allCases.map { ($0.label, $0) }, selection: $model.tab)
    }
}

struct UsagePeriodMenu: View {
    @Bindable var model: UsageModel
    var margin = EdgeInsets()

    var body: some View {
        ActionMenu(model.period.label, variant: .outline, margin: margin) {
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
    var margin = EdgeInsets()

    var body: some View {
        ActionButton(icon: .refreshCw, help: "Refresh", pending: model.refreshing, margin: margin) { model.refresh() }
    }
}

/// What the open tab shows, in a column that doesn't grow past a width.
extension EnvironmentValues {
    /// How tall the usage page shows, which a long list in it is kept to a part of.
    @Entry var usagePageHeight: CGFloat = 800
}

struct UsageContent: View {

    @Environment(AppStore.self) private var store
    @State private var pageHeight: CGFloat = 800
    let model: UsageModel
    /// The tab shown, where it isn't the one picked.
    var shown: UsageModel.Tab?
    var margin: CGFloat = 24
    var top: CGFloat = 20

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                page
            }
            .environment(\.usagePageHeight, pageHeight)
            .frame(maxWidth: Theme.contentWidth, alignment: .leading)
            .padding(.horizontal, margin)
            .padding(.top, top)
            .padding(.bottom, 80)
            .frame(maxWidth: .infinity)
        }
        .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { pageHeight = $0 }
        .onAppear {
            guard shown == nil else { return }
            model.start(store)
        }
    }

    private var tab: UsageModel.Tab { shown ?? model.tab }

    @ViewBuilder private var page: some View {
        switch tab {
        case .limits:
            // The gateway's accounts are among the limits while it is followed.
            VStack(alignment: .leading, spacing: 0) { limits }
                .onAppear { store.watchGateway(true) }
                .onDisappear { store.watchGateway(false) }
        case .activity: activity
        case .cost: spending(.cost)
        case .tokens: spending(.tokens)
        }
    }

    @ViewBuilder private var limits: some View {
        switch model.limits {
        case .loading: loading
        case .failed(let message): UsageNote(text: message)
        case .ready(let report): LimitsView(report: report) { model.refresh() }
        }
    }

    @ViewBuilder private var activity: some View {
        switch model.activity {
        case .loading: loading
        case .failed(let message): UsageNote(text: message)
        case .ready(let report): ActivityView(report: report, period: model.period)
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
            .foregroundStyle(Color.themeMutedForeground)
            .frame(maxWidth: .infinity, minHeight: 240)
    }
}

/// Why a tab has nothing to show.
struct UsageNote: View {
    let text: String

    var body: some View {
        Text(text)
            .font(.ui(.xs))
            .foregroundStyle(Color.themeMutedForeground)
            .multilineTextAlignment(.center)
            .frame(maxWidth: .infinity, minHeight: 240)
    }
}
