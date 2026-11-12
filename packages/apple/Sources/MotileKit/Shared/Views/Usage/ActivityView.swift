import Charts
import SwiftUI

/// What the user and the agents did on the servers counted, the Activity tab of the usage route:
/// the headline numbers, the work over time, the hours of the week, the clients the user worked
/// from, how the turns went and the features used.
struct ActivityView: View {
    enum Measure: String, CaseIterable, Identifiable {
        case messages, turns, engaged, time

        var id: Self { self }

        var label: String {
            switch self {
            case .messages: "Messages"
            case .turns: "Turns"
            case .engaged: "Your time"
            case .time: "Agent time"
            }
        }
    }

    private enum Breakdown: String, CaseIterable, Identifiable {
        case agents, models, projects

        var id: Self { self }

        var label: String {
            switch self {
            case .agents: "Agents"
            case .models: "Models"
            case .projects: "Projects"
            }
        }
    }

    private enum Who: String, CaseIterable, Identifiable {
        case you, agents

        var id: Self { self }

        var label: String { self == .you ? "You" : "Agents" }
    }

    /// What the heatmap's cells are coloured by.
    enum HourMeasure: String, CaseIterable, Identifiable {
        case time, messages

        var id: Self { self }

        var label: String { self == .time ? "Time spent" : "Messages sent" }
    }

    /// Narrower than this, the tiles go two to a row and the turns' cards under each other.
    static let besideWidth: CGFloat = 640

    @Environment(AppStore.self) private var store
    @Environment(\.colorScheme) private var scheme
    @Environment(\.usagePageHeight) private var pageHeight
    let report: ActivityReport
    let period: UsageModel.Period
    @AppStorage("activity.measure") private var measure = Measure.messages
    @AppStorage("activity.breakdown") private var breakdown = Breakdown.agents
    @AppStorage("activity.who") private var who = Who.you
    @AppStorage("activity.hours") private var hourMeasure = HourMeasure.time
    @State private var pointed: Date?
    @State private var width: CGFloat = 0

    /// The servers from before what is done was kept.
    private var outdated: [Server] {
        store.servers.filter { $0.state == .connected && $0.protocolVersion < 40 }
    }

    private var beside: Bool { width >= Self.besideWidth }

    /// The most a long list shows before it scrolls: half of what the page shows.
    private var listHeight: CGFloat { max(240, pageHeight / 2) }

    var body: some View {
        VStack(alignment: .leading, spacing: 36) {
            if report.empty {
                UsageNote(text: "Nothing was done in this time. Servers keep what is done from the version that shows this on.")
            } else {
                tiles
                overTime
                hours
                if !report.clients.isEmpty { clients }
                if !report.outcomes.isEmpty { turns }
                if !report.features.isEmpty || !report.agentFeatures.isEmpty { features }
            }
            ForEach(outdated) { server in
                Text("Update \(server.name) to see what was done on it.")
                    .font(.ui(.xxs))
                    .foregroundStyle(Color.themeMutedForeground)
            }
        }
        // The width given, not the content's, which would keep a wide layout wide once the page narrows.
        .frame(minWidth: 0, maxWidth: .infinity, alignment: .topLeading)
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
        .onChange(of: period) { pointed = nil }
    }

    // MARK: Tiles

    private var tiles: some View {
        ActivityStatGrid(
            stats: report.tiles.map { ActivityStat(label: $0.label, value: $0.value, detail: $0.detail) },
            columns: beside ? 4 : 2, valueSize: .xl)
    }

    // MARK: Over time

    private var shownSources: [ActivityReport.Source] {
        report.sources.filter { total(of: $0) > 0 }
    }

    private func points(of source: ActivityReport.Source) -> [Double] {
        switch measure {
        case .messages: source.messagePoints
        case .turns: source.turnPoints
        case .engaged: source.engagedPoints
        case .time: source.secondPoints
        }
    }

    private func total(of source: ActivityReport.Source) -> Double {
        switch measure {
        case .messages: Double(source.messages)
        case .turns: Double(source.turns)
        case .engaged: source.engagedSeconds
        case .time: source.seconds
        }
    }

    /// The lines of a chart of durations, where people count time; nil leaves a count's to the chart.
    private var durationTicks: [Double]? {
        switch measure {
        case .messages, .turns: nil
        case .engaged: report.engagedTicks
        case .time: report.secondTicks
        }
    }

