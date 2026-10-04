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
            Image(systemName: "server.rack")
                .font(.ui(size: 24, weight: .medium))
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
                ProgressView().controlSize(.small)
                Text("Waiting for your server…")
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
                HStack(spacing: 6) {
                    Text("Signed in as \(store.account.email)")
                    LinkButton("Sign out") { store.signOut() }
                }
                .font(.ui(size: 12))
                .foregroundStyle(Color.themeTertiary)
                .padding(.bottom, 24)
            } else {
                Button("Done") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                    .padding(.vertical, 26)
            }
        }
        .padding(.horizontal, 30)
        .frame(maxWidth: .infinity, maxHeight: isFirst ? .infinity : nil)
        .onAppear { store.prepareToAddServer() }
        .onDisappear { store.stopAddingServer() }
    }

    private static let buttonSize = scaled(28)
    private static let symbolSize: CGFloat = 14

    private var commandBox: some View {
        #if os(macOS)
        let layout = AnyLayout(HStackLayout(alignment: .top, spacing: 8))
        #else
        // A phone has no room for the buttons beside the command, and can send it to the machine.
        let layout = AnyLayout(VStackLayout(alignment: .trailing, spacing: 8))
        #endif
        return layout {
            Text(store.enrollToken?.command ?? "Preparing the command…")
                .font(.ui(size: 12.5, design: .monospaced))
                .foregroundStyle(store.enrollToken == nil ? Color.themeTertiary : Color.themeText)
                .textSelection(.enabled)
                .lineSpacing(3)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
            HStack(spacing: 4) {
                #if os(iOS)
                ShareLink(item: store.enrollToken?.command ?? "") {
                    Image(systemName: "square.and.arrow.up")
                        .font(.ui(size: Self.symbolSize, weight: .medium))
                        .frame(width: Self.buttonSize, height: Self.buttonSize)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.highlight(radius: 6, faded: true))
                .accessibilityLabel("Share the command")
                #endif
                IconOnlyButton(symbol: copied ? "checkmark" : "doc.on.doc", help: "Copy the command", size: Self.buttonSize, symbolSize: Self.symbolSize, faded: true) {
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
        .background(Color(platform: Theme.codeBackground), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).stroke(Color.themeBorder))
    }
}
