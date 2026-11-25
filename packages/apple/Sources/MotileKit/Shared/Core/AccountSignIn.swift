import Foundation
import Observation

/// Signing an account in from its sheet: its server runs the CLI's sign-in and the user signs in
/// on the page it links to. Claude Code's page shows a code to paste here; Codex's takes the code
/// shown here, and the sign-in finishes by itself once it was entered. Signing in saves the account
/// too.
@Observable
final class AccountSignIn {
    enum Step: Equatable {
        case idle
        case starting
        /// `deviceCode` is the code to enter on the page, for Codex.
        case waiting(id: String, url: URL, deviceCode: String?)
        case signedIn(String)
    }

    private(set) var step = Step.idle
    private(set) var finishing = false
    private(set) var error: String?
    var code = ""
    @ObservationIgnored private var serverID = ""
    /// Who signed in before this sign-in, who still is if it fails.
    @ObservationIgnored private var before = Step.idle

    /// What to do on the page while the sign-in waits.
    var waitingCaption: String? {
        switch step {
        case .waiting(_, _, nil): "Sign in on the page, then paste the code it shows."
        case .waiting(_, _, _?): "Sign in on the page and enter the code shown here."
        default: nil
        }
    }

    /// Whose page a sign-in of the agent opens.
    static func page(of agent: Agent) -> String {
        agent == .claude ? "Claude's sign-in page" : "OpenAI's sign-in page"
    }

    var canFinish: Bool {
        guard !finishing else { return false }
        if case .waiting(_, _, _?) = step { return true }
        return !code.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// Starts the sign-in; one that finishes by itself saves `account` and hands `saved` its id.
    func start(_ account: AgentAccount, on server: Server, in store: AppStore, saved: @escaping (String) -> Void) {
        guard step != .starting else { return }
        let signedIn = step
        cancel(in: store)
        if case .signedIn = signedIn { before = signedIn }
        serverID = server.id
        step = .starting
        error = nil
        store.core.send("request", ["server_id": server.id, "request": ["type": "start_sign_in", "account": account.json]]) { [weak self] result in
            guard let self, step == .starting else { return }
            switch result {
            case .success(let answer):
                guard let url = URL(string: answer.string("url")) else { return fail("\(account.agent.name) didn't give a page to sign in on.") }
                let deviceCode = answer.optionalString("code")
                step = .waiting(id: answer.string("id"), url: url, deviceCode: deviceCode)
                if deviceCode != nil { finish(account, in: store, saved: saved) }
            case .failure(let refusal):
                fail(refusal.message)
            }
        }
    }

    /// Hands the code to the sign-in, or waits for Codex's to be entered, and saves `account` with
    /// it; `saved` gets the saved account's id.
    func finish(_ account: AgentAccount, in store: AppStore, saved: @escaping (String) -> Void) {
        guard case .waiting(let id, _, let deviceCode) = step, canFinish else { return }
        finishing = true
        error = nil
        let code = code.trimmingCharacters(in: .whitespacesAndNewlines)
        let request: JSON = ["type": "finish_sign_in", "id": id, "code": code, "account": account.json]
        store.core.send("request", ["server_id": serverID, "request": request]) { [weak self] result in
            guard let self, case .waiting(id, _, _) = step else { return }
            finishing = false
            switch result {
            case .failure(let refusal) where deviceCode != nil && refusal.message.hasPrefix(Self.stillWaiting):
                finish(account, in: store, saved: saved)
            case .success(let answer):
                self.code = ""
                saved(answer.string("account"))
                let who = answer.optionalString("email").map { [$0, answer.optionalString("plan")].compactMap { $0 }.joined(separator: " · ") }
                step = .signedIn(who ?? "Claude Code")
            case .failure(let refusal):
                refused(refusal.message)
            }
        }
    }

    /// What the server says while Codex waits for its code to be entered, which is asked again.
    private static let stillWaiting = "Codex is still waiting"

    /// A sign-in that is over says so; one refused for the account it would save waits for the
    /// code again.
    private func refused(_ message: String) {
        guard message.hasSuffix("Sign in again.") else {
            error = message
            return
        }
        code = ""
        fail(message)
    }

    /// Stops a sign-in that waits for its code, as closing the sheet or changing the folder does.
    func cancel(in store: AppStore) {
        if case .waiting(let id, _, _) = step {
            store.core.send("request", ["server_id": serverID, "request": ["type": "cancel_sign_in", "id": id]])
        }
        step = .idle
        before = .idle
        finishing = false
        code = ""
        error = nil
    }

    private func fail(_ message: String) {
        step = before
        error = message
    }
}
