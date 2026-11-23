import Foundation
import QuartzCore

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// What a program's views need from the row they are in.
protocol ProgramHost: AnyObject {
    /// The program's row.
    var rowID: String { get }
    /// Tells the core what the user did to the node.
    func send(_ node: String, _ event: JSON)
    /// Hands over the file of an image the program shows.
    func media(_ id: String, done: @escaping (URL?) -> Void)
    /// The node's height changed outside the tree: a page reported its own.
    func resized(_ node: String)
    /// The program still streams in.
    var streaming: Bool { get }
    /// Tells the image whenever the viewport moves.
    func watchViewport(_ image: ProgramImageView)
    /// Opens an HtmlView's page over the window.
    func expand(_ page: ExpandedPage)
}

extension ProgramHost {
    var streaming: Bool { false }
    func expand(_ page: ExpandedPage) {}
    func press(_ node: String) { send(node, ["type": "press"]) }
    func value(_ node: String, _ value: Any) { send(node, ["type": "value", "value": value]) }
}

/// The view of one node of a program, placed at its box. It shows what its box says, then lays
/// its children's views at their places, keeping the view of each node it already had, so that a
/// field being typed in or a track being scrolled stays as it is when the tree changes.
class ProgramNodeView: FlippedView {
    weak var host: ProgramHost?
    /// The kind of node it was made for.
    fileprivate(set) var made = ""
    private(set) var box: ProgramBox?
    private var childViews: [String: ProgramNodeView] = [:]
    private var textViews: [String: ProgramTextView] = [:]
    /// The view its children's views go in.
    var holder: PlatformView { self }
    /// The view its texts go in.
    var textHolder: PlatformView { self }

    final func show(_ box: ProgramBox, host: ProgramHost?) {
        self.host = host
        self.box = box
        configure(box)
        showTexts(box.texts)
        placeChildren(box)
        didShow(box)
        ProgramAccess.describe(self, box: box)
        relayout()
    }

    /// Shows what the box says, besides its texts and children.
    func configure(_ box: ProgramBox) {}

    /// Called once its texts and children show the box too.
    func didShow(_ box: ProgramBox) {}

    var nodeID: String { box?.id ?? "" }

    private func showTexts(_ texts: [String: ProgramTextPart]) {
        for (key, view) in textViews where texts[key] == nil {
            view.removeFromSuperview()
            textViews.removeValue(forKey: key)
        }
        for (key, part) in texts {
            let view = textViews[key] ?? {
                let view = ProgramTextView()
                textHolder.addSubview(view)
                textViews[key] = view
                return view
            }()
            view.onLink = { [weak self] url in self?.open(url) }
            view.part = part
            view.frame = part.frame
        }
    }

    func text(_ key: String) -> ProgramTextView? { textViews[key] }

    /// The texts the node draws itself, besides its children's.
    var ownSelectables: [ProgramSelectable] { Array(textViews.values) }

    private func placeChildren(_ box: ProgramBox) {
        var kept: [String: ProgramNodeView] = [:]
        for (child, origin) in zip(box.children, box.origins) {
            let reused = childViews[child.id].flatMap { ProgramViews.fits($0, child.node) ? $0 : nil }
            let view = reused ?? ProgramViews.make(for: child.node)
            if view.superview !== holder { holder.addSubview(view) }
            view.frame = CGRect(origin: origin, size: child.size)
            view.show(child, host: host)
            kept[child.id] = view
        }
        for (id, view) in childViews where kept[id] !== view { view.removeFromSuperview() }
        childViews = kept
    }

    /// The views of its children, by node id, for the frame dump's checks and for focus.
    var children: [String: ProgramNodeView] { childViews }

    /// Lets go of what only lives while the row is on screen, its web pages.
    func leave() {
        for view in childViews.values { view.leave() }
    }

    func open(_ url: URL) {
        Platform.open(url)
    }

    /// Whether the view's node is drawn disabled.
    func disabled(_ reason: UiDisabled?) -> Bool { reason != nil }

    /// Rounds the view's corners and clips what is inside it to them, or neither with no radius.
    func clip(radius: CGFloat?) {
        #if os(macOS)
        wantsLayer = true
        layer?.cornerRadius = radius ?? 0
        layer?.cornerCurve = (radius ?? 0) * 2 >= min(bounds.width, bounds.height) - 0.5 ? .circular : .continuous
        layer?.masksToBounds = radius != nil
        #else
        layer.cornerRadius = radius ?? 0
        layer.cornerCurve = (radius ?? 0) * 2 >= min(bounds.width, bounds.height) - 0.5 ? .circular : .continuous
        clipsToBounds = radius != nil
        #endif
    }
}

