import CMotileCore
import Foundation
import Observation

/// What a mark is drawn with.
enum MarkupTool: String, CaseIterable {
    case pen, arrow, box, pin

    var symbol: Symbol {
        switch self {
        case .pen: .pencil
        case .arrow: .moveUpRight
        case .box: .square
        case .pin: .mapPin
        }
    }

    var title: String {
        switch self {
        case .pen: "Pen"
        case .arrow: "Arrow"
        case .box: "Box"
        case .pin: "Pin a Note"
        }
    }

    /// The key that picks it on a keyboard.
    var key: String {
        switch self {
        case .pen: "P"
        case .arrow: "A"
        case .box: "B"
        case .pin: "N"
        }
    }
}

/// An image marked up and written out for the composer, with what to go on from.
struct MarkupResult {
    let file: URL
    /// The pins' notes, which go in the message.
    let notes: String?
    let saved: SavedMarkup
}

/// What a marked-up attachment keeps to be marked up again: the image before the marks, and them.
struct SavedMarkup: Equatable {
    let original: URL
    let name: String
    let marks: String
    let notes: String?
}

/// An image being marked up in the viewer, through the core (`markup::markup_json`): the pointer
/// goes in, in the image's pixels, and the scene of its marks comes back to paint over it.
@MainActor @Observable
final class MarkupSession {
    struct Pin: Decodable, Equatable {
        let mark: Int
        let number: Int
        let x: CGFloat
        let y: CGFloat
        let radius: CGFloat
        let note: String
        let ink: String
    }

    private struct State: Decodable {
        let id: UInt64
        let width: CGFloat
        let height: CGFloat
        let scene: DecodedScene
        let pins: [Pin]
        let selected: Int?
        let canUndo: Bool
        let canRedo: Bool
        let marked: Bool
        let inks: [String]
    }

    private static var lastTool = MarkupTool.pen
    private static var lastInk = "red"

    let source: URL
    let name: String
    /// The composer's attachment it replaces, when it is one.
    let attachment: String?
    private(set) var id: UInt64 = 0
    private(set) var width: CGFloat = 1
    private(set) var height: CGFloat = 1
    private(set) var scene: CanvasScene?
    private(set) var pins: [Pin] = []
    private(set) var selected: Int?
    private(set) var canUndo = false
    private(set) var canRedo = false
    private(set) var marked = false
    private(set) var inks: [String] = []
    /// Whether anything was drawn since it opened, which cancelling throws away.
    private(set) var changed = false
    private(set) var finishing = false
    var tool = MarkupSession.lastTool {
        didSet { Self.lastTool = tool }
    }
    var ink = MarkupSession.lastInk {
        didSet {
            Self.lastInk = ink
            inksOpen = false
        }
    }
    /// The selected pin's note as it is being written.
    var note = ""
    /// On a phone the colours open from the one picked, to leave room for the tools.
    var inksOpen = false

    /// `width` and `height` are the pixels of the picture shown; `saved` goes on from an earlier markup.
    init?(source: URL, name: String, attachment: String?, width: Int, height: Int, saved: SavedMarkup?) {
        self.source = source
        self.name = name
        self.attachment = attachment
        var open: JSON = ["op": "open", "width": width, "height": height]
        open["marks"] = saved?.marks
        guard apply(Self.ask(open)) else { return nil }
    }

    var selectedPin: Pin? { pins.first { $0.mark == selected } }

    func down(at point: CGPoint, slop: CGFloat) {
        commitNote()
        send(["op": "down", "tool": tool.rawValue, "ink": ink, "x": point.x, "y": point.y, "slop": slop])
    }

    func drag(to point: CGPoint) {
        send(["op": "move", "x": point.x, "y": point.y])
    }

    func up(at point: CGPoint) {
        send(["op": "up", "x": point.x, "y": point.y])
    }

    /// Drops the mark the pointer started, as a pinch does.
    func cancel() {
        send(["op": "cancel"])
    }

    /// Keeps the selected pin's note and puts the pin down.
    func deselect() {
        commitNote()
        send(["op": "select"])
    }

    func removeSelected() {
        guard let selected else { return }
        send(["op": "remove", "mark": selected])
    }

    func undo() {
        commitNote()
        send(["op": "undo"])
    }

    func redo() {
        commitNote()
        send(["op": "redo"])
    }

    /// Writes the image out with its marks, off the main thread.
    func finish(into folder: URL, done: @escaping (MarkupResult?) -> Void) {
        commitNote()
        finishing = true
        let (id, source, name) = (id, source, name)
        DispatchQueue.global(qos: .userInitiated).async {
            let answer = Self.ask(["op": "flatten", "id": id, "source": source.path, "folder": folder.path, "name": name])
            let result = Self.result(answer, source: source, name: name)
            DispatchQueue.main.async {
                self.finishing = false
                done(result)
            }
        }
    }

    func close() {
        _ = Self.ask(["op": "close", "id": id])
    }

    private func commitNote() {
        guard let pin = selectedPin, pin.note != note.trimmingCharacters(in: .whitespacesAndNewlines) else { return }
        send(["op": "note", "mark": pin.mark, "text": note])
    }

    private func send(_ input: JSON) {
        var input = input
        input["id"] = id
        apply(Self.ask(input))
        if input["op"] as? String != "select" { changed = true }
    }

    @discardableResult
    private func apply(_ answer: Data?) -> Bool {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let answer, let state = try? decoder.decode(State.self, from: answer) else { return false }
        let wasSelected = selected
        (id, width, height, scene, pins) = (state.id, state.width, state.height, state.scene.scene, state.pins)
        (selected, canUndo, canRedo, marked, inks) = (state.selected, state.canUndo, state.canRedo, state.marked, state.inks)
        if selected != wasSelected { note = selectedPin?.note ?? "" }
        return true
    }

    /// The composer's text with an image's notes in the place of the ones it had (`markup::place_notes`).
    nonisolated static func placingNotes(_ notes: String?, over earlier: String?, in text: String) -> String {
        var input: JSON = ["op": "place_notes", "text": text]
        input["notes"] = notes
        input["earlier"] = earlier
        guard let answer = ask(input), let json = try? JSONSerialization.jsonObject(with: answer) as? JSON else { return text }
        return json["text"] as? String ?? text
    }

    nonisolated private static func ask(_ input: JSON) -> Data? {
        guard let data = try? JSONSerialization.data(withJSONObject: input), let json = String(data: data, encoding: .utf8) else { return nil }
        guard let answer = json.withCString({ motile_markup($0) }) else { return nil }
        defer { motile_free_string(answer) }
        return Data(String(cString: answer).utf8)
    }

    /// Keeps the image as it was beside the marked one, to be marked up again.
    nonisolated private static func result(_ answer: Data?, source: URL, name: String) -> MarkupResult? {
        guard let answer, let json = try? JSONSerialization.jsonObject(with: answer) as? JSON else { return nil }
        guard let path = json["file"] as? String, let marks = json["marks"] as? String else { return nil }
        let file = URL(fileURLWithPath: path)
        let original = file.deletingLastPathComponent().appendingPathComponent("original", isDirectory: true)
            .appendingPathComponent(source.lastPathComponent)
        do {
            try FileManager.default.createDirectory(at: original.deletingLastPathComponent(), withIntermediateDirectories: true)
            try FileManager.default.copyItem(at: source, to: original)
        } catch {
            return nil
        }
        let notes = json["notes"] as? String
        return MarkupResult(file: file, notes: notes, saved: SavedMarkup(original: original, name: name, marks: marks, notes: notes))
    }
}
