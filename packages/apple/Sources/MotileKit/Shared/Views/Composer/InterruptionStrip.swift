import SwiftUI

/// Why the agent stopped before it finished, above the composer: the usage limit, with when the
/// thread goes on by itself and the account's resets to refill it now, or a restart of the server,
/// whose agent is continued from here.
struct InterruptionStrip: View {
    @Environment(AppStore.self) private var store
    let interruption: Interruption

    var body: some View {
        TimelineView(.periodic(from: .now, by: 30)) { context in
            let now = context.date.timeIntervalSince1970
            HStack(spacing: 0) {
                Image(interruption.symbol, size: .sm)
                    .foregroundStyle(Color.themeWarning)
                    .padding(.leading, 14)
                    .padding(.trailing, 7)
                // A narrow composer keeps what happens next and leaves out the title.
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 8) {
                        Text(interruption.title)
                            .font(.ui(.xs, weight: .medium))
                            .foregroundStyle(Color.themeWarning)
                        Text(interruption.detail(now: now))
                            .foregroundStyle(Color.themeMutedForeground)
                    }
                    Text(interruption.detail(now: now))
                        .foregroundStyle(Color.themeWarning)
                        .truncationMode(.middle)
                }
                .font(.ui(.xs))
                .lineLimit(1)
                .tooltip("\(interruption.title). \(interruption.detail(now: now)).")
                Spacer(minLength: 8)
                if let thread = store.selectedThread, let resets = limitResets(of: thread) {
                    resetMenu(resets, of: thread)
                }
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
        case .limit:
            EmptyView()
        case .restart:
            button("Continue", help: "Have the agent go on where it left off") { store.continueThread() }
        }
    }

    /// As the strip's other controls, filled on iOS where a control isn't lit under the pointer.
    private func resetMenu(_ resets: LimitResets, of thread: ThreadInfo) -> some View {
        #if os(iOS)
        ResetMenu(resets: resets, variant: .secondary, margin: ComposerStrip.margin, thread: thread)
        #else
        ResetMenu(resets: resets, variant: .ghost, margin: ComposerStrip.margin, thread: thread, up: true)
        #endif
    }

    /// The resets that can refill the limit that stopped the thread's agent.
    private func limitResets(of thread: ThreadInfo) -> LimitResets? {
        guard case .limit = interruption, let resets = store.resets(of: thread) else { return nil }
        return resets.options.contains { $0.usable || $0.atLimit } ? resets : nil
    }

    private func button(_ title: String, help: String, action: @escaping () -> Void) -> some View {
        ActionButton(title, help: help, variant: .ghost, margin: ComposerStrip.margin, action: action)
    }
}
