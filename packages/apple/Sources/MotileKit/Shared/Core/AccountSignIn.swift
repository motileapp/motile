import Foundation
import Observation

/// Signing an account of Claude Code in from its sheet: its server runs the CLI's sign-in, the
/// user signs in on the page it links to, and pastes the code that page shows, which saves the
/// account too.
@Observable
final class AccountSignIn {
    enum Step: Equatable {
        case idle
        case starting
        case waiting(id: String, url: URL)
        case signedIn(String)
    }

    private(set) var step = Step.idle
    private(set) var finishing = false
    private(set) var error: String?
    var code = ""
    @ObservationIgnored private var serverID = ""
    /// Who signed in before this sign-in, who still is if it fails.
    @ObservationIgnored private var before = Step.idle

    var canFinish: Bool { !finishing && !code.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    func start(_ account: AgentAccount, on server: Server, in store: AppStore) {
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
                guard let url = URL(string: answer.string("url")) else { return fail("Claude Code didn't give a page to sign in on.") }
                step = .waiting(id: answer.string("id"), url: url)
            case .failure(let refusal):
                fail(refusal.message)
            }
        }
    }

    /// Hands the code to the sign-in and saves `account` with it; `saved` gets the saved account's id.
    func finish(_ account: AgentAccount, in store: AppStore, saved: @escaping (String) -> Void) {
        guard case .waiting(let id, _) = step, canFinish else { return }
        finishing = true
        error = nil
        let code = code.trimmingCharacters(in: .whitespacesAndNewlines)
        let request: JSON = ["type": "finish_sign_in", "id": id, "code": code, "account": account.json]
        store.core.send("request", ["server_id": serverID, "request": request]) { [weak self] result in
            guard let self, case .waiting(id, _) = step else { return }
            finishing = false
            switch result {
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
        if case .waiting(let id, _) = step {
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
