import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// Where messages are written: the text, and under it the model, the reasoning effort and how
/// much the agent may do without asking.
struct ComposerView: View {
    @Environment(AppStore.self) private var store
    @State private var textHeight: CGFloat = ComposerTextView.minimumHeight

    var body: some View {
        @Bindable var store = store
        VStack(alignment: .leading, spacing: 0) {
            if let thread = store.selectedThread, thread.isDone {
                doneBanner(thread)
            }
            if !store.attachments.isEmpty {
                attachments
            }
            ComposerTextView(
                text: $store.draft,
                height: $textHeight,
                placeholder: placeholder,
                focusKey: "\(store.draftKey)#\(store.composerFocus)",
                onSubmit: { store.send() },
                onFiles: { store.attach($0) },
                onFileDrag: { store.dropTargeted = $0 }
            )
            .frame(height: textHeight)
            .padding(.horizontal, 14)
            .padding(.top, 12)

            HStack(spacing: 2) {
                modelMenu
                effortMenu
                accessMenu
                Spacer(minLength: 8)
                IconOnlyButton(symbol: "paperclip", help: "Attach files", size: 30, symbolSize: 15) {
                    chooseFiles()
                }
                .foregroundStyle(Color.themeSecondary)
                .padding(.trailing, 6)
                HStack(spacing: 8) {
                    primaryButtons
                }
            }
            .padding(.leading, 7)
            .padding([.trailing, .bottom, .top], 8)
        }
        .frame(maxWidth: Theme.contentWidth)
        .background {
            RoundedRectangle(cornerRadius: 22, style: .continuous)
                .fill(Color.themeComposer)
                .shadow(color: .black.opacity(0.10), radius: 16, y: 8)
        }
        .overlay(
            RoundedRectangle(cornerRadius: 22, style: .continuous)
                .stroke(store.dropTargeted ? Color.themePrimary : Color.themeStrongBorder, lineWidth: store.dropTargeted ? 2 : 1)
        )
    }

    private var placeholder: String {
        guard let host = store.composerHost else { return "Ask anything" }
        if host.state != .connected { return "Waiting for \(host.name) to connect…" }
        if host.models.isEmpty && host.known { return "Install Claude Code or Codex on \(host.name) to start" }
        if store.activity.running { return "Send a follow-up; it starts when this turn ends" }
        return "Ask anything, or describe what to build"
    }

