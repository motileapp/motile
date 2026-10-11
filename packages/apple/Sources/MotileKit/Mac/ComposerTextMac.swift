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
        view.string = text
        scroll.documentView = view
        context.coordinator.textView = view
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
        if view.placeholder != placeholder {
            view.placeholder = placeholder
            view.needsDisplay = true
        }
        if view.string != text {
            view.string = text
            context.coordinator.measure()
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

        init(_ parent: ComposerTextView) {
            self.parent = parent
            focusKey = parent.focusKey
        }

        func textDidChange(_ notification: Notification) {
            guard let view = textView else { return }
            parent.text = view.string
            measure(deferred: false)
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
    /// Return sends what is written: the composer's text, not a `TextArea`.
    var sends: Bool { onSubmit != nil }

    override func keyDown(with event: NSEvent) {
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
#endif
