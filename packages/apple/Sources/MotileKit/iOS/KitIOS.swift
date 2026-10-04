#if os(iOS)
import QuartzCore
import UIKit

/// An entry of the menu a view opens when it is held.
struct MenuAction {
    let title: String
    let symbol: String
    let run: () -> Void
}

extension UIView {
    var opacity: CGFloat {
        get { alpha }
        set { alpha = newValue }
    }

    func redraw() { setNeedsDisplay() }

    /// What the Mac says about a view under the pointer. Nothing is said here.
    var tip: String? {
        get { nil }
        set {}
    }

    func describe(_ label: String, button: Bool = false) {
        isAccessibilityElement = true
        accessibilityLabel = label
        if button { accessibilityTraits = .button }
    }

    func fade(to opacity: CGFloat, duration: TimeInterval) {
        UIView.animate(withDuration: duration, delay: 0, options: [.allowUserInteraction, .curveEaseOut]) { self.alpha = opacity }
    }

    func dropShadow(opacity: CGFloat, radius: CGFloat, down: CGFloat) {
        layer.shadowColor = UIColor.black.cgColor
        layer.shadowOpacity = Float(opacity)
        layer.shadowRadius = radius
        layer.shadowOffset = CGSize(width: 0, height: down)
    }

    func ticker(target: Any, selector: Selector) -> CADisplayLink {
        CADisplayLink(target: target, selector: selector)
    }

    /// The view controller the view is shown by.
    var presenter: UIViewController? {
        var responder: UIResponder? = self
        while let current = responder {
            if let controller = current as? UIViewController { return controller }
            responder = current.next
        }
        return window?.rootViewController
    }
}

/// A view that can take taps, say where a finger or the pointer is on it and open a menu. The
/// Mac's twin has its origin at the top left as every view has here, which is what it is named for.
class FlippedView: UIView, UIContextMenuInteractionDelegate {
    enum Pointer {
        case arrow, hand
    }

    /// Called with where the view was tapped. A view with one takes the taps on what is inside it.
    var onPress: ((CGPoint) -> Void)?
    /// Called with where a finger or the pointer is on the view, and with nothing when it leaves.
    var onHover: ((CGPoint?) -> Void)? {
        didSet {
            guard onHover != nil, hover == nil else { return }
            let recognizer = UIHoverGestureRecognizer(target: self, action: #selector(hovered(_:)))
            addGestureRecognizer(recognizer)
            hover = recognizer
        }
    }
    var menuActions: (() -> [MenuAction])? {
        didSet {
            guard menuActions != nil, !hasMenu else { return }
            hasMenu = true
            addInteraction(UIContextMenuInteraction(delegate: self))
        }
    }
    var pointer = Pointer.arrow
    private var hover: UIHoverGestureRecognizer?
    private var hasMenu = false
    private var pressing = false

    override init(frame: CGRect) {
        super.init(frame: frame)
        isOpaque = false
        contentMode = .redraw
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    /// Places what is inside the view.
    func layoutNow() {}

    override func layoutSubviews() {
        super.layoutSubviews()
        layoutNow()
    }

    /// Whether a tap at the point is the view's.
    func takesPress(at point: CGPoint) -> Bool {
        bounds.contains(point)
    }

    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        guard onPress != nil, !isHidden, alpha > 0.01, isUserInteractionEnabled, takesPress(at: point) else {
            return super.hitTest(point, with: event)
        }
        return self
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard onPress != nil, let point = touches.first?.location(in: self), takesPress(at: point) else {
            return super.touchesBegan(touches, with: event)
        }
        pressing = true
        onHover?(point)
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard pressing, let point = touches.first?.location(in: self) else { return super.touchesMoved(touches, with: event) }
        onHover?(takesPress(at: point) ? point : nil)
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard pressing, let point = touches.first?.location(in: self) else { return super.touchesEnded(touches, with: event) }
        pressing = false
        onHover?(nil)
        guard takesPress(at: point) else { return }
        onPress?(point)
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard pressing else { return super.touchesCancelled(touches, with: event) }
        pressing = false
        onHover?(nil)
    }

    @objc private func hovered(_ recognizer: UIHoverGestureRecognizer) {
        switch recognizer.state {
        case .began, .changed: onHover?(recognizer.location(in: self))
        default: onHover?(nil)
        }
    }

    func contextMenuInteraction(_ interaction: UIContextMenuInteraction, configurationForMenuAtLocation location: CGPoint) -> UIContextMenuConfiguration? {
        guard takesPress(at: location), let actions = menuActions?(), !actions.isEmpty else { return nil }
        return UIContextMenuConfiguration(actionProvider: { _ in
            UIMenu(children: actions.map { action in
                UIAction(title: action.title, image: UIImage(systemName: action.symbol)) { _ in action.run() }
            })
        })
    }
}

/// A view that shows its layer, which `paint` sets up in the colours of the appearance.
class LayerView: FlippedView {
    override init(frame: CGRect) {
        super.init(frame: frame)
        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (view: Self, _: UITraitCollection) in view.repaint() }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        guard window != nil else { return }
        repaint()
    }

