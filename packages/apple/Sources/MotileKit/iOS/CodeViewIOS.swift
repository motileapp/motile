#if os(iOS)
import SwiftUI
import UIKit

/// Puts the code view in SwiftUI.
struct CodeViewRepresentable: UIViewRepresentable {
    let document: CodeDocument
    var collapsed: Set<String> = []
    var reveal: (path: String, count: Int)?
    var marks = CodeMarks()
    var onToggle: (String) -> Void = { _ in }
    var onOpenFile: (String) -> Void = { _ in }
    var onViewed: (String) -> Void = { _ in }
    var onComment: (CodeSheet.Place) -> Void = { _ in }
    var onMedia: (CodeFile) -> Void = { _ in }
    var onOpenMedia: (CodeFile) -> Void = { _ in }

    func makeUIView(context: Context) -> CodeView { CodeView() }

    func updateUIView(_ view: CodeView, context: Context) {
        view.onToggle = onToggle
        view.onOpenFile = onOpenFile
        view.onViewed = onViewed
        view.onComment = onComment
        view.onMedia = onMedia
        view.onOpenMedia = onOpenMedia
        view.mark(marks)
        view.show(document, collapsed: collapsed)
        if let reveal { view.reveal(reveal.path, count: reveal.count) }
    }
}

/// Shows the files of a diff one under the other, or one whole file. A scroll view as large as
/// the document moves a canvas as large as the screen, which draws the lines that are on it, so
/// a document of any length costs what is seen of it. Holding a line selects it, and dragging
/// on selects the lines down to the finger.
final class CodeView: UIView, UIScrollViewDelegate, UIEditMenuInteractionDelegate {
    var onToggle: (String) -> Void = { _ in }
    var onOpenFile: (String) -> Void = { _ in }
    var onViewed: (String) -> Void = { _ in }
    var onComment: (CodeSheet.Place) -> Void = { _ in }
    var onMedia: (CodeFile) -> Void = { _ in }
    var onOpenMedia: (CodeFile) -> Void = { _ in }

    private let scrollView = UIScrollView()
    private let canvas = Canvas()
    private let sheet = CodeSheet()
    private var revealed = 0
    private var wanted: String?
    private var laidOut = CGSize.zero
    /// The line a selection set out from.
    private var held: CodeSheet.Place?
    private lazy var editMenu = UIEditMenuInteraction(delegate: self)