/// Text a node draws: lines of a fixed height, cut short where its box says, with links that open
/// in the browser. On the Mac a program's row selects it with its other texts; on iOS it is
/// selected on its own.
final class ProgramTextView: FlippedView, ProgramSelectable {
    var part: ProgramTextPart? {
        didSet {
            laid = nil
            #if os(iOS)
            if let selected, NSMaxRange(selected) > selectableText.length { clearSelection() }
            #endif
            canvas.redraw()
            hasLinks = part.map { Self.hasLinks($0.text) } ?? false
            hasCode = part.map { ProgramText.hasCode($0.text) } ?? false
        }
    }
    var onLink: ((URL) -> Void)?
    var selected: NSRange?
    private var laid: (width: CGFloat, storage: NSTextStorage, geometry: TextGeometry)?
    /// Draws the text, wider than the view by the room inline code's box reaches past its glyphs.
    private let canvas = TextCanvas()
    private var hasCode = false
    private var hasLinks = false {
        didSet {
            #if os(iOS)
            guard hasLinks != oldValue else { return }
            onPress = hasLinks ? { [weak self] point in self?.press(at: point) } : nil
            pointer = hasLinks ? .hand : .arrow
            #endif
        }
    }
    #if os(iOS)
    weak var inputDelegate: UITextInputDelegate?
    lazy var tokenizer: UITextInputTokenizer = UITextInputStringTokenizer(textInput: self)
    #endif

    override init(frame: CGRect) {
        super.init(frame: frame)
        canvas.frame = bounds.insetBy(dx: -RowTextView.overhang, dy: 0)
        canvas.drawing = { [weak self] in self?.drawText() }
        addSubview(canvas)
        #if os(iOS)
        addTextInteraction()
        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (view: Self, _: UITraitCollection) in view.canvas.redraw() }
        #endif
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func layoutNow() {
        canvas.frame = bounds.insetBy(dx: -RowTextView.overhang, dy: 0)
    }

    #if os(macOS)
    /// Outside a row, as in a popover's panel, a click on a link opens it; in one the row does.
    override func mouseDown(with event: NSEvent) {
        guard programRow == nil, let url = link(at: convert(event.locationInWindow, from: nil)) else { return super.mouseDown(with: event) }
        onLink?(url)
    }

    override func cursor(at point: CGPoint) -> NSCursor {
        guard link(at: point) == nil else { return .pointingHand }
        return programRow != nil ? .iBeam : .arrow
    }
    #else
    override var canBecomeFirstResponder: Bool { true }

    override func copy(_ sender: Any?) {
        guard let selected, selected.length > 0 else { return }
        Platform.copy(TextSystem.withLineBreaks(Typesetter.words(of: selectableText.attributedSubstring(from: selected))))
    }

    override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        action == #selector(copy(_:)) && (selected?.length ?? 0) > 0
    }
    #endif

    private func drawText() {
        guard let part, let context = Platform.drawing else { return }
        context.saveGState()
        defer { context.restoreGState() }
        context.translateBy(x: RowTextView.overhang, y: 0)
        #if os(macOS)
        if let selected, let geometry = textGeometry {
            context.setFillColor(NSColor.selectedTextBackgroundColor.cgColor)
            context.fill(geometry.rects(of: selected))
        }
        #endif
        if hasCode, let geometry = textGeometry {
            ProgramText.codeRanges(in: part.text).forEach { geometry.layout.drawInlineCode($0, at: geometry.origin) }
        }
        let area = CGRect(x: 0, y: -part.lift, width: bounds.width, height: bounds.height + part.lift)
        guard part.oneLine || part.maxLines != nil else {
            return part.text.draw(with: area, options: [.usesLineFragmentOrigin, .usesFontLeading], context: nil)
        }
        part.text.draw(with: area, options: [.usesLineFragmentOrigin, .usesFontLeading, .truncatesLastVisibleLine], context: nil)
    }

    // MARK: ProgramSelectable

    var selectableText: NSAttributedString { part?.text ?? NSAttributedString() }

    var textGeometry: TextGeometry? {
        guard let part else { return nil }
        if let laid, laid.width == bounds.width { return laid.geometry }
        let storage = NSTextStorage(attributedString: part.text)
        let layout = NSLayoutManager()
        let container = NSTextContainer(size: CGSize(width: max(1, bounds.width), height: .greatestFiniteMagnitude))
        container.lineFragmentPadding = 0
        layout.addTextContainer(container)
        storage.addLayoutManager(layout)
        let geometry = TextGeometry(layout: layout, container: container, origin: CGPoint(x: 0, y: -part.lift), lineShift: part.lift)
        laid = (bounds.width, storage, geometry)
        return geometry
    }

    #if os(macOS)
    func showSelected(_ range: NSRange?) {
        guard range != selected else { return }
        selected = range
        canvas.redraw()
    }
    #endif

    func link(at point: CGPoint) -> URL? {
        guard hasLinks, bounds.contains(point) else { return nil }
        return textGeometry?.link(at: point)
    }

    #if os(iOS)
    private func press(at point: CGPoint) {
        guard let url = link(at: point) else { return }
        onLink?(url)
    }

    override func takesPress(at point: CGPoint) -> Bool {
        link(at: point) != nil
    }
    #endif

    private static func hasLinks(_ text: NSAttributedString) -> Bool {
        var found = false
        text.enumerateAttribute(.link, in: NSRange(location: 0, length: text.length)) { value, _, stop in
            guard value != nil else { return }
            found = true
            stop.pointee = true
        }
        return found
    }
}

