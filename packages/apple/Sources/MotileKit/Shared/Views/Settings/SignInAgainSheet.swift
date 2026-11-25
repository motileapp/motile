import SwiftUI

/// An agent's account to sign in again, from the error of the thread whose agent it stopped.
struct ThreadSignIn: Identifiable {
    let threadID: String
    let accountID: String

    var id: String { "\(threadID)/\(accountID)" }
}

/// Signs an agent's account in again once its sign-in stopped working, and has the thread's agent
/// go on. Claude Code signs in here, as its account's sheet does; Codex, and a server too old for
/// it, with a command run on the server.
struct SignInAgainSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.closeModal) private var close
    let request: ThreadSignIn
    @State private var signIn = AccountSignIn()

    private var thread: ThreadInfo? { store.threads[request.threadID] }
    private var server: Server? { thread.flatMap { store.server($0.serverID) } }
    private var account: AgentAccount? { server?.agentAccounts.first { $0.id == request.accountID } }
    private var agentName: String { account?.agent.name ?? "the agent" }
    private var serverName: String { server?.shortName ?? "its server" }

    private var signsInHere: Bool { account.map { server?.signsIn($0.agent) == true } ?? false }
    private var signedIn: Bool {
        if case .signedIn = signIn.step { return true }
        return false
    }
    private var signedInAs: String? {
        if case .signedIn(let who) = signIn.step { return who }
        return nil
    }

    private var title: String { "Sign In to \(agentName)" }
    /// The agent, and the account when the agent has several of its own on the server.
    private var accountTitle: String {
        guard let account else { return agentName }
        let accounts = server?.agentAccounts.filter { $0.agent == account.agent && !$0.gateway }.count ?? 0
        return accounts > 1 ? "\(agentName) (\(account.name))" : agentName
    }
    private var message: String {
        "\(accountTitle) on \(serverName) was signed out, so the agent stopped. Sign in again and it goes on where it left off."
    }
    private var caption: String {
        guard signsInHere else { return "Run it in a terminal on \(serverName), then continue." }
        if signedIn { return "Continue has the agent go on where it left off." }
        if let waiting = signIn.waitingCaption { return waiting }
        return "Opens \(AccountSignIn.page(of: account?.agent ?? .claude))."
    }

    var body: some View {
        content
            .onAppear(perform: start)
            .onDisappear { signIn.cancel(in: store) }
    }

    private func start() {
        guard signsInHere, let account, let server, signIn.step == .idle else { return }
        signIn.start(account, on: server, in: store) { _ in }
    }

    private func finish() {
        guard let account else { return }
        signIn.finish(account, in: store) { _ in }
    }

    private func goOn() {
        close()
        store.continueThread()
    }

    @ViewBuilder private var content: some View {
        #if os(macOS)
        VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 6) {
                Text(title)
                    .font(.ui(.lg, weight: .semibold))
                Text(message)
                    .font(.ui(.sm))
                    .foregroundStyle(Color.themeMutedForeground)
                    .fixedSize(horizontal: false, vertical: true)
            }
            VStack(alignment: .leading, spacing: 6) {
                steps
                Text(signIn.error ?? caption)
                    .font(.ui(.xs))
                    .foregroundStyle(signIn.error == nil ? Color.themeMutedStrongerForeground : Color.themeDestructive)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 4)
            }
            HStack(spacing: 0) {
                Spacer()
                if signedIn || !signsInHere {
                    ActionButton(signedIn ? "Close" : "Cancel", variant: .ghost, margin: .beside(trailing: 4)) { close() }
                        .keyboardShortcut(.cancelAction)
                    ActionButton("Continue", variant: .primary, margin: .beside(leading: 4), action: goOn)
                        .keyboardShortcut(.defaultAction)
                } else {
                    ActionButton("Cancel", variant: .ghost) { close() }
                        .keyboardShortcut(.cancelAction)
                }
            }
        }
        .padding(20)
        .frame(width: 460)
        #else
        NavigationStack {
            SettingsList {
                Section {
                    if signsInHere {
                        SignInRows(signIn: signIn, signedInAs: signedInAs, signsInAgain: false, start: start, finish: finish)
                    } else if let account {
                        Text(account.signInCommand)
                            .font(.system(.callout, design: .monospaced))
                            .foregroundStyle(Color.themeForeground)
                            .textSelection(.enabled)
                        ShareLink(item: account.signInCommand) {
                            SettingsActionLabel("Share Command", symbol: .share)
                        }
                    }
                } header: {
                    Text(message)
                        .textCase(nil)
                } footer: {
                    Text(signIn.error ?? caption)
                        .foregroundStyle(signIn.error == nil ? Color.themeMutedForeground : Color.themeDestructive)
                }
            }
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                GlassToolbarItem(placement: .cancellationAction) { SheetCloseButton() }
                if signedIn || !signsInHere {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Continue", action: goOn)
                    }
                }
            }
        }
        .presentationDragIndicator(.visible)
        .interactiveDismissDisabled(signIn.finishing)
        #endif
    }

    @ViewBuilder private var steps: some View {
        #if os(macOS)
        if let signedInAs {
            HStack(spacing: 8) {
                Image(.check, size: .sm)
                    .foregroundStyle(Color.themeSuccess)
                Text("Signed in as \(signedInAs)")
                    .font(.ui(.sm, weight: .medium))
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .padding(.horizontal, 4)
        } else if signsInHere {
            SignInSteps(signIn: signIn, signedInAs: signedInAs, signsInAgain: false, start: start, finish: finish)
        } else if let account {
            CommandBox(command: account.signInCommand)
        }
        #endif
    }
}
