import Charts
import SwiftUI

/// What the agents spent on the account's servers: a line for each agent over time, and what
/// each model, project and kind of token took.
struct UsageView: View {
    private enum Period: String, CaseIterable, Identifiable {
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

    private enum Measure: String, CaseIterable, Identifiable {
        case cost, tokens

        var id: Self { self }
        var label: String { self == .cost ? "Cost" : "Tokens" }
    }

    private enum Breakdown: String, CaseIterable, Identifiable {
        case models, projects, kinds

        var id: Self { self }

        var label: String {
            switch self {
            case .models: "Models"
            case .projects: "Projects"
            case .kinds: "Tokens"
            }
        }
    }

    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    @AppStorage("usage.period") private var period = Period.week
    @AppStorage("usage.measure") private var measure = Measure.cost
    @AppStorage("usage.breakdown") private var breakdown = Breakdown.models
    @State private var report: Loaded<UsageReport> = .loading
    @State private var pointed: Date?

    /// The servers from before usage was kept.
    private var outdated: [Server] {
        store.servers.filter { $0.state == .connected && $0.protocolVersion < 10 }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            ThemeDivider()
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    ViewThatFits(in: .horizontal) {
                        HStack {
                            periods
                            Spacer(minLength: 12)
                            measures
                        }
                        VStack(alignment: .leading, spacing: 8) {
                            periods
                            measures
                        }
                    }
                    switch report {
                    case .loading:
                        Spinner()
                            .foregroundStyle(Color.themeSecondary)
                            .frame(maxWidth: .infinity, minHeight: 240)
                    case .failed(let message):
                        note(message)
                    case .ready(let report) where report.tokens == 0:
                        note("Nothing was spent in this time. Your servers keep what their agents spend from the version that shows this on.")
                    case .ready(let report):
                        summary(report)
                        chart(report)
                        lines(report)
                    }
                    ForEach(outdated) { server in
                        Text("Update \(server.name) to see what its agents spend.")
                            .font(.caption)
                            .foregroundStyle(Color.themeSecondary)
                    }
                }
                .padding(16)
            }
        }
        #if os(macOS)
        .frame(width: 620, height: 640)
        #else
        .presentationDetents([.large])
        #endif
        .presentationBackground(Color.themeSheet)
        .onAppear(perform: load)
        .onChange(of: period) { load() }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Text("Usage")
                .font(.ui(size: 13, weight: .semibold))
            Spacer()
            ActionButton("Done") { dismiss() }
                .keyboardShortcut(.cancelAction)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
    }

    private var periods: some View {
        Picker("Time", selection: $period) {
            ForEach(Period.allCases) { Text($0.label).tag($0) }
        }
        .pickerStyle(.segmented)
        .labelsHidden()
        .fixedSize()
    }

    private var measures: some View {
        Picker("Measure", selection: $measure) {
            ForEach(Measure.allCases) { Text($0.label).tag($0) }
        }
        .pickerStyle(.segmented)
        .labelsHidden()
        .fixedSize()
    }

    private func load() {
        let asked = period
        report = .loading
        pointed = nil
        store.loadUsage(bucketSeconds: asked.bucketSeconds, buckets: asked.buckets) { result in
            guard asked == period else { return }
            switch result {
            case .success(let loaded): report = .ready(loaded)
            case .failure(let error): report = .failed(error.message)
            }
        }
    }

    private func note(_ text: String) -> some View {
        Text(text)
            .font(.ui(size: 12))
            .foregroundStyle(Color.themeSecondary)
            .multilineTextAlignment(.center)
            .frame(maxWidth: .infinity, minHeight: 240)
    }

    private func summary(_ report: UsageReport) -> some View {
        HStack(alignment: .top, spacing: 16) {
            VStack(alignment: .leading, spacing: 3) {
                Text(measure == .cost ? Self.cost(report.costUSD) : Self.count(report.tokens))
                    .font(.ui(size: 26, weight: .semibold))
                    .foregroundStyle(Color.themeText)
                Text(measure == .cost ? "What the API would have charged" : "Tokens in and out")
                    .font(.caption)
                    .foregroundStyle(Color.themeSecondary)
                if report.writingTokens > 0 {
                    let written = measure == .cost ? Self.cost(report.writingCostUSD) : Self.count(report.writingTokens)
                    Text("\(written) of \(measure == .cost ? "it" : "them") for titles, branch names, commit messages and pull requests")
                        .font(.caption)
                        .foregroundStyle(Color.themeSecondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if measure == .cost && report.unpricedTokens > 0 {
                    Text("Without \(Self.count(report.unpricedTokens)) tokens of models whose price isn't known")
                        .font(.caption)
                        .foregroundStyle(Color.themeTertiary)
                }
            }
            Spacer(minLength: 12)
            if report.cacheSavingsUSD > 0 {
                VStack(alignment: .trailing, spacing: 3) {
                    Text(Self.cost(report.cacheSavingsUSD))
                        .font(.ui(size: 13, weight: .medium))
                        .foregroundStyle(Color.themeText)
                    Text("Saved by the cache")
                        .font(.caption)
                        .foregroundStyle(Color.themeSecondary)
                }
            }
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
    private func legend(_ report: UsageReport) -> some View {
        let index = pointedIndex(in: report)
        return HStack(spacing: 14) {
            ForEach(report.agents) { series in
                HStack(spacing: 6) {
                    Circle()
                        .fill(Self.color(of: series.agent))
                        .frame(width: 8, height: 8)
                    Text(series.agent.name)
                        .foregroundStyle(Color.themeText)
                    Text(amount(index.map { value(of: series, at: $0) } ?? (measure == .cost ? series.costUSD : Double(series.tokens))))
                        .foregroundStyle(Color.themeSecondary)
                        .monospacedDigit()
                }
            }
            Spacer(minLength: 8)
            if let index {
                let start = report.starts[index]
                Text(period == .day ? start.formatted(.dateTime.weekday().hour()) : start.formatted(.dateTime.weekday().month(.abbreviated).day()))
                    .foregroundStyle(Color.themeSecondary)
            }
        }
        .font(.ui(size: 12))
    }

    private func chart(_ report: UsageReport) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            legend(report)
            Chart {
                ForEach(report.agents) { series in
                    ForEach(Array(report.starts.enumerated()), id: \.offset) { index, start in
                        AreaMark(x: .value("Time", start), y: .value("Spent", value(of: series, at: index)), stacking: .unstacked)
                            .foregroundStyle(by: .value("Agent", series.agent.name))
                            .interpolationMethod(.monotone)
                            .opacity(0.12)
                        LineMark(x: .value("Time", start), y: .value("Spent", value(of: series, at: index)))
                            .foregroundStyle(by: .value("Agent", series.agent.name))
                            .interpolationMethod(.monotone)
                            .lineStyle(StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round))
                    }
                }
                if let index = pointedIndex(in: report) {
                    RuleMark(x: .value("Time", report.starts[index]))
                        .foregroundStyle(Color.themeStrongBorder)
                        .lineStyle(StrokeStyle(lineWidth: 1))
                }
            }
            .chartForegroundStyleScale(
                domain: report.agents.map(\.agent.name), range: report.agents.map { Self.color(of: $0.agent) }
            )
            .chartLegend(.hidden)
            .chartXSelection(value: $pointed)
            .chartXScale(range: .plotDimension(startPadding: 16, endPadding: 16))
            .chartXAxis {
                AxisMarks(preset: .aligned, values: .automatic(desiredCount: 5)) { _ in
                    AxisValueLabel(format: period == .day ? .dateTime.hour() : .dateTime.month(.abbreviated).day())
                        .foregroundStyle(Color.themeSecondary)
                }
            }
            .chartYAxis {
                AxisMarks(values: .automatic(desiredCount: 4)) { mark in
                    AxisGridLine().foregroundStyle(Color.themeBorder)
                    AxisValueLabel {
                        if let value = mark.as(Double.self) { Text(amount(value)) }
                    }
                    .foregroundStyle(Color.themeSecondary)
                }
            }
            .frame(height: 190)
        }
    }

    private func pointedIndex(in report: UsageReport) -> Int? {
        guard let pointed, !report.starts.isEmpty else { return nil }
        let nearest = report.starts.enumerated().min { abs($0.element.timeIntervalSince(pointed)) < abs($1.element.timeIntervalSince(pointed)) }
        return nearest?.offset
    }

    private func lines(_ report: UsageReport) -> some View {
        let shown: [UsageReport.Line] =
            switch breakdown {
            case .models: report.models
            case .projects: report.projects
            case .kinds: report.kinds
            }
        return VStack(alignment: .leading, spacing: 8) {
            Picker("By", selection: $breakdown) {
                ForEach(Breakdown.allCases) { Text($0.label).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .fixedSize()
            VStack(spacing: 0) {
                ForEach(shown) { line in
                    UsageLineRow(line: line, measure: measure == .cost ? .cost : .tokens)
                    if line.id != shown.last?.id {
                        ThemeDivider().padding(.horizontal, 12)
                    }
                }
            }
            .background(Color.themeHover, in: RoundedRectangle(cornerRadius: Radius.card, style: .continuous))
        }
    }

    static func color(of agent: Agent) -> Color {
        agent == .claude ? .themeClaudeSeries : .themeCodexSeries
    }

    static func cost(_ value: Double) -> String {
        if value > 0 && value < 0.01 { return "<" + 0.01.formatted(.currency(code: "USD")) }
        return value.formatted(.currency(code: "USD"))
    }

    static func count(_ value: Int) -> String {
        value.formatted(.number.notation(.compactName).precision(.significantDigits(1...3)))
    }
}

/// A model, a project or a kind of token: its name, its part of the whole as a bar, its tokens
/// and what they cost.
private struct UsageLineRow: View {
    enum Measure { case cost, tokens }

    let line: UsageReport.Line
    let measure: Measure
    private static let barWidth: CGFloat = 72

    var body: some View {
        // A row too narrow for the whole name gives up the bar.
        ViewThatFits(in: .horizontal) {
            row(whole: true)
            row(whole: false)
        }
        .font(.ui(size: 12))
        .monospacedDigit()
        .padding(.horizontal, 12)
        .frame(minHeight: scaled(36))
    }

    private func row(whole: Bool) -> some View {
        HStack(spacing: 10) {
            if let agent = line.agent {
                Circle()
                    .fill(UsageView.color(of: agent))
                    .frame(width: 8, height: 8)
            }
            Text(line.name)
                .foregroundStyle(Color.themeText)
                .lineLimit(1)
                .truncationMode(.middle)
                .fixedSize(horizontal: whole, vertical: false)
            Spacer(minLength: 8)
            if whole {
                Capsule()
                    .fill(Color.themeSelected)
                    .frame(width: Self.barWidth, height: 4)
                    .overlay(alignment: .leading) {
                        Capsule()
                            .fill(Color.themeSecondary)
                            .frame(width: Self.barWidth * min(max(line.share, 0), 1), height: 4)
                    }
            }
            Text(UsageView.count(line.tokens))
                .foregroundStyle(measure == .tokens ? Color.themeText : Color.themeSecondary)
                .frame(width: scaled(64), alignment: .trailing)
            Text(line.costUSD.map(UsageView.cost) ?? "No price")
                .foregroundStyle(measure == .cost && line.costUSD != nil ? Color.themeText : Color.themeSecondary)
                .frame(width: scaled(72), alignment: .trailing)
        }
    }
}