/// Draws what its owner does, in a view that reaches past the owner's sides.
final class TextCanvas: FlippedView {
    var drawing: (() -> Void)?

    override init(frame: CGRect) {
        super.init(frame: frame)
        #if os(macOS)
        autoresizingMask = [.width, .height]
        #else
        autoresizingMask = [.flexibleWidth, .flexibleHeight]
        isUserInteractionEnabled = false
        #endif
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    #if os(macOS)
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
    #endif

    override func draw(_ rect: CGRect) {
        drawing?()
    }
}

/// Which view shows which node.
enum ProgramViews {
    static func make(for node: UiNode) -> ProgramNodeView {
        let view = blank(for: node)
        view.made = key(node)
        return view
    }

    private static func blank(for node: UiNode) -> ProgramNodeView {
        switch node.kind {
        case .col, .row, .box, .grid, .card, .form, .list: ProgramStackView()
        case .pressable: ProgramPressableView()
        case .carousel: ProgramCarouselView()
        case .popover: ProgramPopoverView()
        case .divider: ProgramDividerView()
        case .title, .text, .caption, .label, .quote: ProgramPartsView()
        case .link: ProgramLinkView()
        case .badge: ProgramBadgeView()
        case .listItem: ProgramListItemView()
        case .markdown: ProgramMarkdownView()
        case .code: ProgramCodeView()
        case .button, .iconButton: ProgramButtonView()
        case .input: ProgramInputView()
        case .textArea: ProgramTextAreaView()
        case .select: ProgramSelectView()
        case .segmentedControl: ProgramSegmentedView()
        case .slider: ProgramSliderView()
        case .checkbox: ProgramCheckboxView()
        case .radioGroup: ProgramRadioView()
        case .datePicker: ProgramDateView()
        case .timer: ProgramTimerView()
        case .searchImage, .image: ProgramImageView()
        case .icon: ProgramIconView()
        case .table: ProgramTableView()
        case .chart: ProgramChartView()
        case .svg, .math: ProgramSceneView()
        case .flashcard: ProgramFlashcardView()
        case .celebration: ProgramCelebrationView()
        case .map: ProgramMapView()
        case .youTube, .htmlView: ProgramWebView()
        case .spacer, .mapMarker, .g, .circle, .ellipse, .rect, .line, .path, .polyline, .polygon, .svgText: ProgramNodeView()
        }
    }

    /// What a node's view is made for: its component, so that a view only ever shows one kind.
    static func key(_ node: UiNode) -> String {
        switch node.kind {
        case .col: "col"
        case .row: "row"
        case .box: "box"
        case .grid: "grid"
        case .card: "card"
        case .carousel: "carousel"
        case .popover: "popover"
        case .divider: "divider"
        case .spacer: "spacer"
        case .title: "title"
        case .text: "text"
        case .caption: "caption"
        case .label: "label"
        case .badge: "badge"
        case .link: "link"
        case .list: "list"
        case .listItem: "listItem"
        case .markdown: "markdown"
        case .quote: "quote"
        case .code: "code"
        case .math: "math"
        case .button: "button"
        case .iconButton: "iconButton"
        case .pressable: "pressable"
        case .input: "input"
        case .textArea: "textArea"
        case .select: "select"
        case .segmentedControl: "segmentedControl"
        case .slider: "slider"
        case .checkbox: "checkbox"
        case .radioGroup: "radioGroup"
        case .datePicker: "datePicker"
        case .timer: "timer"
        case .form: "form"
        case .searchImage: "searchImage"
        case .image: "image"
        case .icon: "icon"
        case .table: "table"
        case .chart: "chart"
        case .map: "map"
        case .mapMarker: "mapMarker"
        case .youTube: "youTube"
        case .flashcard: "flashcard"
        case .celebration: "celebration"
        case .svg: "svg"
        case .g: "g"
        case .circle: "circle"
        case .ellipse: "ellipse"
        case .rect: "rect"
        case .line: "line"
        case .path: "path"
        case .polyline: "polyline"
        case .polygon: "polygon"
        case .svgText: "svgText"
        case .htmlView: "htmlView"
        }
    }