    func paint(_ layer: CALayer) {}

    func repaint() { paint(layer) }

    func resolved(_ color: UIColor) -> CGColor { color.resolvedColor(with: traitCollection).cgColor }
}

/// One line of text.
class TextLabel: UILabel {
    convenience init(font: UIFont, color: UIColor) {
        self.init(frame: .zero)
        self.font = font
        textColor = color
        lineBreakMode = .byTruncatingTail
        numberOfLines = 1
    }

    var string: String {
        get { text ?? "" }
        set { text = newValue }
    }

    var attributed: NSAttributedString {
        get { attributedText ?? NSAttributedString() }
        set { attributedText = newValue }
    }

    var color: UIColor? {
        get { textColor }
        set { textColor = newValue }
    }

    var breaks: NSLineBreakMode {
        get { lineBreakMode }
        set { lineBreakMode = newValue }
    }

    var centered: Bool {
        get { textAlignment == .center }
        set { textAlignment = newValue ? .center : .natural }
    }

    var naturalWidth: CGFloat { intrinsicContentSize.width }
}

/// A bright copy of a label, laid over it and seen only through a soft band that sweeps across.
final class ShimmerLabel: TextLabel {
    private static let bandWidth: CGFloat = 72
    private static let period: CFTimeInterval = 2.2

    private let band = CAGradientLayer()

    var sweeps = false {
        didSet { restart() }
    }

    override var frame: CGRect {
        didSet {
            guard frame.size != oldValue.size else { return }
            restart()
        }
    }

    override var text: String? {
        didSet {
            guard text != oldValue else { return }
            restart()
        }
    }

    override var attributedText: NSAttributedString? {
        didSet { restart() }
    }

    override var isHidden: Bool {
        didSet { restart() }
    }

    static func make(_ font: UIFont) -> ShimmerLabel {
        let label = ShimmerLabel(font: font, color: Theme.text)
        label.isAccessibilityElement = false
        let alphas: [CGFloat] = [0, 0.12, 0.55, 1, 0.55, 0.12, 0]
        label.band.colors = alphas.map { UIColor.black.withAlphaComponent($0).cgColor }
        label.band.locations = [0, 0.15, 0.35, 0.5, 0.65, 0.85, 1]
        label.band.startPoint = CGPoint(x: 0, y: 0.5)
        label.band.endPoint = CGPoint(x: 1, y: 0.5)
        label.layer.mask = label.band
        return label
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        restart()
    }

    private func restart() {
        band.removeAllAnimations()
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        band.frame = CGRect(x: -Self.bandWidth, y: 0, width: Self.bandWidth, height: bounds.height)
        CATransaction.commit()
        guard sweeps, window != nil, !isHidden, !Platform.reducesMotion else { return }

        let sweep = CABasicAnimation(keyPath: "position.x")
        sweep.fromValue = -Self.bandWidth / 2
        // Across the words, however much room the label has.
        sweep.toValue = min(bounds.width, intrinsicContentSize.width) + Self.bandWidth / 2
        sweep.duration = Self.period
        sweep.repeatCount = .infinity
        sweep.isRemovedOnCompletion = false
        // Every sweep starts on the same beat, so labels move together and a restart doesn't show.
        let now = band.convertTime(CACurrentMediaTime(), from: nil)
        sweep.beginTime = now - now.truncatingRemainder(dividingBy: Self.period)
        band.add(sweep, forKey: "sweep")
    }
}

/// The rows ask for the same few symbols every time one scrolls in, so each is made once.
private var symbols: [String: UIImage] = [:]

private func symbol(_ name: String, size: CGFloat, weight: UIFont.Weight) -> UIImage? {
    let key = "\(name)/\(size)/\(weight.rawValue)"
    if let made = symbols[key] { return made }
    let made = UIImage.symbol(name, size: size * Platform.scale, weight: weight)
    symbols[key] = made
    return made
}

/// An SF Symbol in one colour, in the middle of its frame.
final class SymbolView: UIImageView {
    convenience init(_ name: String = "", size: CGFloat = 12, weight: UIFont.Weight = .regular, tint: UIColor = Theme.secondary) {
        self.init(frame: .zero)
        contentMode = .center
        tintColor = tint
        if !name.isEmpty { show(name, size: size, weight: weight) }
    }

