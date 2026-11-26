import MotileShared
import SwiftUI
import WidgetKit

struct AgentsEntry: TimelineEntry {
    let date: Date
    let work: AgentsWork
}

/// What the app last left of the agents' work. The app reloads the timeline when that changes;
/// between, an entry comes whenever something turns stale or an ended thread goes.
struct AgentsProvider: TimelineProvider {
    func placeholder(in context: Context) -> AgentsEntry {
        AgentsEntry(date: .now, work: AgentsWork([]))
    }

    func getSnapshot(in context: Context, completion: @escaping (AgentsEntry) -> Void) {
        completion(AgentsEntry(date: .now, work: AgentsWork(SharedState.work)))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<AgentsEntry>) -> Void) {
        let work = AgentsWork(SharedState.work)
        let now = Date.now
        let changes = work.items.map { item in
            item.state.isActive ? item.heard + AgentsWork.staleAfter : item.since + AgentsWork.endedStays
        }
        let later = Set(changes).sorted().map { Date(timeIntervalSince1970: $0 + 1) }.filter { $0 > now }
        let entries = ([now] + later).map { AgentsEntry(date: $0, work: work) }
        completion(Timeline(entries: entries, policy: .never))
    }
}

struct AgentsWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "Agents", provider: AgentsProvider()) { entry in
            AgentsWidgetView(entry: entry)
                .containerBackground(WidgetTheme.background, for: .widget)
        }
        .configurationDisplayName("Agents")
        .description("What your agents are doing.")
        .supportedFamilies([.systemSmall, .systemMedium, .accessoryRectangular, .accessoryCircular])
    }
}

struct AgentsWidgetView: View {
    @Environment(\.widgetFamily) private var family
    let entry: AgentsEntry

    var body: some View {
        let items = entry.work.shown(at: entry.date)
        switch family {
        case .accessoryCircular:
            circular
        case .accessoryRectangular:
            rectangular(items)
        case .systemMedium:
            system(items, rows: 4, links: true)
        default:
            system(items, rows: 3, links: false)
                .widgetURL(items.first?.link)
        }
    }

    private func system(_ items: [AgentsWork.Item], rows: Int, links: Bool) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            WorkSummary(work: entry.work)
            ForEach(items.prefix(rows)) { item in
                let row = WorkRow(item: item, stale: item.isStale(at: entry.date))
                if links {
                    Link(destination: item.link) { row }
                } else {
                    row
                }
            }
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func rectangular(_ items: [AgentsWork.Item]) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(entry.work.summary)
                .font(.widget(.md, weight: .semibold))
            ForEach(items.prefix(2)) { item in
                Text(item.title)
                    .font(.widget(.sm))
                    .opacity(item.isStale(at: entry.date) ? WidgetTheme.disabled : 1)
            }
        }
        .lineLimit(1)
        .frame(maxWidth: .infinity, alignment: .leading)
        .widgetURL(items.first?.link)
    }

    private var circular: some View {
        ZStack {
            AccessoryWidgetBackground()
            VStack(spacing: 0) {
                Text("\(entry.work.active)")
                    .font(.widget(.lg, weight: .semibold))
                Text(entry.work.active == 1 ? "agent" : "agents")
                    .font(.widget(.xs))
            }
        }
        .widgetURL(entry.work.items.first?.link)
    }
}
