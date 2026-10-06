import SwiftUI

/// How much of their plans the agents' logins have used: a section for each login, a card for
/// each of its windows with a bar of what was used.
struct LimitsView: View {
    /// Narrower than this, a card puts its bar under its numbers.
    private static let besideWidth: CGFloat = 520

    let report: LimitsReport
    @State private var width: CGFloat = 0

    var body: some View {
        VStack(alignment: .leading, spacing: 28) {
            if report.sections.isEmpty && report.notes.isEmpty {
                UsageNote(text: "No agent is installed on these servers.")
            }
            ForEach(report.sections) { section in
                VStack(alignment: .leading, spacing: 10) {
                    header(section)
                    if let note = section.note {
                        Text(note)
                            .font(.ui(size: 12))
                            .foregroundStyle(Color.themeSecondary)
                            .fixedSize(horizontal: false, vertical: true)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(16)
                            .card()
                    }
                    ForEach(section.windows) { window in
                        LimitCard(window: window, agent: section.agent, beside: width >= Self.besideWidth)
                    }
                }
            }
            ForEach(report.notes, id: \.self) { note in
                Text(note)
                    .font(.caption)
                    .foregroundStyle(Color.themeSecondary)
            }
        }
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
    }

    private func header(_ section: LimitsReport.Section) -> some View {
        HStack(spacing: 8) {
            AgentIcon(agent: section.agent, size: 16)
            Text(section.agent.name)
                .font(.ui(size: 14, weight: .medium))
                .foregroundStyle(Color.themeText)
            if let plan = section.plan {
                Chip(plan)
            }
            if let account = section.account {
                Text(account)
                    .font(.ui(size: 12))
                    .foregroundStyle(Color.themeSecondary)
                    .truncationMode(.middle)
            }
            Spacer(minLength: 0)
        }
        .lineLimit(1)
        .padding(.horizontal, 4)
        .padding(.bottom, 4)
    }
}

/// One window of a plan: what was used of it, how that compares with the time passed, and a bar
/// with when it starts over.
private struct LimitCard: View {
    let window: LimitsReport.Window
    let agent: Agent
    let beside: Bool

    var body: some View {
        Group {
            if beside {
                HStack(spacing: 24) {
                    numbers
                        .frame(width: 168, alignment: .leading)
                    LimitBar(window: window, color: color)
                }
            } else {
                VStack(alignment: .leading, spacing: 12) {
                    numbers
                    LimitBar(window: window, color: color)
                }
            }
        }
        .padding(16)
        .card()
    }

    private var color: Color {
        window.warning ? .themeWarning : agent == .claude ? .themeClaudeSeries : .themeCodexSeries
    }

    private var numbers: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(window.label)
                .font(.ui(size: 13, weight: .medium))
                .foregroundStyle(Color.themeText)
                .lineLimit(1)
            HStack(alignment: .firstTextBaseline, spacing: 5) {
                Text(window.used)
                    .font(.ui(size: 28, weight: .semibold))
                    .foregroundStyle(Color.themeText)
                    .monospacedDigit()
                Text("used")
                    .font(.ui(size: 13))
                    .foregroundStyle(Color.themeSecondary)
            }
            if let pace = window.pace {
                HStack(spacing: 5) {
                    Image(Self.symbol(of: pace), size: 12)
                    Text(Self.label(of: pace))
                        .font(.ui(size: 12))
                }
                .foregroundStyle(Color.themeTertiary)
                .help(Self.help(of: pace))
            }
        }
    }

    private static func symbol(of pace: LimitsReport.Window.Pace) -> Symbol {
        switch pace {
        case .ahead: .trendingUp
        case .on: .gauge
        case .under: .trendingDown
        }
    }

    private static func label(of pace: LimitsReport.Window.Pace) -> String {
        switch pace {
        case .ahead: "Ahead of pace"
        case .on: "On pace"
        case .under: "Under pace"
        }
    }

    private static func help(of pace: LimitsReport.Window.Pace) -> String {
        switch pace {
        case .ahead: "Used faster than the window passes"
        case .on: "Used as fast as the window passes"
        case .under: "Room left for the rest of the window"
        }
    }
}

/// What was used of a window as the part of a bar it fills, and at its end when the window starts
/// over and the resets the login may use.
private struct LimitBar: View {
    private static let height: CGFloat = 28

    @Environment(\.surface) private var surface
    let window: LimitsReport.Window
    let color: Color

    var body: some View {
        RoundedRectangle(cornerRadius: Radius.control, style: .continuous)
            .fill(surface.next.color)
            .overlay(alignment: .leading) {
                GeometryReader { bar in
                    RoundedRectangle(cornerRadius: Radius.control, style: .continuous)
                        .fill(color.opacity(0.45))
                        .frame(width: bar.size.width * window.usedPercent / 100)
                }
            }
            .overlay {
                HStack(spacing: 8) {
                    Text(window.used)
                        .font(.ui(size: 11, weight: .semibold))
                        .foregroundStyle(Color.themeText)
                        .monospacedDigit()
                    Spacer(minLength: 0)
                    plate
                }
                .padding(.leading, 9)
                .padding(.trailing, 4)
            }
            .frame(height: scaled(Self.height))
    }

    @ViewBuilder private var plate: some View {
        if window.resetsIn != nil || window.resetCredits > 0 {
            HStack(spacing: 6) {
                if let resetsIn = window.resetsIn {
                    Text(resetsIn)
                        .help("Starts over in \(resetsIn)")
                }
                if window.resetsIn != nil, window.resetCredits > 0 {
                    Text("·")
                        .foregroundStyle(Color.themeTertiary)
                }
                if window.resetCredits > 0 {
                    HStack(spacing: 3) {
                        Image(.ticket, size: 11)
                        Text("\(window.resetCredits)")
                    }
                    .help(window.resetCredits == 1 ? "1 reset to use" : "\(window.resetCredits) resets to use")
                }
            }
            .font(.ui(size: 11, weight: .medium))
            .foregroundStyle(Color.themeText)
            .monospacedDigit()
            .padding(.horizontal, 7)
            .frame(height: scaled(Self.height) - 8)
            .background(surface.next.color, in: RoundedRectangle(cornerRadius: Radius.small, style: .continuous))
        }
    }
}