    private func amount(_ value: Double) -> String {
        switch measure {
        case .messages, .turns: Int(value.rounded()).formatted()
        case .engaged, .time: Self.duration(value)
        }
    }

    private var overTime: some View {
        VStack(alignment: .leading, spacing: 12) {
            ActivityHeading(title: "Over time", stacked: true) {
                Segmented(Measure.allCases.map { ($0.label, $0) }, selection: $measure, fills: !beside)
            }
            legend
            chart
        }
    }

    /// Each source with its total, or what it did at the time the pointer is over.
    private var legend: some View {
        let index = pointedIndex
        return HStack(alignment: .firstTextBaseline, spacing: 12) {
            FlowRow(spacing: 14) {
                ForEach(shownSources) { source in
                    HStack(spacing: 6) {
                        Circle()
                            .fill(Self.color(source.color, in: scheme))
                            .frame(width: 8, height: 8)
                        Image(source.symbol, size: .xs)
                            .foregroundStyle(Color.themeMutedForeground)
                        Text(source.label)
                            .foregroundStyle(Color.themeForeground)
                        Text(amount(index.map { points(of: source)[safe: $0] ?? 0 } ?? total(of: source)))
                            .foregroundStyle(Color.themeMutedForeground)
                            .monospacedDigit()
                    }
                    .font(.ui(.xs))
                    .lineLimit(1)
                }
            }
            Spacer(minLength: 0)
            if let index {
                let start = report.starts[index]
                Text(period == .day ? start.formatted(.dateTime.weekday().hour()) : start.formatted(.dateTime.weekday().month(.abbreviated).day()))
                    .font(.ui(.xs))
                    .foregroundStyle(Color.themeMutedForeground)
                    .lineLimit(1)
                    .fixedSize()
            }
        }
    }

    private var chart: some View {
        let sources = shownSources
        return Chart {
            ForEach(sources) { source in
                let values = points(of: source)
                ForEach(Array(report.starts.enumerated()), id: \.offset) { index, start in
                    BarMark(
                        x: .value("Time", start, unit: period == .day ? .hour : .day),
                        y: .value(measure.label, values[safe: index] ?? 0)
                    )
                    .foregroundStyle(by: .value("Source", source.label))
                    .cornerRadius(2)
                    .opacity(pointedIndex.map { $0 == index ? 1 : Opacity.disabled.value(for: scheme) } ?? 1)
                }
            }
        }
        .chartForegroundStyleScale(domain: sources.map(\.label), range: sources.map { Self.color($0.color, in: scheme) })
        .chartLegend(.hidden)
        .chartPointing($pointed, starts: report.starts, marks: .bars)
        .chartXAxis {
            AxisMarks(preset: .aligned, values: .automatic(desiredCount: 5)) { _ in
                AxisValueLabel(format: period == .day ? .dateTime.hour() : .dateTime.month(.abbreviated).day())
                    .foregroundStyle(Color.themeMutedForeground)
            }
        }
        .modifier(DurationScale(ticks: durationTicks))
        .chartYAxis {
            if let ticks = durationTicks {
                AxisMarks(values: ticks) { mark in yMark(mark) }
            } else {
                AxisMarks(values: .automatic(desiredCount: 4)) { mark in yMark(mark) }
            }
        }
        .frame(height: 190)
    }

    @AxisMarkBuilder private func yMark(_ mark: AxisValue) -> some AxisMark {
        AxisGridLine().foregroundStyle(Color.themeBorder)
        AxisValueLabel {
            if let value = mark.as(Double.self) { Text(amount(value)) }
        }
        .foregroundStyle(Color.themeMutedForeground)
    }

    private var pointedIndex: Int? {
        pointed.flatMap { ChartMarks.bars.index(at: $0, in: report.starts) }
    }

    // MARK: Clients

    private var clients: some View {
        VStack(alignment: .leading, spacing: 12) {
            ActivityHeading(title: "Clients", detail: "Where you worked from")
            FadingScroll(maxHeight: listHeight) {
                VStack(spacing: 0) {
                    ForEach(report.clients) { client in
                        ActivityClientRow(client: client, beside: beside)
                        if client.id != report.clients.last?.id {
                            ThemeDivider()
                        }
                    }
                }
            }
            .card()
        }
    }

    // MARK: Hours