    func show(_ name: String, size: CGFloat = 12, weight: UIFont.Weight = .regular) {
        image = symbol(name, size: size, weight: weight)
    }

    var tint: UIColor? {
        get { tintColor }
        set { tintColor = newValue }
    }
}

/// A borderless button with an SF Symbol and, optionally, a title. It lights up under a finger.
final class IconButton: UIButton {
    static let side: CGFloat = 36
    private static let symbolSize: CGFloat = 14

    convenience init(symbolName: String, title: String = "", tooltip: String, action: @escaping () -> Void) {
        self.init(type: .custom)
        layer.cornerRadius = 8
        layer.cornerCurve = .continuous
        tintColor = Theme.secondary
        setTitleColor(Theme.secondary, for: .normal)
        titleLabel?.font = Theme.smallFont
        accessibilityLabel = tooltip
        set(symbolName: symbolName, title: title)
        addAction(UIAction { _ in action() }, for: .touchUpInside)
    }

    func set(symbolName: String, title: String = "") {
        setImage(symbol(symbolName, size: Self.symbolSize, weight: .medium), for: .normal)
        setTitle(title.isEmpty ? nil : title, for: .normal)
    }

    override var isHighlighted: Bool {
        didSet { backgroundColor = isHighlighted ? Theme.hover : .clear }
    }
}

/// A read-only text view that the transcript sizes itself. It never scrolls.
final class RowTextView: UITextView, UITextViewDelegate {
    /// Called when the user starts selecting here, so other rows can let go of their selection.
    var onSelect: (() -> Void)?
    private var system: TextSystem!

    static func make(wraps: Bool = true) -> RowTextView {
        let system = TextSystem()
        let view = RowTextView(frame: .zero, textContainer: system.container)
        view.system = system
        view.isEditable = false
        view.isSelectable = true
        view.isScrollEnabled = false
        view.backgroundColor = .clear
        view.textContainerInset = .zero
        view.contentInsetAdjustmentBehavior = .never
        view.dataDetectorTypes = []
        view.linkTextAttributes = [.foregroundColor: Theme.link]
        view.textDragInteraction?.isEnabled = false
        view.delegate = view
        if !wraps { system.unwrap() }
        return view
    }

    var content: NSAttributedString {
        get { system.storage }
        set { system.update(to: newValue) }
    }

    /// The room beside the text, inside the view.
    var sideInset: CGFloat {
        get { textContainerInset.left }
        set { textContainerInset = UIEdgeInsets(top: 0, left: newValue, bottom: 0, right: newValue) }
    }

    func height(forWidth width: CGFloat) -> CGFloat {
        system.height(forWidth: width)
    }

    /// Fades in the text under `oldHeight`, which was just added, and leaves the rest as it is.
    func fadeIn(below oldHeight: CGFloat) {
        GrowthFade.run(on: layer, size: bounds.size, below: oldHeight)
    }

    func codeBoxes() -> [(frame: CGRect, language: String, code: String)] {
        system.codeBoxes(origin: CGPoint(x: textContainerInset.left, y: textContainerInset.top))
    }

    /// Where each table goes, over the line that keeps its place.
    func tableBoxes() -> [(frame: CGRect, table: TableContent)] {
        var boxes: [(frame: CGRect, table: TableContent)] = []
        let storage = system.storage
        storage.enumerateAttribute(.motileTable, in: NSRange(location: 0, length: storage.length)) { value, range, _ in
            guard let table = value as? TableContent else { return }
            let glyph = system.layout.glyphIndexForCharacter(at: range.location)
            let line = system.layout.lineFragmentRect(forGlyphAt: glyph, effectiveRange: nil)
            boxes.append((CGRect(x: 0, y: line.minY, width: bounds.width, height: table.size.height), table))
        }
        return boxes
    }

    func clearSelection() {
        guard selectedRange.length > 0 else { return }
        selectedTextRange = nil
    }

    func textViewDidChangeSelection(_ textView: UITextView) {
        guard selectedRange.length > 0 else { return }
        onSelect?()
    }

    override func copy(_ sender: Any?) {
        guard selectedRange.length > 0 else { return }
        let selected = system.storage.attributedSubstring(from: selectedRange)
        UIPasteboard.general.string = TextSystem.withLineBreaks(Typesetter.words(of: selected))
    }
}

