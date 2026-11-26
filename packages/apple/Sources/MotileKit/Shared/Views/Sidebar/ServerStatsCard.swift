import SwiftUI

/// What a server's machine does: how long it has been up, then its CPU, memory and traffic over
/// the last minutes as bars and how full its disk is, scrolling where the window is too short. It
/// follows the machine while it is shown.
struct ServerStatsCard: View {
    static let width: CGFloat = 300
    static let edge: CGFloat = 14
    private static let headerTop: CGFloat = 10
    private static let headerBottom: CGFloat = 10
    /// The update's button stands as far from the header's side as from its top and bottom.
    private static let buttonSide: CGFloat = 12

    @Environment(AppStore.self) private var store
    let server: Server

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ServerStatsHeader(server: server, named: true)
                .padding(.leading, Self.edge)
                .padding(.trailing, Self.buttonSide)
                .padding(.top, Self.headerTop)
                .padding(.bottom, Self.headerBottom)
            Rectangle()
                .fill(Color.themeBorderPopover)
                .frame(height: 1)
            FadingScroll(maxHeight: .infinity) {
                ServerStatsRows(server: server)
                    .padding(.horizontal, Self.edge)
                    .padding(.top, ServerStatsRows.top)
                    .padding(.bottom, Self.edge)
            }
        }
        .frame(width: Self.width, alignment: .leading)
        .dropdownCard()
        .onAppear { store.watchStats(of: server.id, true) }
        .onDisappear { store.watchStats(of: server.id, false) }
    }
}

/// The server's name where nothing above names it, its uptime, the update's button at their end
/// and, while the update goes, how far it is under them.
struct ServerStatsHeader: View {
    /// On iOS the button lies in a list's rounded group, whose corners a pill follows.
    #if os(iOS)
    private static let pill = true
    #else
    private static let pill = false
    #endif

    @Environment(AppStore.self) private var store
    let server: Server
    let named: Bool

    var body: some View {
        let step = store.updateStep(of: server)
        return VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                VStack(alignment: .leading, spacing: 2) {
                    if named {
                        Text(server.name)
                            .font(.ui(.sm, weight: .medium))
                            .foregroundStyle(Color.themeForeground)
                    }
                    // The line keeps its room while the stats come, so that nothing under it moves.
                    uptime
                        .font(ServerStatsRow.font)
                        .monospacedDigit()
                }
                .lineLimit(1)
                Spacer(minLength: 0)
                updateButton(step)
            }
            .frame(minHeight: ControlSize.regular.height)
            updateLine(step)
        }
    }

    /// Named as a part's row is, and as much as its value.
    private var uptime: Text {
        guard let uptime = store.serverStats[server.id]?.uptime else { return Text(verbatim: " ") }
        let value = Text(uptime).foregroundStyle(Color.themeForeground).fontWeight(.medium)
        return Text("\(muted("Uptime:")) \(value)")
    }

    @ViewBuilder private func updateButton(_ step: ServerUpdateStep) -> some View {
        switch step {
        case .offered(busy: false):
            ActionButton("Update", help: "Install the latest version on \(server.shortName). It restarts once no agent works there.", variant: .secondary, size: .regular, round: Self.pill) {
                store.update(server)
            }
        case .offered(busy: true):
            ActionMenu("Update", help: "Agents are working on \(server.shortName). Update it once they finish, or now: they stop and continue once it is back.", variant: .secondary, size: .regular, round: Self.pill) {
                Button("Update when agents finish") { store.update(server, when: .idle) }
                Button("Update now and auto-resume agents") { store.update(server, when: .now) }
            }
        case .waiting:
            ActionButton("Restart Now", help: "Stop the agents on \(server.shortName) and restart it. Their threads continue once it is back.", variant: .secondary, size: .regular, round: Self.pill) {
                store.update(server, when: .now)
            }
        case .current, .downloading, .restarting:
            EmptyView()
        }
    }

    @ViewBuilder private func updateLine(_ step: ServerUpdateStep) -> some View {
        switch step {
        case .downloading(let progress):
            updating(progress)
        case .waiting:
            Text("Installed, restarts once its agents finish")
                .font(.ui(.xs))
                .foregroundStyle(Color.themeMutedForeground)
        case .restarting:
            updating("Restarting")
        case .current, .offered:
            EmptyView()
        }
    }

    private func updating(_ text: String) -> some View {
        HStack(spacing: 6) {
            Spinner(size: .xs)
            Text(text)
                .monospacedDigit()
        }
        .font(.ui(.xs))
        .foregroundStyle(Color.themeMutedForeground)
    }
}

/// Where a server's update is, as its dropdown shows it.
enum ServerUpdateStep: Equatable {
    case current
    /// A newer version is out. While agents work, the update waits for them or stops them.
    case offered(busy: Bool)
    /// "Updating 40%".
    case downloading(String)
    /// The new version is installed and starts once the agents have finished.
    case waiting
    case restarting
}