    private var hours: some View {
        VStack(alignment: .leading, spacing: 12) {
            ActivityHeading(title: "When you work", detail: hourMeasure == .time ? report.busiestTime : report.busiestMessages, stacked: true) {
                Segmented(HourMeasure.allCases.map { ($0.label, $0) }, selection: $hourMeasure, fills: !beside)
            }
            ActivityHeatmap(heatmap: report.heatmap, measure: hourMeasure)
                .padding(16)
                .card()
        }
    }

    // MARK: Turns

    private var turns: some View {
        VStack(alignment: .leading, spacing: 12) {
            ActivityHeading(title: "Turns")
            if beside {
                HStack(alignment: .top, spacing: 12) {
                    outcomes
                        .frame(maxHeight: .infinity, alignment: .top)
                        .card()
                    breakdownCard
                        .frame(maxHeight: .infinity, alignment: .top)
                        .card()
                }
                .fixedSize(horizontal: false, vertical: true)
            } else {
                outcomes.card()
                breakdownCard.card()
            }
            if !report.facts.isEmpty {
                ActivityStatGrid(
                    stats: report.facts.map { ActivityStat(label: $0.label, value: $0.value, detail: $0.detail) },
                    columns: beside ? 4 : 2, valueSize: .lg)
            }
        }
    }

    private var outcomes: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("How they ended")
                .font(.ui(.sm, weight: .medium))
                .foregroundStyle(Color.themeForeground)
                .padding(.horizontal, 14)
                .activityTableHeader(minHeight: Self.breakdownHeaderHeight)
            ThemeDivider()
            ForEach(report.outcomes) { line in
                ActivityLineRow(line: line, tint: Self.outcomeColor(line.name))
                if line.id != report.outcomes.last?.id {
                    ThemeDivider()
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .topLeading)
    }

    private static let breakdownPadding: CGFloat = 4
    /// The breakdown's picker with its padding, which the outcomes' header beside it matches.
    private static var breakdownHeaderHeight: CGFloat {
        ControlSize.regular.height + 2 * (Segmented<Breakdown>.inset + Segmented<Breakdown>.border + breakdownPadding)
    }

