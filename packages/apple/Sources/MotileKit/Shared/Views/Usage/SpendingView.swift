import Charts
import SwiftUI

/// What the agents spent on the servers counted, the Cost and Tokens tabs of the usage route:
/// the total and each agent's part beside a line for each agent over time, then what each model,
/// project, server, account and kind of token took.
struct SpendingView: View {
    enum Measure { case cost, tokens }

    private enum Breakdown: String, CaseIterable, Identifiable {
        case models, projects, servers, accounts, kinds

        var id: Self { self }

        var label: String {
            switch self {
            case .models: "Models"
            case .projects: "Projects"
            case .servers: "Servers"
            case .accounts: "Accounts"
            case .kinds: "Tokens"
            }
        }
    }

    /// Narrower than this, the chart goes under the totals.
    private static let besideWidth: CGFloat = 640
    /// Narrower than this, a row's bar goes under its name and numbers.
    private static let stackWidth: CGFloat = 520

    @Environment(AppStore.self) private var store
    @Environment(\.colorScheme) private var scheme
    let report: UsageReport
    let measure: Measure
    let period: UsageModel.Period
    @AppStorage("usage.breakdown") private var breakdown = Breakdown.models
    @State private var pointed: Date?
    @State private var width: CGFloat = 0

    /// The servers from before usage was kept.
    private var outdated: [Server] {
        store.servers.filter { $0.state == .connected && $0.protocolVersion < 10 }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 24) {
            if report.tokens == 0 {
                UsageNote(text: "Nothing was spent in this time. Your servers keep what their agents spend from the version that shows this on.")
            } else {
                if width >= Self.besideWidth {
                    HStack(alignment: .top, spacing: 28) {
                        summary
                            .frame(width: 240, alignment: .leading)
                        chart
                    }
                } else {
                    summary
                    chart
                }
                lines
            }
            ForEach(outdated) { server in
                Text("Update \(server.name) to see what its agents spend.")
                    .font(.caption)
                    .foregroundStyle(Color.themeMutedForeground)
            }
        }
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
        .onChange(of: period) { pointed = nil }
    }

    private var summary: some View {
        VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 3) {
                Text(measure == .cost ? Self.cost(report.costUSD) : Self.count(report.tokens))
                    .font(.ui(size: 28, weight: .semibold))
                    .foregroundStyle(Color.themeForeground)
                    .monospacedDigit()
                Text(measure == .cost ? "What the API would have charged" : "Tokens in and out")
                    .font(.caption)
                    .foregroundStyle(Color.themeMutedForeground)
                if report.writingTokens > 0 {
                    let written = measure == .cost ? Self.cost(report.writingCostUSD) : Self.count(report.writingTokens)
                    Text("\(written) of \(measure == .cost ? "it" : "them") for titles, branch names, commit messages and pull requests")
                        .font(.caption)
                        .foregroundStyle(Color.themeMutedStrongerForeground)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if measure == .cost && report.unpricedTokens > 0 {
                    Text("Without \(Self.count(report.unpricedTokens)) tokens of models whose price isn’t known")
                        .font(.caption)
                        .foregroundStyle(Color.themeMutedStrongerForeground)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            VStack(alignment: .leading, spacing: 10) {
                ForEach(report.agents) { series in
                    agentLine(series)
                }
                if report.cacheSavingsUSD > 0 {
                    HStack {
                        Text("Saved by the cache")
                            .foregroundStyle(Color.themeMutedForeground)
                        Spacer(minLength: 8)
                        Text(Self.cost(report.cacheSavingsUSD))
                            .foregroundStyle(Color.themeForeground)
                            .monospacedDigit()
                    }
                    .font(.ui(size: 12))
                }
            }
        }
    }

    /// An agent's part: what it spent, and its share of the whole with the other measure.
    private func agentLine(_ series: UsageReport.Series) -> some View {
        let whole = measure == .cost ? report.costUSD : Double(report.tokens)
        let part = measure == .cost ? series.costUSD : Double(series.tokens)
        let share = whole > 0 ? part / whole : 0
        let other = measure == .cost ? "\(Self.count(series.tokens)) tokens" : Self.cost(series.costUSD)
        return VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 7) {
                Circle()
                    .fill(series.agent.color)
                    .frame(width: 8, height: 8)
                AgentIcon(agent: series.agent, size: 14)
                Text(series.agent.name)
                    .foregroundStyle(Color.themeForeground)
                Spacer(minLength: 8)
                Text(amount(part))
                    .foregroundStyle(Color.themeForeground)
                    .monospacedDigit()
            }
            .font(.ui(size: 12))
            Text("\(share.formatted(.percent.precision(.fractionLength(0...1)))) of \(measure == .cost ? "the cost" : "the tokens") · \(other)")
                .font(.ui(size: 11))
                .foregroundStyle(Color.themeMutedStrongerForeground)
                .monospacedDigit()
                .padding(.leading, 15)
        }
    }

    private func value(of series: UsageReport.Series, at index: Int) -> Double {
        let points = measure == .cost ? series.costPoints : series.tokenPoints
        return points.indices.contains(index) ? points[index] : 0
    }

    private func amount(_ value: Double) -> String {
        measure == .cost ? Self.cost(value) : Self.count(Int(value))
    }

    /// The agents with what each spent: in all, or at the time the pointer is over.
    private var legend: some View {
        let index = pointedIndex
        return HStack(spacing: 14) {
            ForEach(report.agents) { series in
                HStack(spacing: 6) {
                    Circle()
                        .fill(series.agent.color)
                        .opacity(.colorTintChart)
                        .frame(width: 8, height: 8)
                    Text(series.agent.name)
                        .foregroundStyle(Color.themeForeground)
                    Text(amount(index.map { value(of: series, at: $0) } ?? (measure == .cost ? series.costUSD : Double(series.tokens))))
                        .foregroundStyle(Color.themeMutedForeground)
                        .monospacedDigit()
                }
            }
            Spacer(minLength: 8)
            if let index {
                let start = report.starts[index]
                Text(period == .day ? start.formatted(.dateTime.weekday().hour()) : start.formatted(.dateTime.weekday().month(.abbreviated).day()))
                    .foregroundStyle(Color.themeMutedForeground)
            }
        }
        .font(.ui(size: 12))
    }

    private var chart: some View {
        VStack(alignment: .leading, spacing: 10) {
            legend
            Chart {
                ForEach(report.agents) { series in
                    ForEach(Array(report.starts.enumerated()), id: \.offset) { index, start in
                        AreaMark(x: .value("Time", start), y: .value("Spent", value(of: series, at: index)), stacking: .unstacked)
                            .foregroundStyle(by: .value("Agent", series.agent.name))
                            .interpolationMethod(.monotone)
                            .opacity(Opacity.colorTint.value(for: scheme))
                        LineMark(x: .value("Time", start), y: .value("Spent", value(of: series, at: index)))
                            .foregroundStyle(by: .value("Agent", series.agent.name))
                            .interpolationMethod(.monotone)
                            .lineStyle(StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round))
                            .opacity(Opacity.colorTintChart.value(for: scheme))
                    }
                }
                if let index = pointedIndex {
                    RuleMark(x: .value("Time", report.starts[index]))
                        .foregroundStyle(Color.themeBorder)
                        .lineStyle(StrokeStyle(lineWidth: 1))
                }
            }
            .chartForegroundStyleScale(
                domain: report.agents.map(\.agent.name), range: report.agents.map { $0.agent.color }
            )
            .chartLegend(.hidden)
            .chartXSelection(value: $pointed)
            .chartXScale(range: .plotDimension(startPadding: 16, endPadding: 16))
            .chartXAxis {
                AxisMarks(preset: .aligned, values: .automatic(desiredCount: 5)) { _ in
                    AxisValueLabel(format: period == .day ? .dateTime.hour() : .dateTime.month(.abbreviated).day())
                        .foregroundStyle(Color.themeMutedForeground)
                }
            }
            .chartYAxis {
                AxisMarks(values: .automatic(desiredCount: 4)) { mark in
                    AxisGridLine().foregroundStyle(Color.themeBorder)
                    AxisValueLabel {
                        if let value = mark.as(Double.self) { Text(amount(value)) }
                    }
                    .foregroundStyle(Color.themeMutedForeground)
                }
            }
            .frame(height: 190)
        }
    }

    private var pointedIndex: Int? {
        guard let pointed, !report.starts.isEmpty else { return nil }
        let nearest = report.starts.enumerated().min { abs($0.element.timeIntervalSince(pointed)) < abs($1.element.timeIntervalSince(pointed)) }
        return nearest?.offset
    }

    private var lines: some View {
        // Accounts are offered once an agent has more than one.
        let offered = Breakdown.allCases.filter { $0 != .accounts || !report.accounts.isEmpty }
        let current = offered.contains(breakdown) ? breakdown : .models
        let shown: [UsageReport.Line] =
            switch current {
            case .models: report.models
            case .projects: report.projects
            case .servers: report.servers
            case .accounts: report.accounts
            case .kinds: report.kinds
            }
        let stacked = width < Self.stackWidth
        return VStack(alignment: .leading, spacing: 8) {
            Segmented(offered.map { ($0.label, $0) }, selection: Binding { current } set: { breakdown = $0 }, fills: stacked)
            VStack(spacing: 0) {
                ForEach(shown) { line in
                    UsageLineRow(line: line, measure: measure, stacked: stacked)
                    if line.id != shown.last?.id {
                        ThemeDivider()
                    }
                }
            }
            .card()
        }
    }

    static func cost(_ value: Double) -> String {
        if value > 0 && value < 0.01 { return "<" + 0.01.formatted(.currency(code: "USD")) }
        return value.formatted(.currency(code: "USD"))
    }

    static func count(_ value: Int) -> String {
        value.formatted(.number.notation(.compactName).precision(.significantDigits(1...3)))
    }
}

