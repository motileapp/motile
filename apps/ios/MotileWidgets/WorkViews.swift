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

/// A thread in the work: a dot of its state, its title, and its project under it.
struct WorkRow: View {
    static let dot: CGFloat = 7

    let item: AgentsWork.Item
    var stale = false
    var showsProject = true

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Circle()
                .fill(WidgetTheme.color(of: item.state))
                .frame(width: Self.dot, height: Self.dot)
                .alignmentGuide(.firstTextBaseline) { $0[.bottom] }
            VStack(alignment: .leading, spacing: 1) {
                Text(item.title)
                    .font(.widget(.sm, weight: .medium))
                    .foregroundStyle(WidgetTheme.foreground)
                if showsProject {
                    Text(item.project)
                        .font(.widget(.xs))
                        .foregroundStyle(WidgetTheme.mutedForeground)
                }
            }
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
