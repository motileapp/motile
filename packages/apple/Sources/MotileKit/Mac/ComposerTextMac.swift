#if os(macOS)
import AppKit
import SwiftUI

/// The composer's text: grows with what is typed, sends on Return, and takes dropped files. A
/// `TextArea` is the same view with Return as a line break and no focus of its own.
struct ComposerTextView: NSViewRepresentable {
    static let font = NSFont.systemFont(ofSize: 14)
    static let paragraphStyle: NSParagraphStyle = {
        let style = NSMutableParagraphStyle()
        style.lineSpacing = 3
        return style
    }()
    static let verticalInset: CGFloat = 4
    /// Two lines, so the composer only grows when a third one starts.
    static let minimumHeight = height(of: 2, in: font)
    static let maximumHeight: CGFloat = 220

    /// How tall the view is with that many lines of the font in it.
    static func height(of lines: Int, in font: NSFont) -> CGFloat {
        let text = (1...max(1, lines)).map(String.init).joined(separator: "\n")
        let storage = NSTextStorage(string: text, attributes: [.font: font, .paragraphStyle: paragraphStyle])
        let layout = NSLayoutManager()
        storage.addLayoutManager(layout)
        let container = NSTextContainer(size: NSSize(width: 100, height: CGFloat.greatestFiniteMagnitude))
        layout.addTextContainer(container)
        layout.ensureLayout(for: container)
        return ceil(layout.usedRect(for: container).height) + verticalInset * 2
    }

    @Binding var text: String
    @Binding var height: CGFloat
    let placeholder: String
    /// Changes when another thread's draft is shown, which is when the cursor should come here.
    /// Without one the view never takes the cursor by itself.
    var focusKey: String?
    /// What Return does. Without it, Return is a line break.
    var onSubmit: (() -> Void)?
    var onFiles: ([URL]) -> Void = { _ in }
    /// Files are being dragged over the text, or no longer are.
    var onFileDrag: (Bool) -> Void = { _ in }
    var font = Self.font
    var heights = Self.minimumHeight...Self.maximumHeight
    /// What the text does with skills. Without it, a `$name` is only text.
    var skills: SkillTyping?

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = ComposerScrollView()
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        scroll.borderType = .noBorder