/// A model, a project, a server, an account or a kind of token: its name, its part of the whole as a bar, its
/// tokens and what they cost. Stacked, the bar runs under the rest across the row.
private struct UsageLineRow: View {
    @Environment(\.surface) private var surface

    let line: UsageReport.Line
    let measure: SpendingView.Measure
    let stacked: Bool
    private static let barWidth: CGFloat = 72
    private static let dotWidth: CGFloat = 8
    private static let spacing: CGFloat = 10

    var body: some View {
        Group {
            if stacked {
                VStack(alignment: .leading, spacing: 8) {
                    row(whole: false, bar: false)
                    bar(width: nil)
                        .padding(.leading, line.agent == nil ? 0 : Self.dotWidth + Self.spacing)
                }
                .padding(.vertical, 11)
            } else {
                // A row too narrow for the whole name gives up the bar.
                ViewThatFits(in: .horizontal) {
                    row(whole: true, bar: true)
                    row(whole: false, bar: false)
                }
                .frame(minHeight: scaled(36))
            }
        }
        .font(.ui(size: 12))
        .monospacedDigit()
        .padding(.horizontal, 12)
    }

    private func row(whole: Bool, bar shown: Bool) -> some View {
        HStack(spacing: Self.spacing) {
            if let agent = line.agent {
                Circle()
                    .fill(agent.color)
                    .frame(width: Self.dotWidth, height: Self.dotWidth)
            }
            Text(line.name)
                .foregroundStyle(Color.themeForeground)
                .lineLimit(1)
                .truncationMode(.middle)
                .fixedSize(horizontal: whole, vertical: false)
            if let server = line.server {
                Text(server)
                    .foregroundStyle(Color.themeMutedForeground)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .fixedSize(horizontal: whole, vertical: false)
            }
            Spacer(minLength: 8)
            if shown {
                bar(width: Self.barWidth)
            }
            Text(SpendingView.count(line.tokens))
                .foregroundStyle(measure == .tokens ? Color.themeForeground : Color.themeMutedForeground)
                .frame(width: scaled(64), alignment: .trailing)
            Text(line.costUSD.map(SpendingView.cost) ?? "No price")
                .foregroundStyle(measure == .cost && line.costUSD != nil ? Color.themeForeground : Color.themeMutedForeground)
                .frame(width: scaled(72), alignment: .trailing)
        }
    }

    /// The line's share of the whole, `width` wide or as wide as it is given.
    private func bar(width: CGFloat?) -> some View {
        Capsule()
            .fill(surface.color(.control))
            .frame(width: width, height: 4)
            .overlay(alignment: .leading) {
                GeometryReader { track in
                    Capsule()
                        .fill(Color.themeMutedForeground)
                        .frame(width: track.size.width * min(max(line.share, 0), 1))
                }
            }
    }
}
