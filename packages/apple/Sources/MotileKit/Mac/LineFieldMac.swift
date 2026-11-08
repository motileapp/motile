#if os(macOS)
import AppKit
import SwiftUI

/// The cell of every line to type in on the Mac. It keeps the text `inset` in from the field's
/// sides, as a bordered field's cell does. A borderless cell lets its editor reach past the
/// field's sides, and the field cuts off what is drawn there: the cursor after the last letter
/// of a line that scrolls, and the hook of a last f.
final class LineFieldCell: NSTextFieldCell {
    static let inset: CGFloat = 3
    /// How far past the right of the frame it is given AppKit shows a field's editor. The editor
    /// is given that much less, so that at the end of a line the cursor stays inside the line's
    /// frame, clear of what lies beside it.
    static let editorReach: CGFloat = 2

    /// Where the text and the field's editor lie in the field's bounds. AppKit places the editor
    /// by `edit` and `select` when typing starts and by `drawingRect` when the field is resized.
    static func textFrame(_ rect: NSRect) -> NSRect {
        var frame = rect.insetBy(dx: inset, dy: 0)
        frame.size.width = max(0, frame.width - editorReach)
        return frame
    }

    override func drawingRect(forBounds rect: NSRect) -> NSRect {
        Self.textFrame(super.drawingRect(forBounds: rect))
    }

    override func edit(withFrame rect: NSRect, in controlView: NSView, editor: NSText, delegate: Any?, event: NSEvent?) {
        super.edit(withFrame: Self.textFrame(rect), in: controlView, editor: editor, delegate: delegate, event: event)
    }

    override func select(
        withFrame rect: NSRect, in controlView: NSView, editor: NSText, delegate: Any?, start: Int, length: Int
    ) {
        super.select(withFrame: Self.textFrame(rect), in: controlView, editor: editor, delegate: delegate, start: start, length: length)
    }
}

/// A borderless line to type in with a `LineFieldCell`, which scrolls sideways when its text is
/// longer than it. A secure one shows dots for what is typed.
final class LineTextField: NSTextField {
    init(secure: Bool) {
        super.init(frame: .zero)
        let cell = secure ? LineSecureFieldCell(textCell: "") : LineFieldCell(textCell: "")
        cell.isEditable = true
        cell.isSelectable = true
        cell.isScrollable = true
        cell.usesSingleLineMode = true
        cell.lineBreakMode = .byClipping
        self.cell = cell
        isBordered = false
        isBezeled = false
        drawsBackground = false
        focusRingType = .none
        textColor = Theme.foreground
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    /// How far it stands out past the frame its text lies in, on either side: its cell's inset.
    static let outset = LineFieldCell.inset
}

/// `LineFieldCell` for a secure field, whose editor shows dots.
final class LineSecureFieldCell: NSSecureTextFieldCell {
    override func drawingRect(forBounds rect: NSRect) -> NSRect {
        LineFieldCell.textFrame(super.drawingRect(forBounds: rect))
    }

    override func edit(withFrame rect: NSRect, in controlView: NSView, editor: NSText, delegate: Any?, event: NSEvent?) {
        super.edit(withFrame: LineFieldCell.textFrame(rect), in: controlView, editor: editor, delegate: delegate, event: event)
    }

    override func select(
        withFrame rect: NSRect, in controlView: NSView, editor: NSText, delegate: Any?, start: Int, length: Int
    ) {
        super.select(
            withFrame: LineFieldCell.textFrame(rect), in: controlView, editor: editor, delegate: delegate, start: start, length: length)
    }
}

/// The SwiftUI views' line to type in on the Mac: a `LineTextField`, which stands out past its
/// frame by its cell's inset, so that its text lies where a SwiftUI field's would. It is focused
/// with `focused`, Return does what `onReturn` says, the keys `onLineKey` names do what it says
/// first, and Esc goes on to what lies behind it.
struct LineField: View {
    @Binding var text: String
    let font: NSFont
    var secure = false

    var body: some View {
        LineFieldRepresentable(text: $text, font: font, secure: secure)
            .id(secure)
            .padding(.horizontal, -LineTextField.outset)
    }
}

private struct LineFieldRepresentable: NSViewRepresentable {
    @Binding var text: String
    let font: NSFont
    let secure: Bool

    func makeNSView(context: Context) -> LineTextField {
        let field = LineTextField(secure: secure)
        field.delegate = context.coordinator
        field.setContentHuggingPriority(.defaultLow, for: .horizontal)
        field.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        return field
    }

    func updateNSView(_ field: LineTextField, context: Context) {
        context.coordinator.parent = self
        if field.font != font { field.font = font }
        if field.stringValue != text {
            // While it is typed in, its editor holds the text.
            if let editor = field.currentEditor() {
                editor.string = text
            } else {
                field.stringValue = text
            }
        }
        let enabled = context.environment.isEnabled
        field.isEnabled = enabled
        field.alphaValue = enabled ? 1 : Opacity.disabled.fixed
        context.coordinator.autocorrects = !context.environment.autocorrectionDisabled
        context.coordinator.onReturn = context.environment.lineReturn
        context.coordinator.onKey = context.environment.lineKey
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView field: LineTextField, context: Context) -> CGSize? {
        CGSize(width: proposal.width ?? field.intrinsicContentSize.width, height: field.intrinsicContentSize.height)
    }

    func makeCoordinator() -> Coordinator { Coordinator(parent: self) }

    final class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: LineFieldRepresentable
        var autocorrects = true
        var onReturn: (() -> Void)?
        var onKey: ((KeyEquivalent) -> KeyPress.Result)?

        /// The keys `onLineKey` can name, by what the editor would do with them.
        static let keys: [Selector: KeyEquivalent] = [
            #selector(NSResponder.moveUp(_:)): .upArrow,
            #selector(NSResponder.moveDown(_:)): .downArrow,
            #selector(NSResponder.moveLeft(_:)): .leftArrow,
            #selector(NSResponder.moveRight(_:)): .rightArrow,
            #selector(NSResponder.cancelOperation(_:)): .escape,
        ]

        init(parent: LineFieldRepresentable) {
            self.parent = parent
        }

        /// The window's fields share one editor, so each says whether it corrects what is typed.
        func controlTextDidBeginEditing(_ notification: Notification) {
            guard let editor = (notification.object as? NSTextField)?.currentEditor() as? NSTextView else { return }
            editor.isAutomaticSpellingCorrectionEnabled = autocorrects && NSSpellChecker.isAutomaticSpellingCorrectionEnabled
            editor.isAutomaticTextReplacementEnabled = autocorrects && NSSpellChecker.isAutomaticTextReplacementEnabled
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let field = notification.object as? NSTextField else { return }
            parent.text = field.stringValue
        }

        func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
            if let key = Self.keys[selector], onKey?(key) == .handled { return true }
            switch selector {
            case #selector(NSResponder.insertNewline(_:)):
                onReturn?()
                return true
            case #selector(NSResponder.cancelOperation(_:)):
                // Esc goes on to what lies behind the field, never to the editor, which would
                // offer to complete the word.
                _ = control.nextResponder?.tryToPerform(#selector(NSResponder.cancelOperation(_:)), with: control)
                return true
            default:
                return false
            }
        }
    }
}
#endif
