import SwiftUI

/// How much of their plans the agents' logins have used: a section for each login, its windows
/// in one card, each with a bar of what was used.
struct LimitsView: View {
    /// Narrower than this, a window puts its bar under its numbers.
    private static let besideWidth: CGFloat = 520

    let report: LimitsReport
    @State private var width: CGFloat = 0

    var body: some View {
        VStack(alignment: .leading, spacing: 40) {
            if report.sections.isEmpty && report.notes.isEmpty {
                UsageNote(text: "No agent is installed on these servers.")
            }
            ForEach(report.sections) { section in
                VStack(alignment: .leading, spacing: 10) {
                    header(section)
                    if let note = section.note {
                        Text(note)
                            .font(.ui(size: 12))
                            .foregroundStyle(Color.themeMutedForeground)
                            .fixedSize(horizontal: false, vertical: true)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(16)
                            .card()
                    }
                    if !section.windows.isEmpty {
                        VStack(spacing: 0) {
                            ForEach(section.windows) { window in
                                LimitRow(window: window, beside: width >= Self.besideWidth)
                                if window.id != section.windows.last?.id {
                                    ThemeDivider()
                                }
                            }
                        }
                        .card()
                    }
                }
            }
            ForEach(report.notes, id: \.self) { note in
                Text(note)
                    .font(.caption)
                    .foregroundStyle(Color.themeMutedForeground)
            }
        }
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
    }

    private func header(_ section: LimitsReport.Section) -> some View {
        HStack(spacing: 8) {
            AgentIcon(agent: section.agent, size: 16)
            Text(section.agent.name)
                .font(.ui(size: 14, weight: .medium))
                .foregroundStyle(Color.themeForeground)
            if let plan = section.plan {
                Chip(plan)
            }
            login(section)
                .font(.ui(size: 12))
                .foregroundStyle(Color.themeMutedForeground)
                .truncationMode(.middle)
            Spacer(minLength: 0)
        }
        .lineLimit(1)
        .padding(.horizontal, 4)
        .padding(.bottom, 2)
    }

    /// "Work · a@b.c", or whichever of the two is known.
    private func login(_ section: LimitsReport.Section) -> Text {
        switch (section.name, section.account) {
        case let (name?, account?):
            return Text(name) + Text(" · ").foregroundStyle(Color.themeMutedStrongerForeground) + Text(account)
        case let (name?, nil):
            return Text(name)
        case let (nil, account?):
            return Text(account)
        case (nil, nil):
            return Text("")
        }
    }
}

/// One window of a plan: what was used of it, how that compares with the time passed, and a bar
/// with when it starts over.
private struct LimitRow: View {
    let window: LimitsReport.Window
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
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
    }

    private var color: Color {
        switch window.tone {
        case .success: .themeSuccess
        case .pending: .themePending
        case .warning: .themeWarning
        }
    }

    private var numbers: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(window.label)
                .font(.ui(size: 13, weight: .medium))
                .foregroundStyle(Color.themeForeground)
                .lineLimit(1)
            HStack(alignment: .firstTextBaseline, spacing: 5) {
                Text(window.used)
                    .font(.ui(size: 28, weight: .semibold))
                    .foregroundStyle(Color.themeForeground)
                    .monospacedDigit()
                Text("used")
                    .font(.ui(size: 13))
                    .foregroundStyle(Color.themeMutedForeground)
            }
            if let pace = window.pace {
                HStack(spacing: 5) {
                    Image(Self.symbol(of: pace), size: 12)
                    Text(Self.label(of: pace))
                        .font(.ui(size: 12))
                }
                .foregroundStyle(Color.themeMutedStrongerForeground)
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
/// over and the resets the login may use. The text is drawn in the fill's foreground where it lies
/// on the fill and the page's elsewhere.
private struct LimitBar: View {
    private static let height: CGFloat = 28

    @Environment(\.surface) private var surface
    let window: LimitsReport.Window
    let color: Color

    var body: some View {
        GeometryReader { bar in
            let filled = bar.size.width * window.usedPercent / 100
            RoundedRectangle(cornerRadius: Radius.md, style: .continuous)
                .fill(surface.color(.control))
                .overlay(alignment: .leading) {
                    RoundedRectangle(cornerRadius: Radius.md, style: .continuous)
                        .fill(color)
                        .frame(width: filled)
                }
                .overlay {
                    HStack(spacing: 8) {
                        onFill(used, filled: filled)
                        Spacer(minLength: 0)
                        onFill(end, filled: filled)
                    }
                    .padding(.horizontal, 9)
                    .monospacedDigit()
                }
                .coordinateSpace(name: "bar")
        }
        .frame(height: scaled(Self.height))
    }

    private func onFill(_ text: some View, filled: CGFloat) -> some View {
        text.foregroundStyle(Color.themeForeground)
            .overlay {
                text.foregroundStyle(fillForeground)
                    .mask(alignment: .leading) {
                        GeometryReader { text in
                            Rectangle().frame(width: max(0, filled - text.frame(in: .named("bar")).minX))
                        }
                    }
            }
    }

    private var used: some View {
        Text(window.used).font(.ui(size: 11, weight: .semibold))
    }

    @ViewBuilder private var end: some View {
        HStack(spacing: 6) {
            if let resetsIn = window.resetsIn {
                Text(resetsIn)
                    .help("Starts over in \(resetsIn)")
            }
            if window.resetsIn != nil, window.resetCredits > 0 {
                Text("·")
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
    }

    private var fillForeground: Color {
        switch window.tone {
        case .success: .themeSuccessForeground
        case .pending: .themePendingForeground
        case .warning: .themeWarningForeground
        }
    }
}
