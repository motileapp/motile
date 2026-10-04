import CMotileCore
import Foundation

/// The client's end of the Rust core: commands go in as JSON, events come out as JSON. Events are
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
    /// What turns a large answer into what the client keeps, off the main thread, and where that goes.
    private var readers: [UInt64: ([String: Any]) -> Any] = [:]
    private var readReplies: [UInt64: (Result<Any, CoreError>) -> Void] = [:]
    private let readersLock = NSLock()

    func start(config: [String: Any]) -> Bool {
        guard let json = Self.json(config) else { return false }
        let context = Unmanaged.passUnretained(self).toOpaque()
        return json.withCString { motile_start($0, coreEvent, context) }
    }

    /// Sends a command. `reply` is called on the main thread with the answer. Answers with the
    /// command's id, which the events that belong to it carry.
    @discardableResult
    func send(_ type: String, _ fields: [String: Any] = [:], reply: Reply? = nil) -> UInt64 {
        dispatchPrecondition(condition: .onQueue(.main))
        nextID += 1
        if let reply { replies[nextID] = reply }
        post(type, fields, id: nextID)
        return nextID
    }

    /// Sends a command whose answer is large: `read` turns it into what the client keeps, off the
    /// main thread, and `reply` gets that on the main thread.
    @discardableResult
    func send<Read>(
        _ type: String, _ fields: [String: Any], read: @escaping ([String: Any]) -> Read, reply: @escaping (Result<Read, CoreError>) -> Void
    ) -> UInt64 {
        dispatchPrecondition(condition: .onQueue(.main))
        nextID += 1
        readersLock.withLock { readers[nextID] = read }
        readReplies[nextID] = { result in reply(result.map { $0 as! Read }) }
        post(type, fields, id: nextID)
        return nextID
    }

    private func post(_ type: String, _ fields: [String: Any], id: UInt64) {
        var command = fields
        command["type"] = type
        command["id"] = id
        guard let json = Self.json(command) else { return }
        json.withCString { motile_send($0) }
    }

    fileprivate func received(_ data: Data) {
        decoding.async { [weak self] in
            guard let self, let event = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return }
            if event["type"] as? String == "reply" {
                let id = (event["id"] as? NSNumber)?.uint64Value ?? 0
                let value = event["value"] as? [String: Any] ?? [:]
                let reader = self.readersLock.withLock { self.readers.removeValue(forKey: id) }
                let read = event["ok"] as? Bool == true ? reader?(value) : nil
                DispatchQueue.main.async { self.answer(id: id, ok: event["ok"] as? Bool == true, value: value, read: read) }
                return
            }
            guard let apply = self.decode?(event) else { return }
            DispatchQueue.main.async(execute: apply)
        }
    }

    private func answer(id: UInt64, ok: Bool, value: [String: Any], read: Any?) {
        let failure = CoreError(message: value["error"] as? String ?? "Something went wrong.")
        if let reply = readReplies.removeValue(forKey: id) {
            reply(read.map { .success($0) } ?? .failure(failure))
            return
        }
        guard let reply = replies.removeValue(forKey: id) else { return }
        reply(ok ? .success(value) : .failure(failure))
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