/// Shows code that is wider than the column and scrolls it sideways.
final class SidewaysClipView: UIScrollView {
    override init(frame: CGRect) {
        super.init(frame: frame)
        showsVerticalScrollIndicator = false
        alwaysBounceVertical = false
        isDirectionalLockEnabled = true
        contentInsetAdjustmentBehavior = .never
        clipsToBounds = true
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    func setContent(_ view: UIView, size: CGSize) {
        if view.superview !== self { addSubview(view) }
        view.frame = CGRect(origin: .zero, size: size)
        // Only as tall as it is shown, so that it never scrolls up or down.
        contentSize = CGSize(width: size.width, height: 1)
    }
}

/// The scroll view the transcript's rows are in, as the transcript drives it.
final class TranscriptScroller: UIView, UIScrollViewDelegate, UIGestureRecognizerDelegate {
    let document = FlippedView()
    /// The viewport moved, by the user's hand or not.
    var onScroll: (() -> Void)?
    /// The user's fingers, or the momentum they gave it, started or stopped moving the viewport.
    var onUserScroll: ((Bool) -> Void)?

    private let scrollView = UIScrollView()

    override init(frame: CGRect) {
        super.init(frame: frame)
        scrollView.contentInsetAdjustmentBehavior = .never
        scrollView.alwaysBounceVertical = true
        scrollView.showsHorizontalScrollIndicator = false
        scrollView.keyboardDismissMode = .interactive
        scrollView.delegate = self
        addSubview(scrollView)
        scrollView.addSubview(document)
        // A tap on the transcript puts the keyboard away at once, whatever else the tap does.
        let tap = UITapGestureRecognizer(target: self, action: #selector(tapped))
        tap.cancelsTouchesInView = false
        tap.delegate = self
        scrollView.addGestureRecognizer(tap)
    }

    @objc private func tapped() {
        Platform.endEditing()
    }

    func gestureRecognizer(_ recognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
        true
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func layoutSubviews() {
        super.layoutSubviews()
        guard scrollView.frame != bounds else { return }
        scrollView.frame = bounds
    }

    /// Fades the rows out at the viewport's edges. The scroll view's own layer moves with what
    /// it scrolls, so the mask is on the view around it.
    var fadeMask: CALayer? {
        get { layer.mask }
        set { layer.mask = newValue }
    }

    var offsetY: CGFloat { scrollView.contentOffset.y }
    var viewportHeight: CGFloat { scrollView.bounds.height }
    var documentSize: CGSize { scrollView.contentSize }

    /// Keeps the scroll indicator clear of what covers the transcript's ends.
    func setIndicatorInsets(top: CGFloat, bottom: CGFloat) {
        scrollView.verticalScrollIndicatorInsets = UIEdgeInsets(top: top, left: 0, bottom: bottom, right: 0)
    }

    func setDocument(width: CGFloat, height: CGFloat) {
        guard scrollView.contentSize.height != height || scrollView.contentSize.width != width else { return }
        scrollView.contentSize = CGSize(width: width, height: height)
        document.frame = CGRect(x: 0, y: 0, width: width, height: height)
    }

    func scroll(to y: CGFloat) {
        scrollView.contentOffset = CGPoint(x: 0, y: y)
    }

    func scrollViewDidScroll(_ scrollView: UIScrollView) { onScroll?() }

    func scrollViewWillBeginDragging(_ scrollView: UIScrollView) { onUserScroll?(true) }

    func scrollViewDidEndDragging(_ scrollView: UIScrollView, willDecelerate decelerate: Bool) {
        guard !decelerate else { return }
        onUserScroll?(false)
    }

    func scrollViewDidEndDecelerating(_ scrollView: UIScrollView) { onUserScroll?(false) }
}

/// What is done with the file of an image or a video outside the app.
enum MediaFiles {
    static let saveTitle = "Share…"

    static func copyImage(at file: URL) {
        DispatchQueue.global(qos: .userInitiated).async {
            guard let image = UIImage(contentsOfFile: file.path) else { return }
            DispatchQueue.main.async { UIPasteboard.general.image = image }
        }
    }

    /// Opens the share sheet on a copy of the file that has its name, which is where it can be
    /// saved to Photos or Files.
    static func save(_ file: URL, named name: String, from view: UIView) {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("motile-shared", isDirectory: true)
        let copy = folder.appendingPathComponent(name.isEmpty ? file.lastPathComponent : name)
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try? FileManager.default.removeItem(at: copy)
        let shared = (try? FileManager.default.copyItem(at: file, to: copy)) == nil ? file : copy
        let sheet = UIActivityViewController(activityItems: [shared], applicationActivities: nil)
        sheet.popoverPresentationController?.sourceView = view
        sheet.popoverPresentationController?.sourceRect = view.bounds
        view.presenter?.present(sheet, animated: true)
    }
}
#endif