    private var breakdownCard: some View {
        let offered = Breakdown.allCases.filter { $0 != .projects || report.projects.count > 1 }
        let current = offered.contains(breakdown) ? breakdown : .agents
        let shown: [ActivityReport.Line] =
            switch current {
            case .agents: report.agents
            case .models: report.models
            case .projects: report.projects
            }
        return VStack(alignment: .leading, spacing: 0) {
            Segmented(offered.map { ($0.label, $0) }, selection: Binding { current } set: { breakdown = $0 }, fills: true)
                .padding(Self.breakdownPadding)
            ThemeDivider()
            FadingScroll(maxHeight: listHeight) {
                VStack(spacing: 0) {
                    ForEach(shown) { line in
                        ActivityLineRow(line: line, tint: nil)
                        if line.id != shown.last?.id {
                            ThemeDivider()
                        }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .topLeading)
    }

    // MARK: Features

    private var features: some View {
        let offered = Who.allCases.filter { $0 == .you ? !report.features.isEmpty : !report.agentFeatures.isEmpty }
        let current = offered.contains(who) ? who : (offered.first ?? .you)
        return VStack(alignment: .leading, spacing: 12) {
            ActivityHeading(title: "Features", stacked: !beside && offered.count > 1) {
                if offered.count > 1 {
                    Segmented(offered.map { ($0.label, $0) }, selection: Binding { current } set: { who = $0 }, fills: !beside)
                }
            }
            switch current {
            case .you:
                if beside {
                    HStack(alignment: .top, spacing: 12) {
                        featureColumn(report.features.filter { $0.column == 0 })
                        featureColumn(report.features.filter { $0.column == 1 })
                    }
                } else {
                    featureColumn(report.features)
                }
            case .agents:
                ActivityFeatureCard(title: "Motile tools", count: report.agentFeatures.reduce(0) { $0 + $1.count }, features: report.agentFeatures, maxHeight: listHeight)
            }
        }
    }

    private func featureColumn(_ groups: [ActivityReport.FeatureGroup]) -> some View {
        VStack(spacing: 12) {
            ForEach(groups) { group in
                ActivityFeatureCard(title: group.name, count: group.count, features: group.features, maxHeight: listHeight)
            }
        }
        .frame(maxWidth: .infinity, alignment: .top)
    }

    // MARK: Colours and words

    /// A source's colour: one of the six chart colours, the agents' colour or its tint, or muted for
    /// clients that didn't say which they are.
    static func color(_ index: Int, in scheme: ColorScheme) -> Color {
        switch index {
        case 1: .themeChart1
        case 2: .themeChart2
        case 3: .themeChart3
        case 4: .themeChart4
        case 5: .themeChart5
        case 6: .themeChart6
        case 7: .themePending
        case 8: .themePending.opacity(Opacity.colorTintChartStronger.value(for: scheme))
        default: .themeMutedForeground
        }
    }

    static func outcomeColor(_ name: String) -> Color {
        switch name {
        case "Finished": .themeSuccess
        case "Stopped": .themeMutedForeground
        case "Hit the usage limit": .themeWarning
        default: .themeDestructive
        }
    }

    static func duration(_ seconds: Double) -> String {
        let seconds = Int(max(seconds, 0).rounded())
        if seconds < 60 { return "\(seconds)s" }
        if seconds < 3600 { return "\(seconds / 60)m" }
        let (hours, minutes) = (seconds / 3600, seconds % 3600 / 60)
        return minutes == 0 || hours >= 100 ? "\(hours.formatted())h" : "\(hours)h \(minutes)m"
    }
}

struct ActivityStat: Identifiable {
    let label: String
    let value: String
    let detail: String

    var id: String { label }
}

/// Numbers in cards, `columns` to a row, every card of a row as tall as its tallest.
private struct ActivityStatGrid: View {
    let stats: [ActivityStat]
    let columns: Int
    let valueSize: TextSize

    var body: some View {
        let rows = stride(from: 0, to: stats.count, by: columns).map { Array(stats[$0..<min($0 + columns, stats.count)]) }
        Grid(horizontalSpacing: 10, verticalSpacing: 10) {
            ForEach(rows, id: \.first?.id) { row in
                GridRow(alignment: .top) {
                    ForEach(row) { stat in card(stat) }
                    ForEach(row.count..<columns, id: \.self) { _ in Color.clear.gridCellUnsizedAxes([.horizontal, .vertical]) }
                }
            }
        }
    }

    private func card(_ stat: ActivityStat) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(stat.label)
                .font(.ui(.xs))
                .foregroundStyle(Color.themeMutedForeground)
                .lineLimit(1)
            Text(stat.value)
                .font(.ui(valueSize, weight: .semibold))
                .foregroundStyle(Color.themeForeground)
                .monospacedDigit()
                .lineLimit(1)
                .fixedSize(horizontal: false, vertical: true)
            Text(stat.detail)
                .font(.ui(.xxs))
                .foregroundStyle(Color.themeMutedStrongerForeground)
                .monospacedDigit()
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .card()
    }
}

/// A section's title, with what it says under it and a control on its right, or `stacked` across
/// the row under it, where a narrow page has no room beside the title.
private struct ActivityHeading<Trailing: View>: View {
    let title: String
    var detail: String?
    var stacked = false
    @ViewBuilder var trailing: Trailing

    /// How far a picker beside the title reaches past the heading, which stays as tall as a small one would make it.
    private static var trailingOverhang: CGFloat { (ControlSize.regular.height - ControlSize.small.height) / 2 }

    var body: some View {
        if stacked {
            VStack(alignment: .leading, spacing: 10) {
                words.padding(.horizontal, 4)
                trailing
            }
        } else {
            HStack(alignment: .center, spacing: 12) {
                words
                Spacer(minLength: 0)
                trailing.padding(.vertical, -Self.trailingOverhang)
            }
            .padding(.horizontal, 4)
        }
    }

    private var words: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.ui(.md, weight: .medium))
                .foregroundStyle(Color.themeForeground)
            if let detail {
                Text(detail)
                    .font(.ui(.xs))
                    .foregroundStyle(Color.themeMutedForeground)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

extension ActivityHeading where Trailing == EmptyView {
    init(title: String, detail: String? = nil) {
        self.init(title: title, detail: detail) { EmptyView() }
    }
}

/// A client: its device, what it is, how it reaches the servers, and its part of your time and messages.
private struct ActivityClientRow: View {
    @Environment(\.surface) private var surface
    @Environment(\.colorScheme) private var scheme
    let client: ActivityReport.Client
    let beside: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .center, spacing: 12) {
                badge
                VStack(alignment: .leading, spacing: 3) {
                    HStack(spacing: 6) {
                        Text(verbatim: client.name)
                            .font(.ui(.sm, weight: .medium))
                            .foregroundStyle(Color.themeForeground)
                            .lineLimit(1)
                            .truncationMode(.middle)
                        if client.thisDevice {
                            Chip("This device", small: true)
                        }
                        if client.removed {
                            Chip("Removed", muted: true, small: true)
                        }
                    }
                    if beside {
                        Text(dotted: ([client.client] + (client.detail.isEmpty ? [] : [client.detail])).map { Text(verbatim: $0) })
                            .lineLimit(1)
                    } else {
                        Text(verbatim: client.client)
                        if !client.detail.isEmpty {
                            Text(verbatim: client.detail)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
                .font(.ui(.xs))
                .foregroundStyle(Color.themeMutedForeground)
                .layoutPriority(1)
                Spacer(minLength: 8)
            }
            VStack(alignment: .leading, spacing: 6) {
                ActivityBar(share: client.share, color: ActivityView.color(client.color, in: scheme))
                HStack {
                    Text(verbatim: client.summary)
                        .foregroundStyle(Color.themeForeground)
                    Spacer(minLength: 8)
                    if let seen {
                        Text(seen)
                            .foregroundStyle(Color.themeMutedStrongerForeground)
                    }
                }
                .font(.ui(.xxs))
                .monospacedDigit()
                .lineLimit(1)
            }
            .padding(.leading, scaled(32) + 12)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
    }

    private var seen: String? {
        guard !client.active else { return "Active now" }
        guard let lastSeen = client.lastSeen else { return nil }
        let ago = Time.ago(lastSeen.timeIntervalSince1970)
        return ago == "now" ? "Seen just now" : "Seen \(ago) ago"
    }

    private var badge: some View {
        let color = ActivityView.color(client.color, in: scheme)
        return Image(client.symbol, size: .md)
            .foregroundStyle(color)
            .frame(width: scaled(32), height: scaled(32))
            .background(RoundedRectangle(cornerRadius: Radius.md, style: .continuous).fill(color.wash()))
    }
}

/// An outcome, an agent, a model, a project or a starter: its name, how long its turns took, and
/// its part of them.
private struct ActivityLineRow: View {
    let line: ActivityReport.Line
    let tint: Color?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                if let agent = line.agent {
                    Circle()
                        .fill(agent.color)
                        .frame(width: 8, height: 8)
                } else if let tint {
                    Circle()
                        .fill(tint)
                        .frame(width: 8, height: 8)
                }
                Text(verbatim: line.name)
                    .foregroundStyle(Color.themeForeground)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if let server = line.server {
                    Text(server)
                        .foregroundStyle(Color.themeMutedForeground)
                        .lineLimit(1)
                }
                Spacer(minLength: 8)
                Text(line.detail)
                    .foregroundStyle(Color.themeMutedForeground)
                    .lineLimit(1)
                Text(line.turns.formatted())
                    .foregroundStyle(Color.themeForeground)
                    .frame(minWidth: scaled(36), alignment: .trailing)
            }
            .font(.ui(.xs))
            .monospacedDigit()
            ActivityBar(share: line.share, color: tint ?? line.agent?.color ?? .themeMutedForeground)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 11)
    }
}

/// The features of a group as a table under a header of its name and total, each with how often
/// it was used, scrolling past `maxHeight`.
private struct ActivityFeatureCard: View {
    let title: String
    let count: Int
    let features: [ActivityReport.Feature]
    let maxHeight: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(title)
                    .foregroundStyle(Color.themeForeground)
                Spacer(minLength: 8)
                Text(count.formatted())
                    .foregroundStyle(Color.themeMutedForeground)
                    .monospacedDigit()
            }
            .font(.ui(.sm, weight: .medium))
            .padding(.horizontal, 14)
            .activityTableHeader()
            ThemeDivider()
            FadingScroll(maxHeight: maxHeight) {
                VStack(spacing: 0) {
                    ForEach(features) { feature in
                        row(feature)
                        if feature.id != features.last?.id {
                            ThemeDivider()
                        }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .topLeading)
        .card()
    }

    private func row(_ feature: ActivityReport.Feature) -> some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                Text(feature.label)
                    .foregroundStyle(Color.themeForeground)
                    .lineLimit(2)
                if !feature.detail.isEmpty {
                    Text(feature.detail)
                        .font(.ui(.xxs))
                        .foregroundStyle(Color.themeMutedForeground)
                        .lineLimit(1)
                }
            }
            .padding(.vertical, 8)
            Spacer(minLength: 8)
            ActivityBar(share: feature.share, color: .themeChart1)
                .frame(width: scaled(56))
            Text(feature.count.formatted())
                .foregroundStyle(Color.themeForeground)
                .frame(minWidth: scaled(32), alignment: .trailing)
        }
        .font(.ui(.xs))
        .monospacedDigit()
        .padding(.horizontal, 14)
        .frame(minHeight: scaled(40))
    }
}

