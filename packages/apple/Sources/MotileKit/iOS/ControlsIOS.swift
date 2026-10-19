#if os(iOS)
import UIKit

/// `InputField` for the views the transcript draws itself: a line to type in, in a field's fill.
/// A tap anywhere on the fill brings the keyboard.
final class FieldView: FlippedView {
    let size: ControlSize
    let numeric: Bool
    var text: String {
        get { field.text ?? "" }
        set {
            guard newValue != field.text else { return }
            field.text = newValue
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
    var editing: Bool { field.isFirstResponder }

    private let frameView = SurfaceView()
    private let field = UITextField()

    /// A `numeric` field asks for the keypad of numbers.
    init(placeholder: String, size: ControlSize = .regular, numeric: Bool = false, onChange: @escaping (String) -> Void) {
        self.placeholder = placeholder
        self.size = size
        self.numeric = numeric
        self.onChange = onChange
        super.init(frame: .zero)
        frameView.radius = size.radius
        frameView.fill = fieldFill(surface)
        field.borderStyle = .none
        field.font = .ui(size.textSize + 0.5)
        field.textColor = Theme.foreground
        field.tintColor = Theme.primary
        field.keyboardType = numeric ? .decimalPad : .default
        field.addAction(UIAction { [weak self] _ in self?.changed() }, for: .editingChanged)
        addSubview(frameView)
        addSubview(field)
        showPlaceholder()
        onPress = { [weak self] _ in
            guard let self, enabled else { return }
            field.becomeFirstResponder()
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
        field.frame = CGRect(x: inset, y: top, width: max(0, bounds.width - 2 * inset), height: height)
    }

    private func showPlaceholder() {
        field.attributedPlaceholder = NSAttributedString(
            string: placeholder, attributes: [.font: PlatformFont.ui(size.textSize + 0.5), .foregroundColor: Theme.mutedStrongerForeground])
        field.accessibilityLabel = placeholder
    }

    private func changed() {
        onChange(field.text ?? "")
    }
}

/// `TextArea` for the views the transcript draws itself: `rows` lines tall in a field's fill,
/// scrolling past that. Return is a line break.
final class TextAreaView: FlippedView, UITextViewDelegate {
    let size: ControlSize
    let rows: Int
    var text: String {
        get { textView.text }
        set {
            guard newValue != textView.text else { return }
            textView.text = newValue
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
    var editing: Bool { textView.isFirstResponder }

    private let frameView = SurfaceView()
    private let textView = UITextView()
    private let placeholderView = TextBlockView()

    init(placeholder: String, rows: Int = 3, size: ControlSize = .regular, onChange: @escaping (String) -> Void) {
        self.placeholder = placeholder
        self.rows = rows
        self.size = size
        self.onChange = onChange
        super.init(frame: .zero)
        frameView.radius = size.radius
        frameView.fill = fieldFill(surface)
        textView.font = Self.font(size)
        textView.textColor = Theme.foreground
        textView.tintColor = Theme.primary
        textView.backgroundColor = .clear
        textView.textContainerInset = UIEdgeInsets(top: ComposerTextView.verticalInset, left: 0, bottom: ComposerTextView.verticalInset, right: 0)
        textView.textContainer.lineFragmentPadding = 2
        textView.delegate = self
        placeholderView.oneLine = true
        addSubview(frameView)
        addSubview(textView)
        addSubview(placeholderView)
        showPlaceholder()
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    private static func font(_ size: ControlSize) -> UIFont { .ui(size.textSize + 0.5) }

    private static func padding(_ size: ControlSize) -> CGSize {
        CGSize(width: size.padding - 2, height: max(0, size.padding - ComposerTextView.verticalInset))
    }

    static func height(rows: Int, size: ControlSize = .regular) -> CGFloat {
        ComposerTextView.height(of: rows, in: font(size)) + 2 * padding(size).height
    }

    override func layoutNow() {
        frameView.frame = bounds
        let padding = Self.padding(size)
        textView.frame = bounds.insetBy(dx: padding.width, dy: padding.height)
        let line = ControlText.height(placeholderView.text, width: .greatestFiniteMagnitude)
        placeholderView.frame = CGRect(
            x: textView.frame.minX + 2, y: textView.frame.minY + ComposerTextView.verticalInset, width: max(0, textView.frame.width - 4),
            height: line)
    }

    private func showPlaceholder() {
        placeholderView.text = ControlText.line(placeholder, font: Self.font(size), color: Theme.mutedStrongerForeground)
        placeholderView.isHidden = !text.isEmpty
        textView.accessibilityLabel = placeholder
    }

    func textViewDidChange(_ textView: UITextView) {
        placeholderView.isHidden = !textView.text.isEmpty
        onChange(textView.text)
    }
}

extension Choice {
    /// Offers the choices as the sheet `choices(_:)` shows, from the view.
    static func offer(_ choices: [Choice], under view: UIView) {
        guard let presenter = view.presenter else { return }
        let sheet = UIAlertController(title: nil, message: nil, preferredStyle: .actionSheet)
        for choice in choices {
            sheet.addAction(UIAlertAction(title: choice.chosen ? "✓ \(choice.title)" : choice.title, style: .default) { _ in choice.run() })
        }
        sheet.addAction(UIAlertAction(title: "Cancel", style: .cancel))
        sheet.popoverPresentationController?.sourceView = view
        sheet.popoverPresentationController?.sourceRect = view.bounds
        presenter.present(sheet, animated: true)
    }
}

/// The system's calendar, inline in a popover by a view, also on a phone, to pick a day between
/// two. A tap on a day picks it and closes the popover.
enum DayPicker {
    static func present(from view: UIView, day: Day, earliest: Day?, latest: Day?, onPick: @escaping (Day) -> Void) {
        guard let presenter = view.presenter else { return }
        let controller = DayPickerController(day: day, earliest: earliest, latest: latest, onPick: onPick)
        controller.modalPresentationStyle = .popover
        controller.popoverPresentationController?.sourceView = view
        controller.popoverPresentationController?.sourceRect = view.bounds
        controller.popoverPresentationController?.permittedArrowDirections = [.up, .down]
        controller.popoverPresentationController?.delegate = controller
        presenter.present(controller, animated: true)
    }
}

private final class DayPickerController: UIViewController, UIPopoverPresentationControllerDelegate {
    private static let padding: CGFloat = 8

    private let picker = UIDatePicker()
    private let onPick: (Day) -> Void

    init(day: Day, earliest: Day?, latest: Day?, onPick: @escaping (Day) -> Void) {
        self.onPick = onPick
        super.init(nibName: nil, bundle: nil)
        picker.datePickerMode = .date
        picker.preferredDatePickerStyle = .inline
        picker.tintColor = Theme.primary
        picker.date = day.date
        picker.minimumDate = earliest.map { Calendar.current.startOfDay(for: $0.date) }
        picker.maximumDate = latest.map { Calendar.current.startOfDay(for: $0.date).addingTimeInterval(86_399) }
        picker.addAction(UIAction { [weak self] _ in self?.picked() }, for: .valueChanged)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.addSubview(picker)
        let size = picker.systemLayoutSizeFitting(UIView.layoutFittingCompressedSize)
        picker.frame = CGRect(x: Self.padding, y: Self.padding, width: size.width, height: size.height)
        preferredContentSize = CGSize(width: size.width + 2 * Self.padding, height: size.height + 2 * Self.padding)
    }

    private func picked() {
        onPick(Day(picker.date))
        dismiss(animated: true)
    }

    func adaptivePresentationStyle(for controller: UIPresentationController, traitCollection: UITraitCollection) -> UIModalPresentationStyle {
        .none
    }
}
#endif
