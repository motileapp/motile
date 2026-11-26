import ActivityKit
import MotileShared
import SwiftUI
import WidgetKit

/// What the agents of every server do, on the Lock Screen and in the Dynamic Island, while they
/// work. The auth server starts, updates and ends it; each server's part is opened as it's drawn.
struct AgentsLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: AgentsAttributes.self) { context in
            let work = context.state.work()
            LockScreenWork(work: work)
                .activityBackgroundTint(WidgetTheme.background)
                .widgetURL(work.items.first?.link)
        } dynamicIsland: { context in
            let work = context.state.work()
            return DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    WorkSummary(work: work)
                        .padding(.leading, 4)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    VStack(alignment: .leading, spacing: 6) {
                        ForEach(work.items.prefix(3)) { item in
                            Link(destination: item.link) { WorkRow(item: item, showsProject: false) }
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 4)
                }
            } compactLeading: {
                StateDot(work: work)
            } compactTrailing: {
                Text("\(work.active)")
                    .font(.widget(.sm, weight: .semibold))
                    .foregroundStyle(WidgetTheme.foreground)
            } minimal: {
                StateDot(work: work)
            }
            .widgetURL(work.items.first?.link)
        }
    }
}

/// The colour of what matters most: an agent that waits, else one that works.
private struct StateDot: View {
    let work: AgentsWork

    var body: some View {
        Circle()
            .fill(WidgetTheme.color(of: work.waiting > 0 ? .waiting : .working))
            .frame(width: WorkRow.dot * 1.5, height: WorkRow.dot * 1.5)
    }
}

private struct LockScreenWork: View {
    let work: AgentsWork

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            WorkSummary(work: work, size: .lg)
            ForEach(work.items.prefix(3)) { item in
                WorkRow(item: item)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(16)
    }
}
