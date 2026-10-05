import SwiftUI

/// The one command that turns a machine into a server. Shown when the account has no server yet,
/// and as a sheet when adding another.
struct ConnectServerView: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    let isFirst: Bool
    @State private var copied = false

    var body: some View {
        VStack(spacing: 0) {
            if isFirst { Spacer() }
            Image(.server, size: 24)
                .foregroundStyle(Color.themePrimary)
                .frame(width: 56, height: 56)
                .background(Color.themePrimary.opacity(0.1), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                .padding(.top, isFirst ? 0 : 32)
            Text(isFirst ? "Connect your first server" : "Add a server")
                .font(.ui(size: 24, weight: .semibold))
                .padding(.top, 18)
            Text("Run this on the Linux machine or Mac that will run your agents.")
                .font(.ui(size: 14))
                .foregroundStyle(Color.themeSecondary)
                .multilineTextAlignment(.center)
                .lineSpacing(3)
                .frame(maxWidth: 480)
                .padding(.top, 8)

            commandBox
                .frame(maxWidth: 560)
                .padding(.top, 26)

            HStack(spacing: 8) {
                Spinner()
                    .foregroundStyle(Color.themeSecondary)
                Text("Waiting for your server")
                    .font(.ui(size: 13))
                    .foregroundStyle(Color.themeSecondary)
            }
            .padding(.top, 22)

            Text("The command works once, for an hour.")
                .font(.ui(size: 12))
                .foregroundStyle(Color.themeTertiary)
                .multilineTextAlignment(.center)
                .lineSpacing(2)
                .frame(maxWidth: 440)
                .padding(.top, 14)

            if isFirst {
                Spacer()
                HStack(spacing: 0) {
                    Text("Signed in as \(store.account.email)")
                    ActionButton("Sign out", variant: .link, size: .small) { store.signOut() }
                }
                .font(.ui(size: 12))
                .foregroundStyle(Color.themeTertiary)
                .padding(.bottom, 20)
            } else {
                ActionButton("Done") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                    .padding(.vertical, 26)
            }
        }
        .padding(.horizontal, 30)
        .frame(maxWidth: .infinity, maxHeight: isFirst ? .infinity : nil)
        .onAppear { store.prepareToAddServer() }
        .onDisappear { store.stopAddingServer() }
    }

    private var commandBox: some View {
        #if os(macOS)
        let layout = AnyLayout(HStackLayout(alignment: .top, spacing: 8))
        #else
        // A phone has no room for the buttons beside the command, and can send it to the machine.
        let layout = AnyLayout(VStackLayout(alignment: .trailing, spacing: 8))
        #endif
        return layout {
            Text(store.enrollToken.map { Self.breakingAnywhere($0.command) } ?? "Preparing the command…")
                .font(.ui(size: 12.5, design: .monospaced))
                .foregroundStyle(store.enrollToken == nil ? Color.themeTertiary : Color.themeText)
                .lineSpacing(3)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
            HStack(spacing: 4) {
                #if os(iOS)
                ShareLink(item: store.enrollToken?.command ?? "") {
                    ControlLabel(title: nil, icon: .symbol(.share), size: .regular, symbolSize: 12)
                }
                .buttonStyle(.control())
                .accessibilityLabel("Share the command")
                #endif
                ActionButton(icon: copied ? .check : .copy, help: "Copy the command", symbolSize: 12) {
                    guard let command = store.enrollToken?.command else { return }
                    Platform.copy(command)
                    copied = true
                    DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { copied = false }
                }
            }
            .disabled(store.enrollToken == nil)
        }
        .padding(.leading, 14)
        .padding([.vertical, .trailing], 10)
        .layered(in: RoundedRectangle(cornerRadius: Radius.card, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Radius.card, style: .continuous).stroke(Color.themeBorder))
    }

    /// Lets the command wrap between any two characters, as CSS's `break-all` does. Not selectable,
    /// since a copy would carry the zero-width spaces into the shell.
    private static func breakingAnywhere(_ text: String) -> String {
        text.map(String.init).joined(separator: "\u{200B}")
    }
}
