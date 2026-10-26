import SwiftUI

/// The line under a server that says how its copies of the other servers' threads stand. It is
/// always one line, whatever it says, so the rows around it never move.
struct ServerSyncLine: View {
    let sync: ServerSync
    var size: CGFloat = 11.5
    private var symbolSize: IconSize { IconSize(text: size) }

    var body: some View {
        TimelineView(.periodic(from: .now, by: 30)) { context in
            HStack(spacing: 5) {
                symbol
                Text(sync.line(now: context.date.timeIntervalSince1970))
                    .font(.ui(size: size))
                    .monospacedDigit()
                    .foregroundStyle(Color.themeMutedForeground)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
        }
        .help(sync.error ?? "")
    }

    @ViewBuilder private var symbol: some View {
        switch sync.state {
        case .synced:
            Image(.check, size: symbolSize)
                .foregroundStyle(Color.themeSuccess)
        case .copying:
            Spinner(size: symbolSize)
                .foregroundStyle(Color.themeMutedForeground)
        case .failed:
            Image(.triangleAlert, size: symbolSize)
                .foregroundStyle(Color.themeWarning)
        case .waiting, .off:
            Image(.circleDashed, size: symbolSize)
                .foregroundStyle(Color.themeMutedForeground)
        }
    }
}
