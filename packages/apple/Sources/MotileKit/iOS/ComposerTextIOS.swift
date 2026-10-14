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

    @Binding var text: String
    @Binding var height: CGFloat
    let placeholder: String
    /// Changes when another thread's draft is shown. The keyboard only comes when the text is tapped.
    var focusKey: String?
    /// What Return does from a keyboard with keys. Without it, Return is a line break.
    var onSubmit: (() -> Void)?
    var onFiles: ([URL]) -> Void = { _ in }
    var onFileDrag: (Bool) -> Void = { _ in }
    /// What the text does with skills. Without it, a `$name` is only text.
    var skills: SkillTyping?
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
        view.textColor = Theme.foreground
        view.tintColor = Theme.primary
        view.backgroundColor = .clear
        view.textContainerInset = UIEdgeInsets(top: Self.verticalInset, left: 0, bottom: Self.verticalInset, right: 0)
        view.textContainer.lineFragmentPadding = 2
        view.showsVerticalScrollIndicator = false
        view.onSubmit = onSubmit
        view.onFiles = onFiles
        view.skills = skills
        view.placeholder = placeholder
        view.text = text
        context.coordinator.textView = view
        context.coordinator.picked = skills?.pick?.id
        context.coordinator.chipify(atEnd: true)
        DispatchQueue.main.async { context.coordinator.measure() }
        return view
    }

    func updateUIView(_ view: ComposerUITextView, context: Context) {
        context.coordinator.parent = self
        view.onSubmit = onSubmit
        view.onFiles = onFiles
        view.skills = skills
        view.placeholder = placeholder
        if context.coordinator.pressed != pressed {
            context.coordinator.pressed = pressed
            DispatchQueue.main.async { view.becomeFirstResponder() }
        }
        if let pick = skills?.pick, pick.id != context.coordinator.picked {
            context.coordinator.picked = pick.id
            context.coordinator.put(pick)
        }
        guard view.written != text else { return }
        view.text = text
        context.coordinator.chipify(atEnd: true)
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
        /// The last skill picked from the menu that the text has put in.
        var picked: Int?

        init(_ parent: ComposerTextView) {
            self.parent = parent
            pressed = parent.pressed
        }

        func textViewDidBeginEditing(_ textView: UITextView) {
            parent.focused?.wrappedValue = true
        }

        func textViewDidEndEditing(_ textView: UITextView) {
            parent.focused?.wrappedValue = false
            parent.skills?.typing("")
        }

        func textViewDidChange(_ textView: UITextView) {
            guard let view = self.textView else { return }
            parent.text = view.written
            view.showPlaceholder()
            measure(deferred: false)
            let cursor = view.selectedRange.location
            let before = cursor > 0 ? (view.text as NSString).character(at: cursor - 1) : 0
            if let before = UnicodeScalar(before), CharacterSet.whitespacesAndNewlines.contains(before) {
                chipify(atEnd: false)
            }
        }

        func textViewDidChangeSelection(_ textView: UITextView) {
            guard let skills = parent.skills else { return }
            let selection = textView.selectedRange
            let typing = textView.isFirstResponder && selection.length == 0
            skills.typing(typing ? (textView.text as NSString).substring(to: selection.location) : "")
        }

        /// Makes the text's `$name`s of skills chips: those a space or a line follows, and with
        /// `atEnd` the one that ends the text too.
        func chipify(atEnd: Bool) {
            guard let view = textView, let skills = parent.skills, view.text.contains("$") else { return }
            let asked = view.text ?? ""
            skills.spans(asked) { [weak self, weak view] spans in
                guard let self, let view, view.text == asked else { return }
                let text = asked as NSString
                let ended = spans.filter { span in
                    let end = NSMaxRange(span.range)
                    guard end < text.length else { return atEnd }
                    return UnicodeScalar(text.character(at: end)).map(CharacterSet.whitespacesAndNewlines.contains) ?? false
                }
                for span in ended.reversed() {
                    self.replace(span.range, with: SkillChipAttachment(name: span.name, label: span.label, beside: self.parent.font), space: false)
                }
            }
        }

        /// Puts the picked skill in place of the word typed before the cursor.
        func put(_ pick: SkillPick) {
            guard let view = textView else { return }
            guard let typed = (view.text as NSString).skillWord(endingAt: view.selectedRange.location) else { return }
            replace(typed, with: SkillChipAttachment(name: pick.name, label: pick.label, beside: parent.font), space: true)
        }

        private func replace(_ range: NSRange, with chip: SkillChipAttachment, space: Bool) {
            guard let view = textView else { return }
            let chipped = NSMutableAttributedString(attachment: chip)
            if space { chipped.append(NSAttributedString(string: " ")) }
            chipped.addAttributes(view.typingAttributes, range: NSRange(location: 0, length: chipped.length))
            let selection = view.selectedRange
            view.textStorage.replaceCharacters(in: range, with: chipped)
            let shift = chipped.length - range.length
            let cursor = selection.location >= NSMaxRange(range) ? selection.location + shift : NSMaxRange(range) + shift
            view.selectedRange = NSRange(location: cursor, length: 0)
            textViewDidChange(view)
        }

        /// Makes the composer as tall as its text, within limits. Typing grows it in the same
        /// frame as the new line, or the line is clipped until it does; during a view update the
        /// height can only change after it.
        func measure(deferred: Bool = true) {
            guard let view = textView, view.bounds.width > 0 else { return }
            let fitted = view.sizeThatFits(CGSize(width: view.bounds.width, height: .greatestFiniteMagnitude)).height
            let height = min(parent.heights.upperBound, max(parent.heights.lowerBound, ceil(fitted)))
            view.isScrollEnabled = fitted > parent.heights.upperBound
            guard abs(parent.height - height) > 0.5 else { return }
            guard deferred else {
                parent.height = height
                return
            }
            DispatchQueue.main.async { self.parent.height = height }
        }
    }
}

