#if os(iOS)
import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// The composer's text: grows with what is typed and takes pasted images and files. Return is a
/// line break on the screen's keyboard and sends from a keyboard with keys. A `TextArea` is the
/// same view with Return always a line break.
struct ComposerTextView: UIViewRepresentable {
    static let font = UIFont.systemFont(ofSize: 17)
    static let verticalInset: CGFloat = 8
    /// One line: a phone has no room to spare, and the composer grows with what is typed.
    static let minimumHeight = height(of: 1, in: font)
    static let maximumHeight: CGFloat = 180

    /// How tall the view is with that many lines of the font in it.
    static func height(of lines: Int, in font: UIFont) -> CGFloat {
        ceil(font.lineHeight) * CGFloat(max(1, lines)) + verticalInset * 2
    }

    @Environment(AppStore.self) private var store
    @Binding var text: String
    @Binding var height: CGFloat
    let placeholder: String
    /// Changes when another thread's draft is shown. The keyboard only comes when the text is tapped.
    var focusKey: String?
    /// What Return does from a keyboard with keys. Without it, Return is a line break.
    var onSubmit: (() -> Void)?
    var onFiles: ([URL]) -> Void = { _ in }
    var onFileDrag: (Bool) -> Void = { _ in }
    var font = Self.font
    var heights = Self.minimumHeight...Self.maximumHeight
    var focused: Binding<Bool>?
    var pressed = 0

    /// Says whether the text has the keyboard.
    func reporting(focus: Binding<Bool>) -> ComposerTextView {
        var view = self
        view.focused = focus
        return view
    }

    /// Brings the keyboard when the number changes.
    func focusing(on pressed: Int) -> ComposerTextView {
        var view = self
        view.pressed = pressed
        return view
    }

    func makeUIView(context: Context) -> ComposerUITextView {
        let view = ComposerUITextView()
        view.delegate = context.coordinator
        view.font = font
        view.textColor = Theme.text
        view.tintColor = Theme.primary
        view.backgroundColor = .clear
        view.textContainerInset = UIEdgeInsets(top: Self.verticalInset, left: 0, bottom: Self.verticalInset, right: 0)
        view.textContainer.lineFragmentPadding = 2
        view.showsVerticalScrollIndicator = false
        view.onSubmit = onSubmit
        view.onFiles = onFiles
        view.placeholder = placeholder
        view.text = text
        context.coordinator.textView = view
        // The view that sends is the composer's: a sent message sets out from its text.
        if onSubmit != nil { store.composerTextStart = { [weak view] in view?.textStart } }
        DispatchQueue.main.async { context.coordinator.measure() }
        return view
    }

    func updateUIView(_ view: ComposerUITextView, context: Context) {
        context.coordinator.parent = self
        view.onSubmit = onSubmit
        view.onFiles = onFiles
        view.placeholder = placeholder
        if context.coordinator.pressed != pressed {
            context.coordinator.pressed = pressed
            DispatchQueue.main.async { view.becomeFirstResponder() }
        }
        guard view.text != text else { return }
        view.text = text
        view.showPlaceholder()
        context.coordinator.measure()
    }

    /// Takes the width it is given: a text view that doesn't scroll asks for its text on one line.
    func sizeThatFits(_ proposal: ProposedViewSize, uiView: ComposerUITextView, context: Context) -> CGSize? {
        guard let width = proposal.width else { return nil }
        return CGSize(width: width, height: proposal.height ?? height)
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    final class Coordinator: NSObject, UITextViewDelegate {
        var parent: ComposerTextView
        weak var textView: ComposerUITextView?
        var pressed: Int

        init(_ parent: ComposerTextView) {
            self.parent = parent
            pressed = parent.pressed
        }

        func textViewDidBeginEditing(_ textView: UITextView) {
            parent.focused?.wrappedValue = true
        }

        func textViewDidEndEditing(_ textView: UITextView) {
            parent.focused?.wrappedValue = false
        }

        func textViewDidChange(_ textView: UITextView) {
            parent.text = textView.text
            self.textView?.showPlaceholder()
            measure()
        }

        /// Makes the composer as tall as its text, within limits.
        func measure() {
            guard let view = textView, view.bounds.width > 0 else { return }
            let fitted = view.sizeThatFits(CGSize(width: view.bounds.width, height: .greatestFiniteMagnitude)).height
            let height = min(parent.heights.upperBound, max(parent.heights.lowerBound, ceil(fitted)))
            view.isScrollEnabled = fitted > parent.heights.upperBound
            guard abs(parent.height - height) > 0.5 else { return }
            DispatchQueue.main.async { self.parent.height = height }
        }
    }
}

final class ComposerUITextView: UITextView {
    var onSubmit: (() -> Void)?
    var onFiles: (([URL]) -> Void)?
    var placeholder = "" {
        didSet { placeholderLabel.text = placeholder }
    }
    private let placeholderLabel = UILabel()
    private var laidOutWidth: CGFloat = 0

    override var font: UIFont? {
        didSet { placeholderLabel.font = font }
    }

    init() {
        super.init(frame: .zero, textContainer: nil)
        placeholderLabel.font = ComposerTextView.font
        placeholderLabel.textColor = Theme.tertiary
        placeholderLabel.numberOfLines = 1
        placeholderLabel.lineBreakMode = .byTruncatingTail
        addSubview(placeholderLabel)
        isScrollEnabled = false
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    func showPlaceholder() {
        placeholderLabel.isHidden = !text.isEmpty
    }

    /// The top left corner of the text's first line, in the window's coordinates.
    var textStart: CGPoint? {
        guard !text.isEmpty else { return nil }
        let caret = caretRect(for: beginningOfDocument)
        return convert(CGPoint(x: caret.minX, y: caret.minY), to: nil)
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        let inset = textContainerInset
        let padding = textContainer.lineFragmentPadding
        placeholderLabel.frame = CGRect(x: padding, y: inset.top, width: max(0, bounds.width - 2 * padding), height: ceil((font ?? ComposerTextView.font).lineHeight))
        showPlaceholder()
        guard bounds.width != laidOutWidth else { return }
        laidOutWidth = bounds.width
        (delegate as? ComposerTextView.Coordinator)?.measure()
    }

    // Return sends from a keyboard with keys; with Shift or Option it is a line break.
    override var keyCommands: [UIKeyCommand]? {
        guard onSubmit != nil else { return nil }
        let send = UIKeyCommand(input: "\r", modifierFlags: [], action: #selector(submit))
        send.wantsPriorityOverSystemBehavior = true
        return [send]
    }

    @objc private func submit() {
        guard markedTextRange == nil else { return }
        onSubmit?()
    }

    override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        if action == #selector(paste(_:)), UIPasteboard.general.hasImages || UIPasteboard.general.hasURLs { return true }
        return super.canPerformAction(action, withSender: sender)
    }

    /// A copied image is attached; anything else is pasted as plain text.
    override func paste(_ sender: Any?) {
        let pasteboard = UIPasteboard.general
        guard !pasteboard.hasStrings, pasteboard.hasImages, let image = pasteboard.image else {
            guard let text = pasteboard.string else { return }
            insertText(text)
            return
        }
        let onFiles = onFiles
        DispatchQueue.global(qos: .userInitiated).async {
            guard let png = image.pngData(), let file = ImageFiles.saveForAttaching(png, type: .png) else { return }
            DispatchQueue.main.async { onFiles?([file]) }
        }
    }
}
#endif