extension AppStore {
    func updateStep(of server: Server) -> ServerUpdateStep {
        if let update = serverUpdate(of: server) {
            if update.restarting { return .restarting }
            if update.waiting { return .waiting }
            return .downloading(update.fraction.map { "Updating \(Int($0 * 100))%" } ?? "Updating")
        }
        guard isOutdated(server) else { return .current }
        return .offered(busy: isBusy(server) && canChooseRestart(server))
    }
}

/// The machine's parts, each a row of its name, how much it is now and its chart, or why they
/// can't be shown.
struct ServerStatsRows: View {
    /// Between the line over the rows and the first.
    static let top: CGFloat = 12
    /// Between one part's chart and the next part's name: clearly more than a name and its chart.
    private static let gap: CGFloat = 20

    @Environment(AppStore.self) private var store
    let server: Server

    var body: some View {
        switch store.serverStats[server.id] ?? .loading {
        case .loading:
            Spinner(size: .md)
                .foregroundStyle(Color.themeMutedForeground)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 24)
        case .failed(let error):
            Text(error)
                .font(.ui(.xs))
                .foregroundStyle(Color.themeMutedForeground)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
        case .shown(let stats):
            VStack(spacing: Self.gap) {
                ForEach(stats.rows) { ServerStatsRow(row: $0) }
            }
        }
    }
}

/// A part of the machine: its symbol, its name and amounts, how much it is now, and under them the
/// last minutes as bars or how full it is.
struct ServerStatsRow: View {
    private static let symbolGap: CGFloat = 5
    /// Between a part's name and its chart, so that they read as one. iOS's larger text needs more.
    #if os(iOS)
    private static let chartGap: CGFloat = 8
    #else
    private static let chartGap: CGFloat = 6
    #endif
    /// A meter is a thin line, so it stands further from its name than a chart's bars do.
    private static let meterGap: CGFloat = chartGap + 4
    /// On iOS as large as the rows of the other sheets.
    #if os(iOS)
    static let font = Font.body
    private static let symbolSize = IconSize.md
    #else
    static let font = Font.ui(.sm)
    private static let symbolSize = IconSize.sm
    #endif

    let row: ServerStats.Row

    var body: some View {
        VStack(spacing: Self.chartGap) {
            HStack(spacing: 0) {
                Image(symbol, size: Self.symbolSize)
                    .foregroundStyle(Color.themeMutedForeground)
                    .padding(.trailing, Self.symbolGap)
                label
                Spacer(minLength: 8)
                value
                    .fontWeight(.medium)
            }
            .font(Self.font)
            .monospacedDigit()
            .lineLimit(1)
            if let fill = row.fill {
                LimitPill(usedPercent: fill.height * 100, color: fill.tone.color)
                    .padding(.top, Self.meterGap - Self.chartGap)
            } else {
                StatsBars(bars: row.bars)
            }
        }
    }

    private var label: Text {
        guard let detail = row.detail else { return muted(row.label) }
        return Text("\(muted(row.label))\(separator(" · "))\(muted(detail))")
    }

    private var value: Text {
        let value = Text(row.value).foregroundStyle(Color.themeForeground)
        guard let used = row.used else { return value }
        return Text("\(muted(used))\(separator(" · "))\(value)")
    }

    private var symbol: Symbol {
        switch row.kind {
        case .cpu: .cpu
        case .memory: .memoryStick
        case .download: .arrowDown
        case .upload: .arrowUp
        case .disk: .hardDrive
        }
    }
}

/// A bar for each sample, standing on a line of dots: one under every place a bar can stand.
private struct StatsBars: View {
    private static let height: CGFloat = 12
    private static let gap: CGFloat = 1

    let bars: [ServerStats.Bar]

    var body: some View {
        Canvas { context, size in
            guard !bars.isEmpty else { return }
            let pitch = size.width / CGFloat(bars.count)
            let width = max(1, pitch - Self.gap)
            for (index, bar) in bars.enumerated() {
                let x = CGFloat(index) * pitch
                let height = bar.height > 0 ? max(1, (CGFloat(bar.height) * size.height).rounded()) : 0
                guard height > 0 else {
                    context.fill(Path(CGRect(x: x, y: size.height - 1, width: width, height: 1)), with: .color(.themeMutedStrongestForeground))
                    continue
                }
                context.fill(Path(CGRect(x: x, y: size.height - height, width: width, height: height)), with: .color(bar.tone.color))
            }
        }
        .frame(height: Self.height)
    }
}

private func muted(_ text: String) -> Text {
    Text(text).foregroundStyle(Color.themeMutedForeground)
}

/// A separator, a muted step behind the words it divides.
private func separator(_ text: String) -> Text {
    Text(verbatim: text).foregroundStyle(Color.themeMutedStrongerForeground)
}