    override init(frame: CGRect) {
        super.init(frame: frame)
        scrollView.delegate = self
        scrollView.contentInsetAdjustmentBehavior = .never
        scrollView.isDirectionalLockEnabled = true
        scrollView.alwaysBounceVertical = true
        scrollView.keyboardDismissMode = .onDrag
        addSubview(scrollView)
        canvas.owner = self
        canvas.isUserInteractionEnabled = false
        addSubview(canvas)
        scrollView.addGestureRecognizer(UITapGestureRecognizer(target: self, action: #selector(tapped(_:))))
        let hold = UILongPressGestureRecognizer(target: self, action: #selector(held(_:)))
        hold.minimumPressDuration = 0.35
        scrollView.addGestureRecognizer(hold)
        addInteraction(editMenu)
        NotificationCenter.default.addObserver(self, selector: #selector(coloured(_:)), name: .codeColoured, object: nil)
        NotificationCenter.default.addObserver(self, selector: #selector(pictured(_:)), name: .codeMedia, object: nil)
        sheet.wantsMedia = { [weak self] file in self?.onMedia(file) }
        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (view: Self, _: UITraitCollection) in
            view.sheet.forgetLines()
            view.canvas.setNeedsDisplay()
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    deinit {
        NotificationCenter.default.removeObserver(self)
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        scrollView.frame = bounds
        canvas.frame = bounds
        guard bounds.size != laidOut else { return }
        laidOut = bounds.size
        _ = sheet.fit(width: bounds.width)
        fit()
        bringWantedIntoView()
        canvas.setNeedsDisplay()
    }

    /// The scroll view scrolls over at least what the view shows, so short documents fill it.
    private func fit() {
        scrollView.contentSize = CGSize(
            width: max(bounds.width, sheet.contentSize.width),
            height: max(bounds.height, sheet.contentSize.height))
    }

    func mark(_ marks: CodeMarks) {
        guard sheet.marks != marks else { return }
        sheet.marks = marks
        canvas.setNeedsDisplay()
    }

    func show(_ document: CodeDocument, collapsed: Set<String>) {
        guard sheet.document !== document || sheet.collapsed != collapsed else { return }
        let same = sheet.document?.id == document.id
        let anchor = same ? sheet.anchor(at: scrollView.contentOffset.y) : nil
        sheet.set(document, collapsed: collapsed, width: bounds.width)
        fit()
        let y = anchor.map { sheet.offset(of: $0) } ?? 0
        scroll(to: CGPoint(x: same ? scrollView.contentOffset.x : 0, y: y))
        bringWantedIntoView()
        canvas.setNeedsDisplay()
    }

    /// Brings the file's heading to the top, now or once the document that has it is shown.
    func reveal(_ path: String, count: Int) {
        guard count != revealed else { return }
        revealed = count
        wanted = path
        bringWantedIntoView()
    }

    private func bringWantedIntoView() {
        guard bounds.height > 0, let path = wanted, let top = sheet.top(ofFile: path) else { return }
        wanted = nil
        scroll(to: CGPoint(x: 0, y: top))
    }

    private func scroll(to point: CGPoint) {
        let limit = max(0, scrollView.contentSize.height - bounds.height)
        scrollView.contentOffset = CGPoint(x: max(0, point.x), y: min(max(0, point.y), limit))
        canvas.setNeedsDisplay()
    }

    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        canvas.setNeedsDisplay()
    }

    @objc private func coloured(_ notification: Notification) {
        guard notification.object as AnyObject? === sheet.document, let file = notification.userInfo?["file"] as? Int else { return }
        sheet.recolour(file: file)
        canvas.setNeedsDisplay()
    }

    /// A picture has arrived and takes the room it needs, while what is at the top stays there.
    @objc private func pictured(_ notification: Notification) {
        guard let document = sheet.document, document.files.contains(where: { $0.media === notification.object as AnyObject? }) else { return }
        let anchor = sheet.anchor(at: scrollView.contentOffset.y)
        sheet.set(document, collapsed: sheet.collapsed, width: bounds.width)
        fit()
        scroll(to: CGPoint(x: scrollView.contentOffset.x, y: anchor.map { sheet.offset(of: $0) } ?? scrollView.contentOffset.y))
    }

    /// Where the pinned heading of the file at the top is, in the document.
    private var pinnedHeading: (file: Int, frame: CGRect)? {
        let offset = scrollView.contentOffset
        guard let heading = sheet.pinnedHeading(at: offset.y) else { return nil }
        let frame = CGRect(x: offset.x, y: offset.y + heading.offset, width: bounds.width, height: CodeSheet.headingHeight)
        return (heading.file, frame)
    }

    fileprivate func draw(in context: CGContext) {
        let offset = scrollView.contentOffset
        let visible = CGRect(origin: offset, size: bounds.size)
        context.saveGState()
        context.translateBy(x: -offset.x, y: -offset.y)
        sheet.draw(visible: visible, dirty: visible, in: context)
        if let pinned = pinnedHeading {
            sheet.drawHeading(of: pinned.file, in: pinned.frame, lineAbove: false)
        }
        context.restoreGState()
    }

    // MARK: Taps and selection

    @objc private func tapped(_ recognizer: UITapGestureRecognizer) {
        Platform.endEditing()
        let point = recognizer.location(in: scrollView)
        let left = scrollView.contentOffset.x
        if let pinned = pinnedHeading, pinned.frame.contains(point) {
            return pressedHeading(of: pinned.file, at: point.x - left)
        }
        if sheet.document?.headed == true, let block = sheet.block(at: point.y), point.y < block.linesTop {
            return pressedHeading(of: block.file, at: point.x - left)
        }
        if let block = sheet.block(at: point.y), sheet.onMedia(point, of: block), let file = sheet.document?.files[block.file] {
            return onOpenMedia(file)
        }
        if sheet.marks.commentable, sheet.inGutter(point.x - left), let place = sheet.place(at: point), sheet.commentable(place) != nil {
            return onComment(place)
        }
        guard sheet.selection != nil else { return }
        sheet.selection = nil
        canvas.setNeedsDisplay()
    }

    private func pressedHeading(of file: Int, at x: CGFloat) {
        guard let document = sheet.document else { return }
        let path = document.files[file].path
        switch sheet.headingPress(at: x, width: bounds.width, file: file) {
        case .open: return onOpenFile(path)
        case .viewed: return onViewed(path)
        case .toggle: break
        }
        // A file that is closed from under its pinned heading leaves the view at its heading.
        if let top = sheet.top(ofFile: path), top < scrollView.contentOffset.y {
            scroll(to: CGPoint(x: scrollView.contentOffset.x, y: top))
        }
        onToggle(path)
    }

    @objc private func held(_ recognizer: UILongPressGestureRecognizer) {
        let point = recognizer.location(in: scrollView)
        switch recognizer.state {
        case .began:
            guard let block = sheet.block(at: point.y), point.y >= block.linesTop, let place = sheet.place(at: point) else { return }
            held = place
            sheet.selection = sheet.lines(place.line, through: place.line, of: place.file)
            UISelectionFeedbackGenerator().selectionChanged()
            canvas.setNeedsDisplay()
        case .changed:
            guard let start = held, let place = sheet.place(at: point, within: start.file) else { return }
            sheet.selection = sheet.lines(start.line, through: place.line, of: start.file)
            canvas.setNeedsDisplay()
        case .ended:
            guard held != nil, let frame = sheet.selectionFrame else { return }
            held = nil
            let offset = scrollView.contentOffset
            let shown = frame.offsetBy(dx: -offset.x, dy: -offset.y).intersection(bounds)
            let source = CGPoint(x: bounds.midX, y: shown.isNull ? point.y - offset.y : shown.minY)
            editMenu.presentEditMenu(with: UIEditMenuConfiguration(identifier: nil, sourcePoint: source))
        default:
            held = nil
        }
    }

    func editMenuInteraction(_ interaction: UIEditMenuInteraction, menuFor configuration: UIEditMenuConfiguration, suggestedActions: [UIMenuElement]) -> UIMenu? {
        guard let text = sheet.selectedText else { return nil }
        return UIMenu(children: [UIAction(title: "Copy") { _ in Platform.copy(text) }])
    }

    private final class Canvas: UIView {
        weak var owner: CodeView?

        override init(frame: CGRect) {
            super.init(frame: frame)
            isOpaque = false
            contentMode = .redraw
        }

        required init?(coder: NSCoder) { fatalError("not used") }

        override func draw(_ rect: CGRect) {
            guard let context = UIGraphicsGetCurrentContext() else { return }
            owner?.draw(in: context)
        }
    }
}
#endif
