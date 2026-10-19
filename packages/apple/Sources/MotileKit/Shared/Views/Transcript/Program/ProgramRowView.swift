import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// A program in the transcript: the views of its tree, what keeps it from running under them,
/// and its source while it has no tree.
final class ProgramRowView: RowView, ProgramHost {
    private static let errorGap: CGFloat = 8
    private var content: UiContent?
    private var root: ProgramNodeView?
    private let errors = ProgramTextView()
    private let source = ProgramCodeBox()

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(source)
        addSubview(errors)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .ui(let content) = row.kind else { return }
        self.content = content
    }

    /// How tall the row is in a column `width` wide; it lays the tree out, off the main thread.
    static func height(_ content: UiContent, width: CGFloat) -> CGFloat {
        let program = content.box(width: width)?.size.height ?? sourceHeight(content, width: width)
        let errors = errorText(content).map { errorGap + ProgramText.height($0.text, width: width, lineHeight: $0.lineHeight) } ?? 0
        return program + errors
    }

    private static func sourceHeight(_ content: UiContent, width: CGFloat) -> CGFloat {
        content.complete && !content.code.code.isEmpty ? ProgramCode.height(content.code, width: width) : 0
    }

    private static func errorText(_ content: UiContent) -> (text: NSAttributedString, lineHeight: CGFloat)? {
        guard content.complete, !content.errors.isEmpty else { return nil }
        let look = ProgramStyle.look(.caption, narrow: false)
        return (ProgramText.plain(content.errors.joined(separator: "\n"), look: look, color: Theme.destructive), look.lineHeight)
    }

    override func layout(width: CGFloat) -> CGFloat {
        guard let content else { return 0 }
        var y: CGFloat = 0
        if let box = content.box(width: width) {
            let view = root.flatMap { ProgramViews.fits($0, box.node) ? $0 : nil } ?? {
                root?.removeFromSuperview()
                let view = ProgramViews.make(for: box.node)
                addSubview(view)
                return view
            }()
            root = view
            view.frame = CGRect(origin: .zero, size: box.size)
            view.show(box, host: self)
            y = box.size.height
            source.isHidden = true
        } else {
            root?.removeFromSuperview()
            root = nil
            let height = Self.sourceHeight(content, width: width)
            source.isHidden = height == 0
            if height > 0 { source.show(content.code, surface: .background) }
            source.frame = CGRect(x: 0, y: 0, width: width, height: height)
            y = height
        }
        if let error = Self.errorText(content) {
            let height = ProgramText.height(error.text, width: width, lineHeight: error.lineHeight)
            errors.part = ProgramTextPart(text: error.text, frame: .zero, lift: ControlText.lift(Theme.smallFont, lineHeight: error.lineHeight))
            errors.frame = CGRect(x: 0, y: y + Self.errorGap, width: width, height: height)
            errors.isHidden = false
            y += Self.errorGap + height
        } else {
            errors.isHidden = true
        }
        return y
    }

    // MARK: ProgramHost

    func send(_ node: String, _ event: JSON) {
        owner?.programEvent(rowID: rowID, node: node, event: event)
    }

    func media(_ id: String, done: @escaping (URL?) -> Void) {
        guard let owner else { return done(nil) }
        owner.media(id: id, done: done)
    }
}

#if DEBUG
/// The frame of every node of a program laid out in a column of a width, relative to the row, as
/// the views are placed: `{"<node id>": [x, y, width, height]}`. For the test that compares the
/// clients' layouts.
enum ProgramFrames {
    static func frames(tree: JSON, width: CGFloat, window: CGSize = CGSize(width: 1200, height: 800)) -> [String: [Double]] {
        guard let node = UiNode(json: tree) else { return [:] }
        let box = ProgramLayout.layout(node, width: width, env: ProgramEnv(column: width, window: window))
        var frames: [String: [Double]] = [:]
        collect(box, at: .zero, into: &frames)
        return frames
    }

    /// The frames as JSON, with its keys in order.
    static func json(tree: JSON, width: CGFloat) -> String {
        let frames = frames(tree: tree, width: width)
        guard let data = try? JSONSerialization.data(withJSONObject: frames, options: [.sortedKeys, .withoutEscapingSlashes]) else { return "{}" }
        return String(data: data, encoding: .utf8) ?? "{}"
    }

    private static func collect(_ box: ProgramBox, at origin: CGPoint, into frames: inout [String: [Double]]) {
        frames[box.id] = [origin.x, origin.y, box.size.width, box.size.height].map { (Double($0) * 100).rounded() / 100 }
        for (child, place) in zip(box.children, box.origins) {
            collect(child, at: CGPoint(x: origin.x + place.x, y: origin.y + place.y), into: &frames)
        }
    }
}
#endif