final class ComposerUITextView: UITextView {
    var onSubmit: (() -> Void)?
    var onFiles: (([URL]) -> Void)?
    var skills: SkillTyping?

    /// What is written, with each skill's chip as its `$name`.
    var written: String { attributedText.withSkillNames }
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
        placeholderLabel.textColor = Theme.mutedStrongerForeground
        placeholderLabel.numberOfLines = 1
        placeholderLabel.lineBreakMode = .byTruncatingTail
        addSubview(placeholderLabel)
        isScrollEnabled = false
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    func showPlaceholder() {
        placeholderLabel.isHidden = !text.isEmpty
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

    // Return sends from a keyboard with keys; with Shift or Option it is a line break. While the
    // skills' menu is open, its keys are the menu's.
    override var keyCommands: [UIKeyCommand]? {
        guard onSubmit != nil else { return nil }
        let send = UIKeyCommand(input: "\r", modifierFlags: [], action: #selector(submit))
        let menu = [
            UIKeyCommand(input: UIKeyCommand.inputUpArrow, modifierFlags: [], action: #selector(skillUp)),
            UIKeyCommand(input: UIKeyCommand.inputDownArrow, modifierFlags: [], action: #selector(skillDown)),
            UIKeyCommand(input: "\t", modifierFlags: [], action: #selector(submit)),
            UIKeyCommand(input: UIKeyCommand.inputEscape, modifierFlags: [], action: #selector(skillDismiss)),
        ]
        let commands = [send] + (skills?.menuOpen == true ? menu : [])
        for command in commands { command.wantsPriorityOverSystemBehavior = true }
        return commands
    }

    @objc private func submit() {
        guard markedTextRange == nil else { return }
        guard let skills, skills.menuOpen else { return onSubmit?() ?? () }
        skills.key(.pick)
    }

    @objc private func skillUp() { skills?.key(.up) }
    @objc private func skillDown() { skills?.key(.down) }
    @objc private func skillDismiss() { skills?.key(.dismiss) }

    /// A skill's chip is copied as its `$name`.
    override func copy(_ sender: Any?) {
        UIPasteboard.general.string = attributedText.attributedSubstring(from: selectedRange).withSkillNames
    }

    override func cut(_ sender: Any?) {
        guard let range = selectedTextRange else { return }
        copy(sender)
        replace(range, withText: "")
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
            (delegate as? ComposerTextView.Coordinator)?.chipify(atEnd: true)
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
