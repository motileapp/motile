import MotileShared
import SwiftUI

/// "2 working · 1 needs you", its separators one muted step weaker than its words.
struct WorkSummary: View {
    let work: AgentsWork
    var size = WidgetTheme.TextSize.md

    var body: some View {
        work.summaryParts.enumerated().reduce(Text("")) { line, part in
            let words = Text(part.element).foregroundStyle(part.offset == 0 && work.active > 0 ? WidgetTheme.foreground : WidgetTheme.mutedForeground)
            guard part.offset > 0 else { return line + words }
            return line + Text(" · ").foregroundStyle(WidgetTheme.mutedStrongerForeground) + words
        }
        .font(.widget(size, weight: .semibold))
        .lineLimit(1)
    }
}

/// A thread in the work, on one line: a dot of its state, its title, then its project. The title
/// gives way first.
struct WorkRow: View {
    static let dot: CGFloat = 7

    let item: AgentsWork.Item
    var stale = false
    var showsProject = true

    var body: some View {
        HStack(spacing: 6) {
            Circle()
                .fill(WidgetTheme.color(of: item.state))
                .frame(width: Self.dot, height: Self.dot)
            HStack(spacing: 0) {
                Text(item.title)
                    .foregroundStyle(WidgetTheme.foreground)
                if showsProject, !item.project.isEmpty {
                    (Text(" · ").foregroundStyle(WidgetTheme.mutedStrongerForeground)
                        + Text(item.project).foregroundStyle(WidgetTheme.mutedForeground))
                        .fixedSize()
                }
            }
            .font(.widget(.sm, weight: .medium))
            .lineLimit(1)
        }
        .opacity(stale ? WidgetTheme.disabled : 1)
    }
}

extension AgentsWork {
    /// What is still worth showing at `date`: what ended over 15 minutes before is gone.
    func shown(at date: Date) -> [Item] {
        items.filter { $0.state.isActive || date.timeIntervalSince1970 - $0.since < AgentsWork.endedStays }
    }
}