    /// Whether the view can show the node: it was made for a node of its kind.
    static func fits(_ view: ProgramNodeView, _ node: UiNode) -> Bool {
        view.made == key(node)
    }
}

// MARK: Layout and text

/// A column, a row, a box, a grid, a card, a form or a list: its children on its fill.
final class ProgramStackView: ProgramNodeView {
    private let face = SurfaceView()

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(face)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ box: ProgramBox) {
        var radius: CGFloat?
        face.fill = .clear
        face.stroke = nil
        switch box.node.kind {
        case .box(let node):
            face.fill = node.background.map { ProgramStyle.color($0, on: box.surface) } ?? .clear
            face.stroke = node.border ? box.surface.platformBorder : nil
            radius = node.radius.map { ProgramStyle.radius($0, side: min(box.size.width, box.size.height)) }
        case .card(let card):
            face.fill = card.soft ? box.surface.platform(.box) : Theme.card
            face.stroke = card.soft ? nil : Theme.borderCard
            face.radius = ProgramStyle.radius(card.radius, side: min(box.size.width, box.size.height))
            clip(radius: CGFloat(card.padding ?? 16) < face.radius ? face.radius : nil)
        default:
            break
        }
        if case .box = box.node.kind {
            face.radius = radius ?? 0
            clip(radius: radius)
        }
    }

    override func layoutNow() {
        face.frame = bounds
    }
}

/// A Pressable: a column that takes presses as a whole, lit as Motile's rows are.
final class ProgramPressableView: ProgramNodeView {
    private let face = SurfaceView()
    private let wash = SurfaceView()
    private let ring = SurfaceView()
    private let mark = PickMarkView(multiple: false)
    private var lit = false

    override init(frame: CGRect) {
        super.init(frame: frame)
        for view in [face, wash, ring] as [PlatformView] { addSubview(view) }
        addSubview(mark)
        focusable = true
        onPress = { [weak self] _ in
            guard let self, case .pressable(let node) = box?.node.kind, node.disabled == nil else { return }
            host?.press(nodeID)
        }
        onHover = { [weak self] point in
            self?.lit = point != nil
            self?.paint()
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ box: ProgramBox) {
        guard case .pressable(let node) = box.node.kind else { return }
        let radius = ProgramStyle.radius(node.radius, side: min(box.size.width, box.size.height))
        for view in [face, wash, ring] { view.radius = radius }
        clip(radius: CGFloat(node.padding) < radius ? radius : nil)
        ring.stroke = node.ring ? Theme.borderAccent : nil
        mark.picked = true
        mark.isHidden = !node.ring
        opacity = node.disabled == nil ? 1 : Opacity.disabled.fixed
        paint()
    }

    private func paint() {
        guard let box, case .pressable(let node) = box.node.kind else { return }
        let lit = self.lit && node.disabled == nil
        if let background = node.background {
            face.fill = ProgramStyle.color(background, on: box.surface)
            wash.fill = lit ? Theme.thinned(Theme.foreground, .colorTint) : .clear
        } else {
            face.fill = node.selected == true ? box.surface.platform(.rowSelected) : lit ? box.surface.platform(.row) : .clear
            wash.fill = .clear
        }
    }

    /// A press on a control inside it is the control's.
    override func takesPress(at point: CGPoint) -> Bool {
        bounds.contains(point) && !ProgramControls.control(in: self, at: point)
    }

    override var focusShape: (rect: CGRect, radius: CGFloat) { (bounds, face.radius) }

    override func layoutNow() {
        for view in [face, wash, ring] { view.frame = bounds }
        let side = PickMarkView.side
        mark.frame = CGRect(x: bounds.width - 8 - side, y: 8, width: side, height: side)
        #if os(macOS)
        mark.removeFromSuperview()
        addSubview(mark)
        #else
        bringSubviewToFront(mark)
        #endif
    }
}

/// Finds the controls inside a view that take their own presses.
enum ProgramControls {
    static func control(in view: PlatformView, at point: CGPoint) -> Bool {
        for subview in view.subviews.reversed() where !subview.isHidden && subview.frame.contains(point) {
            let inner = subview.convert(point, from: view)
            switch subview {
            case let text as ProgramTextView:
                if text.takesPress(at: inner) { return true }
            case is ProgramButtonView, is ProgramFieldHolder, is ProgramPressableView, is ProgramPopoverView, is ProgramLinkView:
                return true
            default:
                if control(in: subview, at: inner) { return true }
            }
        }
        return false
    }
}

/// Title, Text, Caption, Label and Quote: their texts, and a quote's bar.
final class ProgramPartsView: ProgramNodeView {
    private let bar = SurfaceView()

