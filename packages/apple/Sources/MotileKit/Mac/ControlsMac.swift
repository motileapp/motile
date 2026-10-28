#if os(macOS)
import AppKit

/// `InputField` for the views the transcript draws itself: a line to type in, in a field's fill.
/// A click anywhere on the fill puts the cursor in it.
final class FieldView: FlippedView, NSTextFieldDelegate {
    let size: ControlSize
    let numeric: Bool
    var text: String {
        get { field.stringValue }
        set {
            guard newValue != field.stringValue else { return }
            field.stringValue = newValue
        }
    }
    var placeholder: String { didSet { showPlaceholder() } }
    var surface = Surface.background { didSet { frameView.fill = fieldFill(surface) } }
    var enabled = true {
        didSet {
            field.isEnabled = enabled
            opacity = enabled ? 1 : Opacity.disabled.fixed
        }
    }
    var onChange: (String) -> Void
    /// The user is typing in it.
    var editing: Bool { field.currentEditor() != nil }

    private let frameView = SurfaceView()
    private let field = NSTextField()

    /// A `numeric` field asks for the keypad of numbers where there is one.
    init(placeholder: String, size: ControlSize = .regular, numeric: Bool = false, onChange: @escaping (String) -> Void) {
        self.placeholder = placeholder
        self.size = size
        self.numeric = numeric
        self.onChange = onChange
        super.init(frame: .zero)
        frameView.radius = size.radius
        frameView.fill = fieldFill(surface)
        field.isBordered = false
        field.isBezeled = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.font = .ui(size.textSize + 0.5)
        field.textColor = Theme.foreground
        field.cell?.usesSingleLineMode = true
        field.cell?.isScrollable = true
        field.delegate = self
        addSubview(frameView)
        addSubview(field)
        showPlaceholder()
        onPress = { [weak self] _ in
            guard let self, enabled else { return }
            window?.makeFirstResponder(field)
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func takesPress(at point: CGPoint) -> Bool {
        bounds.contains(point) && !field.frame.contains(point)
    }

    override func layoutNow() {
        let height = size.height
        let top = ((bounds.height - height) / 2).rounded()
        frameView.frame = CGRect(x: 0, y: top, width: bounds.width, height: height)
        let inset = size.padding - 2
        let fieldHeight = field.intrinsicContentSize.height
        field.frame = CGRect(x: inset, y: top + ((height - fieldHeight) / 2).rounded(), width: max(0, bounds.width - 2 * inset), height: fieldHeight)
    }

    private func showPlaceholder() {
        field.placeholderAttributedString = NSAttributedString(
            string: placeholder, attributes: [.font: PlatformFont.ui(size.textSize + 0.5), .foregroundColor: Theme.mutedStrongerForeground])
        field.setAccessibilityLabel(placeholder)
    }

    func controlTextDidChange(_ notification: Notification) {
        onChange(field.stringValue)
    }
}

/// `TextArea` for the views the transcript draws itself: `rows` lines tall in a field's fill,
/// scrolling past that. Return is a line break.
final class TextAreaView: FlippedView, NSTextViewDelegate {
    let size: ControlSize
    let rows: Int
    var text: String {
        get { textView.string }
        set {
            guard newValue != textView.string else { return }
            textView.string = newValue
            placeholderView.isHidden = !newValue.isEmpty
        }
    }
    var placeholder: String { didSet { showPlaceholder() } }
    var surface = Surface.background { didSet { frameView.fill = fieldFill(surface) } }
    var enabled = true {
        didSet {
            textView.isEditable = enabled
            opacity = enabled ? 1 : Opacity.disabled.fixed
        }
    }
    var onChange: (String) -> Void
    /// The user is typing in it.
    var editing: Bool { window?.firstResponder === textView }

    private let frameView = SurfaceView()
    private let scroll = NSScrollView()
    private let textView: NSTextView
    private let placeholderView = TextBlockView()

    init(placeholder: String, rows: Int = 3, size: ControlSize = .regular, onChange: @escaping (String) -> Void) {
        self.placeholder = placeholder
        self.rows = rows
        self.size = size
        self.onChange = onChange
        let storage = NSTextStorage()
        let layout = NSLayoutManager()
        storage.addLayoutManager(layout)
        let container = NSTextContainer(size: NSSize(width: 100, height: CGFloat.greatestFiniteMagnitude))
        layout.addTextContainer(container)
        textView = NSTextView(frame: .zero, textContainer: container)
        super.init(frame: .zero)
        frameView.radius = size.radius
        frameView.fill = fieldFill(surface)
        let font = Self.font(size)
        textView.isRichText = false
        textView.allowsUndo = true
        textView.drawsBackground = false
        textView.font = font
        textView.textColor = Theme.foreground
        textView.insertionPointColor = Theme.foreground
        textView.textContainerInset = NSSize(width: 0, height: ComposerTextView.verticalInset)
        textView.textContainer?.lineFragmentPadding = 2
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        textView.textContainer?.widthTracksTextView = true
        textView.defaultParagraphStyle = ComposerTextView.paragraphStyle
        textView.typingAttributes = [.font: font, .foregroundColor: Theme.foreground, .paragraphStyle: ComposerTextView.paragraphStyle]
        textView.delegate = self
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        scroll.borderType = .noBorder
        scroll.documentView = textView
        placeholderView.oneLine = true
        addSubview(frameView)
        addSubview(scroll)
        addSubview(placeholderView)
        showPlaceholder()
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    private static func font(_ size: ControlSize) -> NSFont { .ui(size.textSize + 0.5) }

    private static func padding(_ size: ControlSize) -> CGSize {
        CGSize(width: size.padding - 2, height: max(0, size.padding - ComposerTextView.verticalInset))
    }

    static func height(rows: Int, size: ControlSize = .regular) -> CGFloat {
        ComposerTextView.height(of: rows, in: font(size)) + 2 * padding(size).height
    }

    override func layoutNow() {
        frameView.frame = bounds
        let padding = Self.padding(size)
        scroll.frame = bounds.insetBy(dx: padding.width, dy: padding.height)
        textView.frame.size.width = scroll.contentSize.width
        let line = ControlText.height(placeholderView.text, width: .greatestFiniteMagnitude)
        placeholderView.frame = CGRect(
            x: scroll.frame.minX + 2, y: scroll.frame.minY + ComposerTextView.verticalInset, width: max(0, scroll.frame.width - 4), height: line)
    }

    private func showPlaceholder() {
        placeholderView.text = ControlText.line(placeholder, font: Self.font(size), color: Theme.mutedStrongerForeground)
        placeholderView.isHidden = !text.isEmpty
        textView.setAccessibilityLabel(placeholder)
    }

    func textDidChange(_ notification: Notification) {
        placeholderView.isHidden = !textView.string.isEmpty
        onChange(textView.string)
    }
}

/// The system's calendar in a popover under a view, to pick a day between two. A click on a day
/// picks it and closes the popover; moving to another month only shows it.
enum DayPicker {
    static func present(from view: NSView, day: Day, earliest: Day?, latest: Day?, onPick: @escaping (Day) -> Void) {
        let controller = DayPickerController(day: day, earliest: earliest, latest: latest, onPick: onPick)
        let popover = NSPopover()
        popover.behavior = .transient
        popover.contentViewController = controller
        controller.popover = popover
        popover.show(relativeTo: view.bounds, of: view, preferredEdge: view.isFlipped ? .maxY : .minY)
    }
}

private final class DayPickerController: NSViewController {
    private static let padding: CGFloat = 8

    weak var popover: NSPopover?
    private let picker = NSDatePicker()
    private var shown: Day
    private let onPick: (Day) -> Void

    init(day: Day, earliest: Day?, latest: Day?, onPick: @escaping (Day) -> Void) {
        shown = day
        self.onPick = onPick
        super.init(nibName: nil, bundle: nil)
        picker.datePickerStyle = .clockAndCalendar
        picker.datePickerElements = .yearMonthDay
        picker.datePickerMode = .single
        picker.isBezeled = false
        picker.isBordered = false
        picker.drawsBackground = false
        picker.dateValue = day.date
        picker.minDate = earliest.map { Calendar.current.startOfDay(for: $0.date) }
        picker.maxDate = latest.map { Calendar.current.startOfDay(for: $0.date).addingTimeInterval(86_399) }
        picker.target = self
        picker.action = #selector(changed)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func loadView() {
        picker.sizeToFit()
        let side = Self.padding
        let container = NSView(frame: NSRect(x: 0, y: 0, width: picker.frame.width + 2 * side, height: picker.frame.height + 2 * side))
        picker.frame.origin = NSPoint(x: side, y: side)
        container.addSubview(picker)
        view = container
    }

    @objc private func changed() {
        let day = Day(picker.dateValue)
        defer { shown = day }
        guard day.year == shown.year, day.month == shown.month else { return }
        onPick(day)
        popover?.close()
    }
}

/// The glass a `GlassButtonView` lies on, a capsule as tall as its frame: Liquid Glass in its tint
/// where the system has it, and the popover's material with a hairline around it and a shadow
/// before that.
final class GlassView: NSView {
    /// Whether it answers the pointer itself, which it leaves to what lies on it here.
    static let answersTouch = false

    private let effect: NSView

    /// What lies on it.
    var content: NSView { self }

    override init(frame: NSRect) {
        if #available(macOS 26.0, *) {
            let glass = NSGlassEffectView()
            glass.tintColor = Theme.glassTint
            effect = glass
        } else {
            let blur = NSVisualEffectView()
            blur.material = .popover
            blur.blendingMode = .withinWindow
            blur.state = .active
            blur.wantsLayer = true
            effect = blur
        }
        super.init(frame: frame)
        addSubview(effect)
        guard effect is NSVisualEffectView else { return }
        effect.layer?.borderWidth = 1
        applyShadow(.sm, .shadowStronger)
        paint()
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override var isFlipped: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func layout() {
        super.layout()
        effect.frame = bounds
        let radius = bounds.height / 2
        if #available(macOS 26.0, *), let glass = effect as? NSGlassEffectView {
            glass.cornerRadius = radius
            return
        }
        effect.layer?.cornerRadius = radius
        effect.layer?.cornerCurve = .continuous
        effect.layer?.masksToBounds = true
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        guard effect is NSVisualEffectView else { return }
        applyShadow(.sm, .shadowStronger)
        paint()
    }

    private func paint() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            effect.layer?.borderColor = Theme.border.cgColor
        }
    }
}
#endif
