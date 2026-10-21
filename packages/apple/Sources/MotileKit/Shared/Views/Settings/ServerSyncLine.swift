import SwiftUI

/// The line under a server that says how its copies of the other servers' threads stand. It is
/// always one line, whatever it says, so the rows around it never move.
struct ServerSyncLine: View {
    let sync: ServerSync
    var size: CGFloat = 11.5

    var body: some View {
        TimelineView(.periodic(from: .now, by: 30)) { context in
            HStack(spacing: 5) {
                symbol
                    .frame(width: size, height: size)
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
            Image(.check, size: size)
                .foregroundStyle(Color.themeSuccess)
        case .copying:
            Spinner(size: size)
                .foregroundStyle(Color.themeMutedForeground)
        case .failed:
            Image(.triangleAlert, size: size - 1)
                .foregroundStyle(Color.themeWarning)
        case .waiting, .off:
            Image(.circleDashed, size: size - 1)
                .foregroundStyle(Color.themeMutedForeground)
        }
    }
}