    override init(frame: CGRect) {
        super.init(frame: frame)
        bar.isHidden = true
        addSubview(bar)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ box: ProgramBox) {
        bar.isHidden = box.rects["bar"] == nil
        bar.fill = box.surface.platformBorder
        bar.frame = box.rects["bar"] ?? .zero
    }
}

/// A Link: its words in the link colour and an arrow after them, opening its address.
final class ProgramLinkView: ProgramNodeView {
    private let arrow = SymbolView()

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(arrow)
        pointer = .hand
        onPress = { [weak self] _ in
            guard let self, case .link(let link) = box?.node.kind, let url = link.url.flatMap(URL.init(string:)) else { return }
            open(url)
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ box: ProgramBox) {
        guard case .link(let link) = box.node.kind else { return }
        arrow.isHidden = link.url == nil
        let frame = box.rects["arrow"] ?? .zero
        arrow.image = .lucide(ProgramGlyphs.arrowUpRight, side: frame.width)
        arrow.tint = Theme.primary
        arrow.frame = frame
    }
}

/// The characters in Lucide's font of the icons the views draw that `Symbol` doesn't have.
enum ProgramGlyphs {
    static let arrowUpRight = "\u{e04d}"
    static let image = "\u{e0f6}"
}

/// A Badge: Motile's chip, in its tone on a wash of it, or bordered without one.
final class ProgramBadgeView: ProgramNodeView {
    private let face = SurfaceView()
    private let icon = SymbolView()
    private let words = ProgramTextView()

    override init(frame: CGRect) {
        super.init(frame: frame)
        for view in [face, icon, words] as [PlatformView] { addSubview(view) }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ box: ProgramBox) {
        guard case .badge(let badge) = box.node.kind else { return }
        let tone = badge.tone.flatMap(Theme.named)
        let ink = tone ?? Theme.foreground
        face.fill = tone.map { Theme.thinned($0, .colorTint) } ?? box.surface.platform(.control)
        face.stroke = tone == nil ? box.surface.platformBorder : nil
        face.radius = Chip.height / 2
        icon.isHidden = badge.icon == nil
        if let glyph = badge.icon?.glyph { icon.image = .lucide(glyph, side: PlatformImage.symbolSide(ProgramBadge.iconSize)) }
        icon.tint = ink
        let text = ControlText.line(badge.text, font: ProgramBadge.font, color: ink)
        words.part = ProgramTextPart(text: text, frame: .zero, lift: ControlText.lift(text), oneLine: true)
    }

    override var ownSelectables: [ProgramSelectable] { [words] }

    override func layoutNow() {
        face.frame = bounds
        var x = ProgramBadge.padding
        if !icon.isHidden {
            let side = PlatformImage.symbolSide(ProgramBadge.iconSize)
            icon.frame = CGRect(x: x, y: ((bounds.height - side) / 2).rounded(), width: side, height: side)
            x += side + ProgramBadge.gap
        }
        let height = ControlText.height(words.selectableText, width: .greatestFiniteMagnitude)
        words.frame = CGRect(x: x, y: ((bounds.height - height) / 2).rounded(), width: max(0, bounds.width - x - ProgramBadge.padding), height: height)
    }
}

final class ProgramDividerView: ProgramNodeView {
    private let line = SurfaceView()

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(line)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ box: ProgramBox) {
        line.fill = box.surface.platformBorder
    }

    override func layoutNow() {
        line.frame = CGRect(x: 0, y: 0, width: bounds.width, height: 1)
    }
}

/// A list's item: its marker in the lane, its title, what it means and its children, and the line
/// of a timeline down to the next.
final class ProgramListItemView: ProgramNodeView {
    private let line = SurfaceView()
    private let marker = SurfaceView()

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(line)
        addSubview(marker)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ box: ProgramBox) {
        line.fill = Theme.borderAccent
        line.frame = box.rects["line"] ?? .zero
        line.isHidden = box.rects["line"] == nil
        let frame = box.rects["marker"] ?? .zero
        marker.frame = frame
        marker.radius = frame.height / 2
        marker.stroke = nil
        marker.isHidden = frame.isEmpty
        marker.lineWidth = 1
        switch box.extra as? ProgramList.Kind ?? .none {
        case .none, .label: marker.isHidden = true
        case .dot: marker.fill = Theme.mutedStrongerForeground
        case .number, .chip: marker.fill = Theme.foreground
        case .ring:
            marker.fill = box.surface.platform
            marker.stroke = Theme.foreground
            marker.lineWidth = 2
        }
    }
}