        // TextKit 1, like the transcript, so the height can be measured directly.
        let storage = NSTextStorage()
        let layout = NSLayoutManager()
        storage.addLayoutManager(layout)
        let container = NSTextContainer(size: NSSize(width: 100, height: CGFloat.greatestFiniteMagnitude))
        layout.addTextContainer(container)
        let view = ComposerNSTextView(frame: .zero, textContainer: container)
        view.minSize = .zero
        view.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        view.delegate = context.coordinator
        view.isRichText = false
        view.allowsUndo = true
        view.drawsBackground = false
        view.font = font
        view.textColor = Theme.foreground
        view.insertionPointColor = Theme.foreground
        view.textContainerInset = NSSize(width: 0, height: Self.verticalInset)
        view.textContainer?.lineFragmentPadding = 2
        view.isVerticallyResizable = true
        view.isHorizontallyResizable = false
        view.autoresizingMask = [.width]
        view.textContainer?.widthTracksTextView = true
        view.isAutomaticQuoteSubstitutionEnabled = false
        view.isAutomaticDashSubstitutionEnabled = false
        view.isAutomaticTextReplacementEnabled = false
        view.isAutomaticSpellingCorrectionEnabled = false
        view.defaultParagraphStyle = Self.paragraphStyle
        view.typingAttributes = [
            .font: font,
            .foregroundColor: Theme.foreground,
            .paragraphStyle: Self.paragraphStyle,
        ]
        view.onSubmit = onSubmit
        view.onFiles = onFiles
        view.onFileDrag = onFileDrag
        view.placeholder = placeholder
        view.skills = skills
        view.string = text
        scroll.documentView = view
        context.coordinator.textView = view
        context.coordinator.picked = skills?.pick?.id
        context.coordinator.chipify(atEnd: true)
        DispatchQueue.main.async {
            if focusKey != nil { view.window?.makeFirstResponder(view) }
            context.coordinator.measure()
        }
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        scroll.putAway(context.environment.putAway)
        guard let view = scroll.documentView as? ComposerNSTextView else { return }
        context.coordinator.parent = self
        view.onSubmit = onSubmit
        view.onFiles = onFiles
        view.onFileDrag = onFileDrag
        view.skills = skills
        if view.placeholder != placeholder {
            view.placeholder = placeholder
            view.needsDisplay = true
        }
        if view.text != text {
            view.string = text
            context.coordinator.chipify(atEnd: true)
            context.coordinator.measure()
        }
        if let pick = skills?.pick, pick.id != context.coordinator.picked {
            context.coordinator.picked = pick.id
            context.coordinator.put(pick)
        }
        if context.coordinator.focusKey != focusKey {
            context.coordinator.focusKey = focusKey
            DispatchQueue.main.async { view.window?.makeFirstResponder(view) }
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: ComposerTextView
        weak var textView: ComposerNSTextView?
        var focusKey: String?
        /// The last skill picked from the menu that the text has put in.
        var picked: Int?

        init(_ parent: ComposerTextView) {
            self.parent = parent
            focusKey = parent.focusKey
        }

        func textDidChange(_ notification: Notification) {
            guard let view = textView else { return }
            parent.text = view.text
            measure(deferred: false)
            let cursor = view.selectedRange().location
            let before = cursor > 0 ? (view.string as NSString).character(at: cursor - 1) : 0
            if let before = UnicodeScalar(before), CharacterSet.whitespacesAndNewlines.contains(before) {
                chipify(atEnd: false)
            }
        }

        func textViewDidChangeSelection(_ notification: Notification) {
            report()
        }

        /// Tells what is typed before the cursor, for the skills that fit it.
        func report() {
            guard let view = textView, let skills = parent.skills else { return }
            let selection = view.selectedRange()
            let typing = view.window?.firstResponder === view && selection.length == 0
            skills.typing(typing ? (view.string as NSString).substring(to: selection.location) : "")
        }

        /// Makes the text's `$name`s of skills chips: those a space or a line follows, and with
        /// `atEnd` the one that ends the text too.
        func chipify(atEnd: Bool) {
            guard let view = textView, let skills = parent.skills, view.string.contains("$") else { return }
            let asked = view.string
            skills.spans(asked) { [weak self, weak view] spans in
                guard let self, let view, view.string == asked else { return }
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
            guard let typed = (view.string as NSString).skillWord(endingAt: view.selectedRange().location) else { return }
            replace(typed, with: SkillChipAttachment(name: pick.name, label: pick.label, beside: parent.font), space: true)
        }

        private func replace(_ range: NSRange, with chip: SkillChipAttachment, space: Bool) {
            guard let view = textView, let storage = view.textStorage, view.shouldChangeText(in: range, replacementString: nil) else { return }
            let chipped = NSMutableAttributedString(attachment: chip)
            if space { chipped.append(NSAttributedString(string: " ")) }
            chipped.addAttributes(view.typingAttributes, range: NSRange(location: 0, length: chipped.length))
            let selection = view.selectedRange()
            storage.replaceCharacters(in: range, with: chipped)
            let shift = chipped.length - range.length
            let cursor = selection.location >= NSMaxRange(range) ? selection.location + shift : NSMaxRange(range) + shift
            view.setSelectedRange(NSRange(location: cursor, length: 0))
            view.didChangeText()
        }

        /// Makes the composer as tall as its text, within limits. Typing grows it in the same
        /// frame as the new line; during a view update the height can only change after it.
        func measure(deferred: Bool = true) {
            guard let view = textView, let container = view.textContainer, let layout = view.layoutManager else { return }
            layout.ensureLayout(for: container)
            let used = layout.usedRect(for: container).height + view.textContainerInset.height * 2
            let height = min(parent.heights.upperBound, max(parent.heights.lowerBound, ceil(used)))
            guard abs(parent.height - height) > 0.5 else { return }
            guard deferred else {
                parent.height = height
                return
            }
            DispatchQueue.main.async { self.parent.height = height }
        }
    }
}

/// A click under the text, where the text view doesn't reach, puts the cursor at its end.
final class ComposerScrollView: NSScrollView {
    override func mouseDown(with event: NSEvent) {
        guard let view = documentView as? NSTextView else { return super.mouseDown(with: event) }
        window?.makeFirstResponder(view)
        view.setSelectedRange(NSRange(location: (view.string as NSString).length, length: 0))
    }
}

final class ComposerNSTextView: NSTextView {
    var onSubmit: (() -> Void)?
    var onFiles: (([URL]) -> Void)?
    var onFileDrag: ((Bool) -> Void)?
    var placeholder = ""
    var skills: SkillTyping?
    /// Return sends what is written: the composer's text, not a `TextArea`.
    var sends: Bool { onSubmit != nil }

