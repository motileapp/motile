import CMotileCore
import Foundation

/// The app's end of the Rust core: commands go in as JSON, events come out as JSON. Events are
/// decoded on a background queue, in the order they were sent, and handed to the main thread.
final class CoreBridge {
    typealias Reply = (Result<[String: Any], CoreError>) -> Void

    struct CoreError: Error, LocalizedError {
        let message: String
        var errorDescription: String? { message }
    }

    /// Decodes an event off the main thread into the work to do on it.
    var decode: (([String: Any]) -> (() -> Void)?)?

    private let decoding = DispatchQueue(label: "app.motile.events", qos: .userInitiated)
    private var nextID: UInt64 = 0
    private var replies: [UInt64: Reply] = [:]

    func start(config: [String: Any]) -> Bool {
        guard let json = Self.json(config) else { return false }
        let context = Unmanaged.passUnretained(self).toOpaque()
        return json.withCString { motile_start($0, coreEvent, context) }
    }

    /// Sends a command. `reply` is called on the main thread with the answer.
    func send(_ type: String, _ fields: [String: Any] = [:], reply: Reply? = nil) {
        dispatchPrecondition(condition: .onQueue(.main))
        nextID += 1
        var command = fields
        command["type"] = type
        command["id"] = nextID
        if let reply { replies[nextID] = reply }
        guard let json = Self.json(command) else { return }
        json.withCString { motile_send($0) }
    }

    fileprivate func received(_ data: Data) {
        decoding.async { [weak self] in
            guard let self, let event = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return }
            if event["type"] as? String == "reply" {
                DispatchQueue.main.async { self.answer(event) }
                return
            }
            guard let apply = self.decode?(event) else { return }
            DispatchQueue.main.async(execute: apply)
        }
    }

    private func answer(_ event: [String: Any]) {
        guard let id = (event["id"] as? NSNumber)?.uint64Value, let reply = replies.removeValue(forKey: id) else { return }
        let value = event["value"] as? [String: Any] ?? [:]
        if event["ok"] as? Bool == true {
            reply(.success(value))
        } else {
            reply(.failure(CoreError(message: value["error"] as? String ?? "Something went wrong.")))
        }
    }

    private static func json(_ object: [String: Any]) -> String? {
        guard let data = try? JSONSerialization.data(withJSONObject: object) else { return nil }
        return String(data: data, encoding: .utf8)
    }
}

private func coreEvent(_ json: UnsafePointer<CChar>?, _ context: UnsafeMutableRawPointer?) {
    guard let json, let context else { return }
    let data = Data(bytes: json, count: strlen(json))
    Unmanaged<CoreBridge>.fromOpaque(context).takeUnretainedValue().received(data)
}