/// Markdown: its paragraphs set as prose is, and its code in boxes.
final class ProgramMarkdownView: ProgramNodeView {
    private var texts: [RowTextView] = []
    private var codes: [ProgramCodeBox] = []
    private var headers: [[CodeHeader]] = []
    #if os(iOS)
    private var tables: [[TableView]] = []
    #endif

    override func configure(_ box: ProgramBox) {
        let blocks = box.extra as? [ProgramProse.Block] ?? []
        let prose = blocks.filter { $0.prose != nil }
        let code = blocks.filter { $0.code != nil }
        while texts.count < prose.count {
            let text = RowTextView.make()
            text.inProgram = true
            addSubview(text)
            texts.append(text)
            headers.append([])
            #if os(iOS)
            tables.append([])
            #endif
        }
        while codes.count < code.count {
            let view = ProgramCodeBox()
            addSubview(view)
            codes.append(view)
        }
        for (index, text) in texts.enumerated() {
            text.isHidden = index >= prose.count
            guard index < prose.count, let content = prose[index].prose else { continue }
            text.content = content
            text.place(prose[index].frame)
            placeHeaders(in: text, width: prose[index].frame.width, index: index)
            #if os(iOS)
            tables[index] = TableView.place(text.tableBoxes(), in: text, reusing: tables[index])
            #endif
        }
        for (index, view) in codes.enumerated() {
            view.isHidden = index >= code.count
            guard index < code.count, let content = code[index].code else { continue }
            view.show(content, surface: box.surface)
            view.frame = code[index].frame
        }
    }

    override var ownSelectables: [ProgramSelectable] {
        texts.filter { !$0.isHidden } + codes.filter { !$0.isHidden }.map(\.text)
    }

    private func placeHeaders(in text: RowTextView, width: CGFloat, index: Int) {
        _ = text.height(forWidth: width)
        let boxes = text.codeBoxes()
        while headers[index].count < boxes.count {
            let header = CodeHeader()
            text.addSubview(header)
            headers[index].append(header)
        }
        for (number, header) in headers[index].enumerated() {
            header.isHidden = number >= boxes.count
            guard number < boxes.count else { continue }
            header.show(language: boxes[number].language, code: boxes[number].code)
            header.place(CGRect(x: boxes[number].frame.minX, y: boxes[number].frame.minY, width: boxes[number].frame.width, height: CodeHeader.height))
        }
    }
}

/// Code as prose shows it: a box with the language and a copy button over the code.
final class ProgramCodeBox: FlippedView {
    private let surface = SurfaceView()
    private let header = CodeHeader()
    let text = RowTextView.make(trimsLineRoom: false)

    override init(frame: CGRect) {
        super.init(frame: frame)
        text.inProgram = true
        addSubview(surface)
        surface.addSubview(header)
        surface.addSubview(text)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    func show(_ content: CodeContent, surface lying: Surface, radius: CGFloat = Radius.lg) {
        surface.radius = radius
        surface.fill = lying.platform(.box)
        surface.stroke = lying.platformBorder
        header.show(language: content.language, code: content.code)
        text.content = ProgramCode.text(content)
        relayout()
    }

    override func layoutNow() {
        surface.frame = bounds
        header.place(CGRect(x: 0, y: 0, width: bounds.width, height: CodeHeader.height))
        let width = max(1, bounds.width - 2 * ProgramCode.inset)
        text.place(CGRect(x: ProgramCode.inset, y: CodeHeader.height, width: width, height: text.height(forWidth: width)))
    }
}

final class ProgramCodeView: ProgramNodeView {
    private let code = ProgramCodeBox()

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(code)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ box: ProgramBox) {
        guard let content = box.extra as? CodeContent, case .code(let node) = box.node.kind else { return }
        code.show(content, surface: box.surface, radius: ProgramStyle.radius(node.radius, side: min(box.size.width, box.size.height)))
    }

    override var ownSelectables: [ProgramSelectable] { [code.text] }

    override func layoutNow() {
        code.frame = bounds
    }
}

/// A popover's trigger lies inline; a press opens its panel, which the core keeps open.
final class ProgramPopoverView: ProgramNodeView {
    private var panel: ProgramPopoverPanel?
    private var lit = false

