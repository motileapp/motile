import AppKit
import SwiftUI

/// The one command that turns a machine into a host. Shown when the account has no host yet,
/// and as a sheet when adding another.
struct ConnectHostView: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    let isFirst: Bool
    @State private var copied = false

    var body: some View {
        VStack(spacing: 0) {
            if isFirst { Spacer() }
            Image(systemName: "server.rack")
                .font(.system(size: 24, weight: .medium))
                .foregroundStyle(Color.themePrimary)
                .frame(width: 56, height: 56)
                .background(Color.themePrimary.opacity(0.1), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                .padding(.top, isFirst ? 0 : 32)
            Text(isFirst ? "Connect your first host" : "Add a host")
                .font(.system(size: 24, weight: .semibold))
                .padding(.top, 18)
            Text("Run this on the Linux machine where your agents should work. It installs Motile, links the machine to your account and keeps it running.")
                .font(.system(size: 14))
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
                Text("Waiting for the host to connect…")
                    .font(.system(size: 13))
                    .foregroundStyle(Color.themeSecondary)
            }
            .padding(.top, 22)

            Text("The command works for one machine, for an hour. If neither Claude Code nor Codex is installed there, the installer offers to install them.")
                .font(.system(size: 12))
                .foregroundStyle(Color.themeTertiary)
                .multilineTextAlignment(.center)
                .lineSpacing(2)
                .frame(maxWidth: 440)
                .padding(.top, 14)

            if isFirst {
                Spacer()
                HStack(spacing: 6) {
                    Text("Signed in as \(store.account.email)")
                    Button("Sign out") { store.signOut() }
                        .buttonStyle(.link)
                }
                .font(.system(size: 12))
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
        .onAppear { store.prepareToAddHost() }
        .onDisappear { store.stopAddingHost() }
    }

    private var commandBox: some View {
        HStack(alignment: .top, spacing: 12) {
            Text(store.enrollToken?.command ?? "Preparing the command…")
                .font(.system(size: 12.5, design: .monospaced))
                .foregroundStyle(store.enrollToken == nil ? Color.themeTertiary : Color.themeText)
                .textSelection(.enabled)
                .lineSpacing(3)
                .frame(maxWidth: .infinity, alignment: .leading)
            Button {
                guard let command = store.enrollToken?.command else { return }
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(command, forType: .string)
                copied = true
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { copied = false }
            } label: {
                Label(copied ? "Copied" : "Copy", systemImage: copied ? "checkmark" : "doc.on.doc")
                    .font(.system(size: 12, weight: .medium))
            }
            .controlSize(.small)
            .disabled(store.enrollToken == nil)
        }
        .padding(14)
        .background(Color(nsColor: Theme.codeBackground), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).stroke(Color.themeBorder))
    }
}