    private func doneBanner(_ thread: ThreadInfo) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "checkmark.circle")
                .foregroundStyle(Color.themeSuccess)
            Text("This thread is done.")
                .fontWeight(.medium)
            Text("Send a message to bring it back.")
                .foregroundStyle(Color.themeSecondary)
            Spacer()
            Button("Mark Undone") { store.setDone([thread.id], done: false) }
                .buttonStyle(.link)
        }
        .font(.system(size: 12.5))
        .padding(.horizontal, 16)
        .padding(.top, 12)
    }

    private var attachments: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(store.attachments, id: \.self) { path in
                    HStack(spacing: 5) {
                        Image(systemName: "doc")
                        Text(URL(fileURLWithPath: path).lastPathComponent)
                            .lineLimit(1)
                        IconOnlyButton(symbol: "xmark", help: "Remove", size: 18, symbolSize: 10) {
                            store.attachments.removeAll { $0 == path }
                        }
                    }
                    .font(.system(size: 12))
                    .padding(.horizontal, 9)
                    .padding(.vertical, 5)
                    .background(Color.themeBubble, in: Capsule())
                }
            }
            .padding(.horizontal, 14)
        }
        .padding(.top, 12)
    }

    private func control(_ title: String, symbol: String? = nil, agent: Agent? = nil) -> some View {
        HStack(spacing: 6) {
            if let agent {
                AgentIcon(agent: agent, size: 14)
            }
            if let symbol {
                Image(systemName: symbol)
                    .font(.system(size: 13, weight: .medium))
            }
            Text(title)
                .font(.system(size: 12.5, weight: .medium))
                .lineLimit(1)
            Image(systemName: "chevron.down")
                .font(.system(size: 9, weight: .bold))
                .foregroundStyle(Color.themeTertiary)
        }
        .foregroundStyle(Color.themeSecondary)
        .padding(.horizontal, 9)
        .frame(height: 30)
        .contentShape(Rectangle())
    }

    /// A menu item with a check mark when it is the one in use.
    private func choice(_ title: String, image: NSImage? = nil, chosen: Bool, choose: @escaping () -> Void) -> some View {
        Toggle(isOn: Binding(get: { chosen }, set: { _ in choose() })) {
            if let image {
                Label {
                    Text(title)
                } icon: {
                    Image(nsImage: image)
                }
            } else {
                Text(title)
            }
        }
    }

    @ViewBuilder private var modelMenu: some View {
        let models = store.composerModels
        Menu {
            ForEach(Agent.allCases, id: \.self) { agent in
                let ofAgent = models.filter { $0.agent == agent }
                if !ofAgent.isEmpty {
                    Section(agent.name) {
                        ForEach(ofAgent) { model in
                            choice(model.name, image: agent.menuLogo, chosen: model.id == store.composerModel?.id) {
                                store.setModel(model)
                            }
                        }
                    }
                }
            }
        } label: {
            control(store.composerModel?.name ?? "No agent", agent: store.composerModel?.agent)
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .hoverHighlight(radius: 9)
        .disabled(models.isEmpty)
    }

    @ViewBuilder private var effortMenu: some View {
        if let model = store.composerModel, !model.efforts.isEmpty {
            Menu {
                ForEach(model.efforts, id: \.self) { effort in
                    choice(effortLabel(effort), chosen: effort == store.composerEffort) {
                        store.setEffort(effort)
                    }
                }
            } label: {
                control(effortLabel(store.composerEffort ?? ""))
            }
            .menuStyle(.button)
            .buttonStyle(.plain)
            .menuIndicator(.hidden)
            .fixedSize()
            .hoverHighlight(radius: 9)
        }
    }

    private func effortLabel(_ effort: String) -> String {
        switch effort {
        case "xhigh": return "Extra high"
        case "": return "Default"
        default: return effort.prefix(1).uppercased() + String(effort.dropFirst())
        }
    }

    private var accessMenu: some View {
        Menu {
            ForEach(Access.allCases) { access in
                choice(access.label, image: NSImage(systemSymbolName: access.symbol, accessibilityDescription: nil), chosen: access == store.composerAccess) {
                    store.setAccess(access)
                }
                .help(access.detail)
            }
            Divider()
            Toggle("Plan mode", isOn: Binding(get: { store.composerPlan }, set: { store.setPlan($0) }))
        } label: {
            control(store.composerPlan ? "Plan" : store.composerAccess.label, symbol: store.composerPlan ? "list.bullet.clipboard" : store.composerAccess.symbol)
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .hoverHighlight(radius: 9)
        .help(store.composerPlan ? "The agent only reads and proposes." : store.composerAccess.detail)
    }

    @ViewBuilder private var primaryButtons: some View {
        let running = store.activity.running && store.selectedThread != nil
        if running {
            Button {
                store.stop()
            } label: {
                RoundedRectangle(cornerRadius: 2.5)
                    .fill(.white)
                    .frame(width: 10, height: 10)
                    .frame(width: 30, height: 30)
                    .background(Color.themeDanger.opacity(0.9), in: Circle())
            }
            .buttonStyle(.plain)
            .help("Stop (⌘.)")
        }
        if !running || store.canSend {
            Button {
                store.send()
            } label: {
                Image(systemName: "arrow.up")
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(width: 30, height: 30)
                    .background(Color.themePrimary, in: Circle())
                    .opacity(store.canSend ? 1 : 0.4)
            }
            .buttonStyle(.plain)
            .disabled(!store.canSend)
            .help(running ? "Queue message" : "Send")
        }
    }

    private func chooseFiles() {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = true
        panel.canChooseDirectories = false
        guard panel.runModal() == .OK else { return }
        store.attach(panel.urls)
    }
}

/// The composer's text: grows with what is typed, sends on Return, and takes dropped files.
struct ComposerTextView: NSViewRepresentable {
    static let minimumHeight: CGFloat = 44
    static let maximumHeight: CGFloat = 220

    @Binding var text: String
    @Binding var height: CGFloat
    let placeholder: String
    /// Changes when another thread's draft is shown, which is when the cursor should come here.
    let focusKey: String
    let onSubmit: () -> Void
    let onFiles: ([URL]) -> Void
    /// Files are being dragged over the text, or no longer are.
    let onFileDrag: (Bool) -> Void

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSScrollView()
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
        view.font = NSFont.systemFont(ofSize: 14)
        view.textColor = Theme.text
        view.insertionPointColor = Theme.text
        view.textContainerInset = NSSize(width: 0, height: 4)
        view.textContainer?.lineFragmentPadding = 2
        view.isVerticallyResizable = true
        view.isHorizontallyResizable = false
        view.autoresizingMask = [.width]
        view.textContainer?.widthTracksTextView = true
        view.isAutomaticQuoteSubstitutionEnabled = false
        view.isAutomaticDashSubstitutionEnabled = false
        view.isAutomaticTextReplacementEnabled = false
        view.isAutomaticSpellingCorrectionEnabled = false
        view.defaultParagraphStyle = {
            let style = NSMutableParagraphStyle()
            style.lineSpacing = 3
            return style
        }()
        view.typingAttributes = [
            .font: NSFont.systemFont(ofSize: 14),
            .foregroundColor: Theme.text,
            .paragraphStyle: view.defaultParagraphStyle ?? NSParagraphStyle.default,
        ]
        view.onSubmit = onSubmit
        view.onFiles = onFiles
        view.onFileDrag = onFileDrag
        view.placeholder = placeholder
        view.string = text
        scroll.documentView = view
        context.coordinator.textView = view
        DispatchQueue.main.async {
            view.window?.makeFirstResponder(view)
            context.coordinator.measure()
        }
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
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
        var focusKey: String

        init(_ parent: ComposerTextView) {
            self.parent = parent
            focusKey = parent.focusKey
        }

        func textDidChange(_ notification: Notification) {
            guard let view = textView else { return }
            parent.text = view.string
            measure()
        }

        /// Makes the composer as tall as its text, within limits.
        func measure() {
            guard let view = textView, let container = view.textContainer, let layout = view.layoutManager else { return }
            layout.ensureLayout(for: container)
            let used = layout.usedRect(for: container).height + view.textContainerInset.height * 2
            let height = min(ComposerTextView.maximumHeight, max(ComposerTextView.minimumHeight, ceil(used)))
            guard abs(parent.height - height) > 0.5 else { return }
            DispatchQueue.main.async { self.parent.height = height }
        }
    }
}

final class ComposerNSTextView: NSTextView {
    var onSubmit: (() -> Void)?
    var onFiles: (([URL]) -> Void)?
    var onFileDrag: ((Bool) -> Void)?
    var placeholder = ""

    override func keyDown(with event: NSEvent) {
        let isReturn = event.keyCode == 36 || event.keyCode == 76
        let modifiers = event.modifierFlags.intersection([.shift, .option, .control])
        // Return sends; with Shift or Option it is a line break. While an input method is
        // composing, Return belongs to it.
        guard isReturn, modifiers.isEmpty, !hasMarkedText() else {
            super.keyDown(with: event)
            return
        }
        onSubmit?()
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        guard string.isEmpty, !placeholder.isEmpty else { return }
        let attributes: [NSAttributedString.Key: Any] = [.font: NSFont.systemFont(ofSize: 14), .foregroundColor: Theme.tertiary]
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