    override init(frame: CGRect) {
        super.init(frame: frame)
        focusable = true
        onPress = { [weak self] _ in
            guard let self, case .popover(let popover) = box?.node.kind else { return }
            host?.send(nodeID, ["type": "expand", "open": !popover.open])
        }
        onHover = { [weak self] point in
            self?.lit = point != nil
            self?.paint()
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func didShow(_ box: ProgramBox) {
        guard case .popover(let popover) = box.node.kind else { return }
        paint()
        guard popover.open else {
            panel?.close()
            panel = nil
            return
        }
        let panel = self.panel ?? ProgramPopoverPanel(anchor: self) { [weak self] in
            guard let self else { return }
            self.panel = nil
            host?.send(nodeID, ["type": "expand", "open": false])
        }
        self.panel = panel
        panel.show(popover.content, host: host, surface: box.surface)
    }

    private func paint() {
        guard let part = text("text")?.part, let base = box?.texts["text"] else { return }
        let colored = NSMutableAttributedString(attributedString: base.text)
        if lit { colored.addAttribute(.foregroundColor, value: Theme.mutedForeground, range: NSRange(location: 0, length: colored.length)) }
        text("text")?.part = ProgramTextPart(text: colored, frame: part.frame, lift: part.lift)
    }

    override func takesPress(at point: CGPoint) -> Bool {
        bounds.contains(point) && !ProgramControls.control(in: self, at: point)
    }
}

/// What a popover's panel shows: its slot, laid out as wide as it needs, at most 280.
enum ProgramPopoverContent {
    static let padding = (ends: CGFloat(12), sides: CGFloat(14))
    static let gap: CGFloat = 8
    static let most: CGFloat = 280

    static func layout(_ slot: UiSlot, window: CGSize, surface: Surface) -> (box: ProgramBox, text: ProgramTextPart?, size: CGSize) {
        let room = min(most, window.width - 24) - 2 * padding.sides
        let env = ProgramEnv(column: room, window: window, surface: .popover)
        var y = padding.ends
        var widest: CGFloat = 0
        var part: ProgramTextPart?
        if let rich = slot.text, !rich.text.isEmpty {
            let look = ProgramStyle.Look(font: Theme.proseFont, lineHeight: scaled(21))
            let text = ProgramText.set(rich, look: look, color: Theme.foreground)
            let width = min(room, ProgramText.width(text))
            let height = ProgramText.height(text, width: width, lineHeight: look.lineHeight)
            part = ProgramTextPart(
                text: text, frame: CGRect(x: padding.sides, y: y, width: width, height: height), lift: ControlText.lift(look.font, lineHeight: look.lineHeight))
            y += height
            widest = width
        }
        let natural = slot.children.map { ProgramLayout.natural($0, env: env) }.max() ?? 0
        let width = slot.children.isEmpty ? widest : max(widest, min(room, natural))
        let spacing = slot.children.indices.map { $0 == 0 && part == nil ? 0.0 : Double(gap) }
        let column = UiNode(json: ["id": "~panel", "component": "Col", "children": [], "align": "stretch", "spacing": []])!
        let box = ProgramLayout.column(column, slot.children, spacing: spacing, align: .stretch, width: width, fill: true, height: nil, env: env)
        y += box.size.height
        return (box, part, CGSize(width: width + 2 * padding.sides, height: y + padding.ends))
    }
}

// MARK: Accessibility

/// Tells assistive technologies what each node is, as the tree says.
enum ProgramAccess {
    static func describe(_ view: ProgramNodeView, box: ProgramBox) {
        guard let a11y = box.node.a11y else { return }
        let label = a11y.label ?? box.texts["text"]?.text.string ?? box.texts["title"]?.text.string ?? ""
        #if os(macOS)
        switch a11y.role {
        case .hidden:
            view.setAccessibilityElement(false)
        case .heading, .text:
            view.setAccessibilityElement(true)
            view.setAccessibilityRole(.staticText)
            view.setAccessibilityLabel(label)
            if a11y.role == .heading { view.setAccessibilityRoleDescription("heading, level \(a11y.level ?? 3)") }
        case .region where view is ProgramMapView:
            break
        case .group, .list, .listItem, .form, .region, .dialog, .table, .blockquote, .code:
            view.setAccessibilityElement(true)
            view.setAccessibilityRole(a11y.role == .list ? .list : a11y.role == .table ? .table : .group)
            view.setAccessibilityLabel(a11y.label)
        case .image:
            view.setAccessibilityElement(true)
            view.setAccessibilityRole(.image)
            view.setAccessibilityLabel(label)
        case .link:
            view.setAccessibilityElement(true)
            view.setAccessibilityRole(.link)
            view.setAccessibilityLabel(label)
        case .button:
            guard view is ProgramPressableView || view is ProgramPopoverView || view is ProgramFlashcardView else { return }
            view.setAccessibilityElement(true)
            view.setAccessibilityRole(.button)
            view.setAccessibilityLabel(label)
            view.setAccessibilityValue(a11y.on.map { $0 ? 1 : 0 })
        case .timer:
            view.setAccessibilityElement(true)
            view.setAccessibilityRole(.group)
            view.setAccessibilityLabel([a11y.label, a11y.value].compactMap { $0 }.joined(separator: ", "))
        case .textField, .popUpButton, .radioGroup, .adjustable, .checkbox:
            break
        }
        #else
        switch a11y.role {
        case .hidden:
            view.isAccessibilityElement = false
            view.accessibilityElementsHidden = true
        case .heading, .text, .image, .link:
            view.isAccessibilityElement = true
            view.accessibilityLabel = label
            view.accessibilityTraits = a11y.role == .heading ? .header : a11y.role == .image ? .image : a11y.role == .link ? .link : .staticText
        case .button:
            guard view is ProgramPressableView || view is ProgramPopoverView || view is ProgramFlashcardView else { return }
            view.isAccessibilityElement = true
            view.accessibilityLabel = label
            view.accessibilityTraits = a11y.on == true ? [.button, .selected] : .button
        case .timer:
            view.isAccessibilityElement = false
            view.accessibilityLabel = [a11y.label, a11y.value].compactMap { $0 }.joined(separator: ", ")
        case .group, .list, .listItem, .form, .region, .dialog, .table, .blockquote, .code, .textField, .popUpButton, .radioGroup, .adjustable,
            .checkbox:
            view.isAccessibilityElement = false
        }
        #endif
    }
}

/// A Carousel: its items side by side on a track that scrolls sideways and stops at an item's
/// edge. With a pointer, buttons at its sides page it.
final class ProgramCarouselView: ProgramNodeView {
    /// How far outside itself the track draws, so that focus rings aren't cut.
    private static let bleed: CGFloat = 4
    private static let button: CGFloat = 34
    private static let buttonInset: CGFloat = 8
    private let track = CarouselTrack()
    private let back = GlassButtonView(symbol: .chevronLeft, symbolSide: 18, help: "Previous", height: ProgramCarouselView.button) {}
    private let forward = GlassButtonView(symbol: .chevronRight, symbolSide: 18, help: "Next", height: ProgramCarouselView.button) {}

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(track)
        track.onScroll = { [weak self] in self?.showButtons() }
        if Platform.hoverReveals {
            addSubview(back)
            addSubview(forward)
        }
        back.action = { [weak self] in self?.track.page(-1) }
        forward.action = { [weak self] in self?.track.page(1) }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override var holder: PlatformView { track.content }

    override func configure(_ box: ProgramBox) {
        guard let carousel = box.extra as? ProgramCarousel else { return }
        track.frame = bounds.insetBy(dx: 0, dy: -Self.bleed)
        track.set(width: carousel.track, height: box.size.height, inset: Self.bleed, step: carousel.item + carousel.gap)
        showButtons()
    }

    private func showButtons() {
        let side = Self.button
        let y = ((bounds.height - side) / 2).rounded()
        back.frame = CGRect(x: Self.buttonInset, y: y, width: side, height: side)
        forward.frame = CGRect(x: bounds.width - Self.buttonInset - side, y: y, width: side, height: side)
        back.isHidden = track.offset <= Self.bleed
        forward.isHidden = track.offset >= track.most - Self.bleed
    }

    override func layoutNow() {
        track.frame = bounds.insetBy(dx: 0, dy: -Self.bleed)
        showButtons()
    }
}

/// A popover's panel: Motile's popover of its slot.
final class ProgramPanelView: FlippedView {
    private let face = SurfaceView()
    private let words = ProgramTextView()
    private let content = ProgramStackView()

    override init(frame: CGRect) {
        super.init(frame: frame)
        face.fill = Surface.popover.platform
        face.stroke = Surface.popover.platformBorder
        face.radius = Radius.lg
        face.drop = (.md, .shadow)
        addSubview(face)
        addSubview(words)
        addSubview(content)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    /// Shows the slot and answers how large the panel is.
    func show(_ slot: UiSlot, host: ProgramHost?, window: CGSize) -> CGSize {
        let laid = ProgramPopoverContent.layout(slot, window: window, surface: .popover)
        words.part = laid.text
        words.frame = laid.text?.frame ?? .zero
        let top = laid.text.map { $0.frame.maxY + (slot.children.isEmpty ? 0 : ProgramPopoverContent.gap) } ?? ProgramPopoverContent.padding.ends
        content.frame = CGRect(x: ProgramPopoverContent.padding.sides, y: top, width: laid.box.size.width, height: laid.box.size.height)
        content.show(laid.box, host: host)
        return laid.size
    }

    override func layoutNow() {
        face.frame = bounds
    }
}
