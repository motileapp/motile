import SwiftUI

/// The code Codex's sign-in shows, to enter on its page, with the way to copy it.
struct DeviceCode: View {
    let code: String

    var body: some View {
        HStack(spacing: 10) {
            Text(code)
                .font(.ui(.xxl, weight: .semibold, design: .monospaced))
                .foregroundStyle(Color.themeForeground)
                .textSelection(.enabled)
            CopyButton(help: "Copy the code") { Platform.copy(code) }
        }
        .frame(maxWidth: .infinity)
    }
}

/// What a sign-in that finishes by itself shows while it waits.
struct WaitingForCode: View {
    var body: some View {
        HStack(spacing: 8) {
            Spinner(size: .md)
            Text("Waiting for you to enter the code")
                .font(.ui(.sm))
        }
        .foregroundStyle(Color.themeMutedForeground)
    }
}

#if os(macOS)
/// Signing an account in: the page to sign in on and the code to paste or to enter there, or who
/// is signed in with the way to sign in again.
struct SignInSteps: View {
    @Bindable var signIn: AccountSignIn
    let signedInAs: String?
    /// Whatever else the code waits for besides being typed.
    var canFinish = true
    /// Once signed in, it offers to sign in again.
    var signsInAgain = true
    let start: () -> Void
    let finish: () -> Void
    @FocusState private var codeFocused: Bool

    var body: some View {
        switch signIn.step {
        case .waiting(_, let url, let deviceCode?):
            VStack(alignment: .leading, spacing: 6) {
                CommandBox(command: url.absoluteString, link: url)
                DeviceCode(code: deviceCode)
                    .padding(.vertical, 6)
                WaitingForCode()
                    .padding(.horizontal, 4)
            }
        case .waiting(_, let url, nil):
            VStack(alignment: .leading, spacing: 6) {
                CommandBox(command: url.absoluteString, link: url)
                HStack(spacing: 6) {
                    InputField("Paste the code", text: $signIn.code, monospaced: true, focus: $codeFocused)
                        .onReturn(finish)
                        .onAppear { codeFocused = true }
                    ActionButton("Sign In", variant: .primary, pending: signIn.finishing, action: finish)
                        .disabled(!signIn.canFinish || !canFinish)
                }
            }
        default:
            HStack(spacing: 10) {
                Text(signedInAs.map { "Signed in as \($0)" } ?? "Not signed in")
                    .font(.ui(.sm, weight: .medium))
                    .foregroundStyle(signedInAs == nil ? Color.themeMutedForeground : Color.themeForeground)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .padding(.horizontal, 4)
                Spacer(minLength: 12)
                if signedInAs == nil || signsInAgain {
                    ActionButton(signedInAs == nil ? "Sign In" : "Sign In Again", pending: signIn.step == .starting, action: start)
                        .disabled(!canFinish)
                }
            }
        }
    }
}
#else
/// Signing an account in, as rows of a section: the page to sign in on and the code to paste or
/// to enter there, or who is signed in with the way to sign in again.
struct SignInRows: View {
    @Bindable var signIn: AccountSignIn
    let signedInAs: String?
    /// Whatever else the code waits for besides being typed.
    var canFinish = true
    /// Once signed in, it offers to sign in again.
    var signsInAgain = true
    let start: () -> Void
    let finish: () -> Void
    @State private var copied = false
    @State private var copies = 0

    var body: some View {
        switch signIn.step {
        case .waiting(_, let url, let deviceCode):
            Text(url.absoluteString)
                .font(.system(.callout, design: .monospaced))
                .foregroundStyle(Color.themeMutedForeground)
                .lineLimit(1)
            Link(destination: url) {
                SettingsActionLabel("Open the Sign-In Page", symbol: .squareArrowOutUpRight)
            }
            Button {
                Platform.copy(url.absoluteString)
                copied = true
                copies += 1
            } label: {
                SettingsActionLabel(copied ? "Copied" : "Copy the Link", symbol: copied ? .check : .copy)
            }
            .task(id: copies) {
                guard copied, (try? await Task.sleep(for: .seconds(1.2))) != nil else { return }
                copied = false
            }
            if let deviceCode {
                DeviceCode(code: deviceCode)
                    .padding(.vertical, 4)
                WaitingForCode()
            } else {
                codeField
            }
        default:
            if let signedInAs {
                Text("Signed in as \(signedInAs)")
                    .foregroundStyle(Color.themeForeground)
            }
            if signedInAs == nil || signsInAgain {
                Button(action: start) {
                    HStack {
                        SettingsActionLabel(signedInAs == nil ? "Sign In" : "Sign In Again", symbol: .circleUser)
                        Spacer()
                        if signIn.step == .starting { Spinner(size: .xl) }
                    }
                }
                .disabled(signIn.step == .starting || !canFinish)
            }
        }
    }

    private var codeField: some View {
        HStack {
            TextField("Paste the code", text: $signIn.code)
                .font(.system(.body, design: .monospaced))
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .submitLabel(.done)
                .onReturn(finish)
            if signIn.finishing {
                Spinner(size: .xl)
            } else {
                Button("Sign In", action: finish)
                    .disabled(!signIn.canFinish || !canFinish)
            }
        }
    }
}
#endif
