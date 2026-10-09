import SwiftUI

/// The one command that turns a machine into a server. Shown when the account has no server yet,
/// and as a sheet when adding another, centred in it on iOS.
struct ConnectServerView: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    let isFirst: Bool

    private var centred: Bool {
        #if os(iOS)
        true
        #else
        isFirst
        #endif
    }

    var body: some View {
        VStack(spacing: 0) {
            if centred { Spacer() }
            Image(.server, size: 24)
                .foregroundStyle(Color.themePrimary)
                .frame(width: 56, height: 56)
                .background(Color.themePrimary.tinted(), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                .padding(.top, centred ? 0 : 32)
            Text(isFirst ? "Connect your first server" : "Run this on your server")
                .font(.ui(size: 24, weight: .semibold))
                .multilineTextAlignment(.center)
                .padding(.horizontal, 24)
                .padding(.top, 18)
            Text("Run the command below on the server that will run your agents")
                .font(.ui(size: 14))
                .foregroundStyle(Color.themeMutedForeground)
                .multilineTextAlignment(.center)
                .lineSpacing(3)
                .frame(maxWidth: 480)
                .padding(.horizontal, 24)
                .padding(.top, 8)

            CommandBox(command: store.enrollToken?.command, spans: store.enrollToken?.spans ?? [])
                .frame(maxWidth: 560)
                .padding(.top, 26)

            TimelineView(.periodic(from: .now, by: 1)) { context in
                status(at: context.date)
            }
            .padding(.top, 22)

            if isFirst {
                Spacer()
                HStack(spacing: 0) {
                    Text("Signed in as \(store.account.email)")
                    ActionButton("Sign out", variant: .link, size: .small) { store.signOut() }
                }
                .font(.ui(size: 12))
                .foregroundStyle(Color.themeMutedMoreForeground)
                .padding(.bottom, 20)
            } else if centred {
                Spacer()
            } else {
                ActionButton("Done") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                    .padding(.vertical, 26)
            }
        }
        .padding(.horizontal, 16)
        .frame(maxWidth: .infinity, maxHeight: centred ? .infinity : nil)
        .onAppear { store.prepareToAddServer() }
        .onDisappear { store.stopAddingServer() }
    }

    /// The spinner and the time the command has left, or that it has run out and a way to a new one.
    @ViewBuilder
    private func status(at date: Date) -> some View {
        if let token = store.enrollToken, token.isExpired(at: date) {
            VStack(spacing: 14) {
                Text("The command has expired")
                    .font(.ui(size: 13))
                    .foregroundStyle(Color.themeMutedForeground)
                ActionButton("Regenerate", pending: store.enrollTokenPending) { store.regenerateEnrollToken() }
            }
        } else {
            VStack(spacing: 14) {
                HStack(spacing: 8) {
                    Spinner()
                        .foregroundStyle(Color.themeMutedForeground)
                    Text("Waiting for your server")
                        .font(.ui(size: 13))
                        .foregroundStyle(Color.themeMutedForeground)
                }
                Text(store.enrollToken?.timeLeft(at: date) ?? "15:00")
                    .font(.ui(size: 12))
                    .monospacedDigit()
                    .foregroundStyle(Color.themeMutedMoreForeground)
            }
        }
    }
}