    /// What is written, with each skill's chip as its `$name`.
    var text: String { textStorage?.withSkillNames ?? string }

    override func keyDown(with event: NSEvent) {
        if let skills, skills.menuOpen, !hasMarkedText(), let key = SkillMenuKey(event) {
            skills.key(key)
            return
        }
        let isReturn = event.keyCode == 36 || event.keyCode == 76
        let modifiers = event.modifierFlags.intersection([.shift, .option, .control])
        // Return sends; with Shift or Option it is a line break. While an input method is
        // composing, Return belongs to it.
        guard isReturn, modifiers.isEmpty, !hasMarkedText(), let onSubmit else {
            super.keyDown(with: event)
            return
        }
        onSubmit()
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        guard string.isEmpty, !placeholder.isEmpty else { return }
        let attributes: [NSAttributedString.Key: Any] = [.font: font ?? ComposerTextView.font, .foregroundColor: Theme.mutedStrongerForeground]
        let origin = NSPoint(x: textContainerInset.width + (textContainer?.lineFragmentPadding ?? 0), y: textContainerInset.height)
        (placeholder as NSString).draw(at: origin, withAttributes: attributes)
    }

    override func resignFirstResponder() -> Bool {
        let resigned = super.resignFirstResponder()
        if resigned { skills?.typing("") }
        return resigned
    }

    /// A skill's chip is copied as its `$name`.
    override func writeSelection(to pboard: NSPasteboard, type: NSPasteboard.PasteboardType) -> Bool {
        guard type == .string, let storage = textStorage else { return super.writeSelection(to: pboard, type: type) }
        let selected = storage.attributedSubstring(from: selectedRange())
        return pboard.setString(selected.withSkillNames, forType: .string)
    }

    override var readablePasteboardTypes: [NSPasteboard.PasteboardType] {
        super.readablePasteboardTypes + [.fileURL, .png, .tiff]
    }

    override func paste(_ sender: Any?) {
        // Files copied in Finder and copied images are attached; anything else is pasted as
        // plain text.
        let pasteboard = NSPasteboard.general
        let urls = Self.files(on: pasteboard)
        guard urls.isEmpty else {
            onFiles?(urls)
            return
        }
        guard pasteboard.string(forType: .string) == nil, let image = pasteboard.data(forType: .png) ?? pasteboard.data(forType: .tiff) else {
            pasteAsPlainText(sender)
            (delegate as? ComposerTextView.Coordinator)?.chipify(atEnd: true)
            return
        }
        let isPNG = pasteboard.data(forType: .png) != nil
        let onFiles = onFiles
        DispatchQueue.global(qos: .userInitiated).async {
            let png = isPNG ? image : NSBitmapImageRep(data: image)?.representation(using: .png, properties: [:])
            guard let png, let file = ImageFiles.saveForAttaching(png, type: .png) else { return }
            DispatchQueue.main.async { onFiles?([file]) }
        }
    }

    private static func files(on pasteboard: NSPasteboard) -> [URL] {
        pasteboard.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL] ?? []
    }

    // A file dropped on the text is attached. Left to the text view, its path would be typed.
    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        guard !Self.files(on: sender.draggingPasteboard).isEmpty else { return super.draggingEntered(sender) }
        onFileDrag?(true)
        return .copy
    }

    override func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation {
        guard !Self.files(on: sender.draggingPasteboard).isEmpty else { return super.draggingUpdated(sender) }
        return .copy
    }

    override func draggingExited(_ sender: NSDraggingInfo?) {
        onFileDrag?(false)
        super.draggingExited(sender)
    }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        let urls = Self.files(on: sender.draggingPasteboard)
        guard !urls.isEmpty else { return super.performDragOperation(sender) }
        onFileDrag?(false)
        onFiles?(urls)
        return true
    }
}

extension SkillMenuKey {
    init?(_ event: NSEvent) {
        guard event.modifierFlags.intersection([.shift, .option, .control, .command]).isEmpty else { return nil }
        switch event.keyCode {
        case 126: self = .up
        case 125: self = .down
        case 36, 76, 48: self = .pick
        case 53: self = .dismiss
        default: return nil
        }
    }
}
#endif