/// A part of the whole as a thin bar across what it is given.
private struct ActivityBar: View {
    @Environment(\.surface) private var surface
    let share: Double
    let color: Color

    var body: some View {
        Capsule()
            .fill(surface.color(.control))
            .frame(height: 4)
            .overlay(alignment: .leading) {
                GeometryReader { track in
                    Capsule()
                        .fill(color)
                        .frame(width: max(track.size.width * min(max(share, 0), 1), share > 0 ? 4 : 0))
                }
            }
    }
}

/// When the work was done, each cell as busy as its colour is strong. The cell the pointer is over,
/// the one a finger slides onto or the last one tapped says what was done in it. The gaps between
/// the cells are only drawn: each cell's hit area reaches halfway into them.
private struct ActivityHeatmap: View {
    private static let gap: CGFloat = 3

    @Environment(\.surface) private var surface
    let heatmap: ActivityReport.Heatmap
    let measure: ActivityView.HourMeasure
    @State private var chosen: Int?
    @State private var width: CGFloat = 0

    /// A cell and the gap after it.
    private var pitch: CGFloat { (width + Self.gap) / CGFloat(max(heatmap.columns, 1)) }
    /// As tall as wide up to a limit, and never shorter than its row's name.
    private var cellHeight: CGFloat { max(min(max(pitch - Self.gap, 4), scaled(18)), Self.labelLine) }
    private static let labelLine = PlatformFont.ui(.xxs).textLineHeight.rounded(.up)
    /// Small cells round less, so that they stay squares and don't turn into dots.
    private var corner: CGFloat { min(Radius.xs, min(pitch - Self.gap, cellHeight) / 4) }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top, spacing: 8) {
                if !heatmap.rows.isEmpty {
                    VStack(alignment: .leading, spacing: Self.gap) {
                        ForEach(heatmap.rows.indices, id: \.self) { row in
                            Text(heatmap.rows[row])
                                .font(.ui(.xxs))
                                .foregroundStyle(Color.themeMutedForeground)
                                .lineLimit(1)
                                .frame(height: cellHeight)
                        }
                    }
                    .fixedSize()
                }
                VStack(alignment: .leading, spacing: 6) {
                    cells
                    axis
                }
                .frame(minWidth: 0, maxWidth: .infinity, alignment: .leading)
                .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
            }
            caption
        }
    }

    private var cells: some View {
        VStack(spacing: 0) {
            ForEach(0..<heatmap.rowCount, id: \.self) { row in
                HStack(spacing: 0) {
                    ForEach(0..<heatmap.columns, id: \.self) { column in
                        cell(row * heatmap.columns + column)
                    }
                }
            }
        }
        .frame(width: width + Self.gap)
        #if os(iOS)
        .gesture(ScrubPan { slid(to: $0) })
        .sensoryFeedback(.selection, trigger: chosen) { _, new in new != nil }
        #endif
        .padding(-Self.gap / 2)
    }

    private func slid(to location: CGPoint) {
        guard location.x >= 0, location.y >= 0 else { return }
        let column = Int(location.x / pitch)
        let row = Int(location.y / (cellHeight + Self.gap))
        guard column < heatmap.columns, row < heatmap.rowCount else { return }
        let index = row * heatmap.columns + column
        guard (heatmap.cells[safe: index] ?? nil) != nil else { return }
        chosen = index
    }

    @ViewBuilder private func cell(_ index: Int) -> some View {
        let slot = CGSize(width: pitch, height: cellHeight + Self.gap)
        if let cell = heatmap.cells[safe: index] ?? nil {
            let isChosen = chosen == index
            RoundedRectangle(cornerRadius: corner, style: .continuous)
                .fill(fill(measure == .time ? cell.timeLevel : cell.messageLevel))
                .overlay {
                    if isChosen {
                        RoundedRectangle(cornerRadius: corner, style: .continuous)
                            .strokeBorder(Color.themeForeground, lineWidth: 1)
                    }
                }
                .padding(Self.gap / 2)
                .frame(width: slot.width, height: slot.height)
                .contentShape(Rectangle())
                .onHover { inside in
                    if inside { chosen = index } else if chosen == index { chosen = nil }
                }
                .onTapGesture { chosen = isChosen ? nil : index }
        } else {
            Color.clear.frame(width: slot.width, height: slot.height)
        }
    }

    /// Each label from the left edge of its column.
    private var axis: some View {
        ZStack(alignment: .topLeading) {
            Text(verbatim: " ").font(.ui(.xxs)).hidden()
            ForEach(heatmap.axis) { label in
                Text(label.label)
                    .font(.ui(.xxs))
                    .foregroundStyle(Color.themeMutedForeground)
                    .lineLimit(1)
                    .fixedSize()
                    .offset(x: CGFloat(label.column) * pitch)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func fill(_ level: Int) -> AnyShapeStyle {
        switch level {
        case 0: AnyShapeStyle(surface.color(.control))
        case 1: AnyShapeStyle(Color.themeChart1.at(.colorTintChart))
        case 2: AnyShapeStyle(Color.themeChart1.at(.colorTintChartStronger))
        case 3: AnyShapeStyle(Color.themeChart1.at(.colorTintChartStrongest))
        default: AnyShapeStyle(Color.themeChart1)
        }
    }

    /// What the chosen cell holds, or how to choose one, and the key to the colours: beside each
    /// other, or the key under it where they don't fit.
    private var caption: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 8) {
                said
                Spacer(minLength: 8)
                key
            }
            VStack(alignment: .leading, spacing: 8) {
                said
                key
            }
        }
        .font(.ui(.xxs))
        .monospacedDigit()
    }

    @ViewBuilder private var said: some View {
        if let chosen, let cell = heatmap.cells[safe: chosen] ?? nil {
            let time = cell.time.isEmpty ? [] : [Text(verbatim: cell.time)]
            Text(dotted: [Text(verbatim: cell.label)] + time + [
                Text(cell.messages == 1 ? "1 message" : "\(cell.messages.formatted()) messages"),
                Text(cell.turns == 1 ? "1 turn" : "\(cell.turns.formatted()) turns"),
            ])
            .foregroundStyle(Color.themeMutedForeground)
            .fixedSize()
        } else {
            Text(verbatim: "\(Platform.hoverReveals ? "Point at" : "Tap") \(heatmap.unit) to see what was done in it")
                .foregroundStyle(Color.themeMutedStrongerForeground)
                .fixedSize()
        }
    }

    private var key: some View {
        HStack(spacing: Self.gap) {
            Text("Less")
                .foregroundStyle(Color.themeMutedForeground)
                .padding(.trailing, 3)
            ForEach(0..<5, id: \.self) { level in
                RoundedRectangle(cornerRadius: min(Radius.xs, 10 / 4), style: .continuous)
                    .fill(fill(level))
                    .frame(width: 10, height: 10)
            }
            Text("More")
                .foregroundStyle(Color.themeMutedForeground)
                .padding(.leading, 3)
        }
        .fixedSize()
    }
}

private extension View {
    /// The header row of a card that is a table, filled as a table's header is.
    func activityTableHeader(minHeight: CGFloat = scaled(40)) -> some View {
        frame(maxWidth: .infinity, minHeight: minHeight, alignment: .leading)
            .background(
                Color.themeCardAccent,
                in: UnevenRoundedRectangle(topLeadingRadius: Radius.lg, topTrailingRadius: Radius.lg, style: .continuous))
    }
}

private extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}

/// A chart of durations ends at its last line, so that its lines fall where people count time.
private struct DurationScale: ViewModifier {
    let ticks: [Double]?

    func body(content: Content) -> some View {
        if let top = ticks?.last, top > 0 {
            content.chartYScale(domain: 0...top)
        } else {
            content
        }
    }
}
