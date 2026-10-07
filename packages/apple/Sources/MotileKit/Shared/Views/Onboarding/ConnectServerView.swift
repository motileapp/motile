import SwiftUI

/// The one command that turns a machine into a server. Shown when the account has no server yet,
/// and as a sheet when adding another, centred in it on iOS.
struct ConnectServerView: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    @Environment(\.surface) private var surface
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
                .background(Color.themePrimary.opacity(0.1), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                .padding(.top, centred ? 0 : 32)
            Text(isFirst ? "Connect your first server" : "Run this on your server")
                .font(.ui(size: 24, weight: .semibold))
                .multilineTextAlignment(.center)
                .padding(.horizontal, 24)
                .padding(.top, 18)
            Text("Run the command below on the server that will run your agents")
                .font(.ui(size: 14))
                .foregroundStyle(Color.themeSecondary)
                .multilineTextAlignment(.center)
                .lineSpacing(3)
                .frame(maxWidth: 480)
                .padding(.horizontal, 24)
                .padding(.top, 8)

            commandBox
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
                .foregroundStyle(Color.themeTertiary)
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
                    .foregroundStyle(Color.themeSecondary)
                ActionButton("Regenerate", pending: store.enrollTokenPending) { store.regenerateEnrollToken() }
            }
        } else {
            VStack(spacing: 14) {
                HStack(spacing: 8) {
                    Spinner()
                        .foregroundStyle(Color.themeSecondary)
                    Text("Waiting for your server")
                        .font(.ui(size: 13))
                        .foregroundStyle(Color.themeSecondary)
                }
                Text(store.enrollToken?.timeLeft(at: date) ?? "15:00")
                    .font(.ui(size: 12))
                    .monospacedDigit()
                    .foregroundStyle(Color.themeTertiary)
            }
        }
    }

    /// Puts the first line of the command level with the middle of the buttons.
    private static let textInset = ((ControlSize.regular.height - PlatformFont.uiMono(12.5).textLineHeight) / 2).rounded()

    private var commandBox: some View {
        HStack(alignment: .top, spacing: 8) {
            Text(store.enrollToken.map(Self.highlighted) ?? AttributedString("Preparing the command…"))
                .font(.ui(size: 12.5, design: .monospaced))
                .foregroundStyle(store.enrollToken == nil ? Color.themeTertiary : Color.themeText)
                .lineSpacing(3)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding([.vertical, .leading], Self.textInset)
            HStack(spacing: 4) {
                #if os(iOS)
                ShareLink(item: store.enrollToken?.command ?? "") {
                    ControlLabel(title: nil, icon: .symbol(.share), size: .regular, symbolSize: 12)
                }
                .buttonStyle(.control())
                .accessibilityLabel("Share the command")
                #endif
                CopyButton(help: "Copy the command", symbolSize: 12) {
                    guard let command = store.enrollToken?.command else { return }
                    Platform.copy(command)
                }
            }
            .disabled(store.enrollToken == nil)
        }
        .padding(scaled(4))
        .layered(in: RoundedRectangle(cornerRadius: Radius.card, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: Radius.card, style: .continuous)
                .strokeBorder(surface.border, lineWidth: 1)
        }
    }

    /// The command in the theme's code colours. It wraps between any two characters, as CSS's
    /// `break-all` does, so it is not selectable: a copy would carry the zero-width spaces into the shell.
    private static func highlighted(_ token: EnrollToken) -> AttributedString {
        var colours = [Int](repeating: 0, count: token.command.utf16.count)
        for index in stride(from: 0, to: token.spans.count - 2, by: 3) {
            let start = token.spans[index], end = start + token.spans[index + 1]
            guard end <= colours.count else { continue }
            colours.replaceSubrange(start..<end, with: repeatElement(token.spans[index + 2], count: end - start))
        }
        var result = AttributedString()
        var offset = 0
        for character in token.command {
            var piece = AttributedString(String(character) + "\u{200B}")
            let colour = colours[offset]
            piece.foregroundColor = Color(platform: Theme.syntax[colour < Theme.syntax.count ? colour : 0])
            result += piece
            offset += character.utf16.count
        }
        return result
    }
}
