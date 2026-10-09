import SwiftUI

/// Why the agent stopped before it finished, above the composer: the usage limit, with when the
/// thread goes on by itself, or a restart of the server. The agent is continued from here.
struct InterruptionStrip: View {
    @Environment(AppStore.self) private var store
    let interruption: Interruption

    var body: some View {
        TimelineView(.periodic(from: .now, by: 30)) { context in
            let now = context.date.timeIntervalSince1970
            HStack(spacing: 0) {
                Image(interruption.symbol, size: 13)
                    .foregroundStyle(Color.themeWarning)
                    .padding(.leading, 14)
                    .padding(.trailing, 7)
                // A narrow composer keeps what happens next and leaves out the title.
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 8) {
                        Text(interruption.title)
                            .font(.ui(size: 12.5, weight: .medium))
                            .foregroundStyle(Color.themeWarning)
                        Text(interruption.detail(now: now))
                            .foregroundStyle(Color.themeMutedForeground)
                    }
                    Text(interruption.detail(now: now))
                        .foregroundStyle(Color.themeWarning)
                        .truncationMode(.middle)
                }
                .font(.ui(size: 12.5))
                .lineLimit(1)
                .help("\(interruption.title). \(interruption.detail(now: now)).")
                Spacer(minLength: 8)
                action(now: now)
            }
        }
        .modifier(ComposerStrip(edge: .top))
    }

    @ViewBuilder private func action(now: Double) -> some View {
        switch interruption {
        case .limit(let resetsAt?, let continues) where resetsAt > now:
            if continues {
                button("Cancel", help: "Don't continue when the limit resets") { store.setContinues(false) }
            } else {
                button("Continue at Reset", help: "Continue the agent once the limit resets") { store.setContinues(true) }
            }
        case .limit(_?, true):
            EmptyView()
        default:
            button("Continue", help: "Have the agent go on where it left off") { store.continueThread() }
        }
    }

    private func button(_ title: String, help: String, action: @escaping () -> Void) -> some View {
        ActionButton(title, help: help, variant: .ghost, margin: ComposerStrip.margin, action: action)
    }
}
