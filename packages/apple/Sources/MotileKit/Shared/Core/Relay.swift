import Foundation

extension AppStore {
    /// Has every device of the account use this iroh relay; `nil` goes back to iroh's public ones.
    /// `done` gets the error, if any.
    func setRelay(_ url: String?, done: @escaping (String?) -> Void) {
        let value: Any = url ?? NSNull()
        core.send("set_relay", ["url": value]) { result in
            switch result {
            case .success: done(nil)
            case .failure(let error): done(error.message)
            }
        }
    }

    /// Goes back to iroh's public relays, saying so if it can't.
    func useIrohRelays() {
        core.send("set_relay", ["url": NSNull()]) { [weak self] result in
            if case .failure(let error) = result { self?.alert(error) }
        }
    }
}
