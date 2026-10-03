import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// Where messages are written: the text, and under it the model, the reasoning effort and how
/// much the agent may do without asking. A strip above says when the agent is monitoring, and
/// one below says where the thread works: the server, the folder and the branch.
struct ComposerView: View {
    @Environment(AppStore.self) private var store
    @State private var textHeight: CGFloat = ComposerTextView.minimumHeight

    var body: some View {
        VStack(spacing: 0) {
            if store.selectedThread != nil, store.activity.monitoring {
                monitoringStrip
            }
            box
            if let project = store.composerProject {
                ContextStrip(project: project, server: store.server(project.serverID))
            }
        }
        .frame(maxWidth: Theme.contentWidth)
    }

    private var box: some View {
        @Bindable var store = store
        return VStack(alignment: .leading, spacing: 0) {
            if let thread = store.selectedThread, thread.isDone {
                doneBanner(thread)
            }
            if store.selectedThread != nil, !store.activity.approvals.isEmpty {
                approvals
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

            ViewThatFits(in: .horizontal) {
                controls(compact: false)
                controls(compact: true)
            }
        }
        .background {
            RoundedRectangle(cornerRadius: Self.radius, style: .continuous)
                .fill(Color.themeComposer)
                .shadow(color: .black.opacity(0.10), radius: 16, y: 8)
        }
        .overlay(
            RoundedRectangle(cornerRadius: Self.radius, style: .continuous)
                .strokeBorder(store.dropTargeted ? Color.themePrimary : Color.themeStrongBorder, lineWidth: store.dropTargeted ? 2 : 1)
        )
    }

    static let radius: CGFloat = 22

    private var monitoringStrip: some View {
        HStack(spacing: 0) {
            Circle()
                .fill(Color.themeText)
                .frame(width: 6, height: 6)
                .padding(.leading, 14)
                .padding(.trailing, 8)
            Text("Monitoring")
                .font(.system(size: 12.5, weight: .medium))
                .foregroundStyle(Color.themeText)
            Spacer(minLength: 8)
            Button {
                store.stop()
            } label: {
                Text("Stop")
                    .font(.system(size: 12.5, weight: .medium))
                    .foregroundStyle(Color.themeSecondary)
                    .padding(.horizontal, 9)
                    .frame(height: 24)
                    .padding(ComposerStrip.margin)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .hoverHighlight(radius: 7, inset: ComposerStrip.margin)
            .help("Stop monitoring (⌘.)")
        }
        .modifier(ComposerStrip(edge: .top))
    }

    private var placeholder: String {
        guard let server = store.composerServer else { return "Ask anything" }
        if server.state != .connected { return "Waiting for \(server.name) to connect…" }
        if server.models.isEmpty && server.known { return "Install Claude Code or Codex on \(server.name) to start" }
        if store.activity.running { return "Send a follow-up; it waits for the agent's turn to end" }
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

    /// The tool calls the agent waits with: each is allowed or refused, and one that asks
    /// questions is answered.
    private var approvals: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Waiting for you")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(Color.themeWarning)
            ForEach(store.activity.approvals) { approval in
                if approval.questions.isEmpty {
                    HStack(spacing: 8) {
                        Image(systemName: approval.symbol)
                            .foregroundStyle(Color.themeSecondary)
                        Text(approval.title)
                            .fontWeight(.medium)
                        Text(approval.target)
                            .font(.system(size: 12, design: .monospaced))
                            .lineLimit(1)
                            .truncationMode(.middle)
                        Spacer(minLength: 8)
                        Button(approval.refuseLabel) { store.answer(approval, allow: false) }
                            .buttonStyle(.bordered)
                        Button(approval.allowLabel) { store.answer(approval, allow: true) }
                            .buttonStyle(.borderedProminent)
                    }
                    .controlSize(.small)
                } else {
                    QuestionsView(approval: approval)
                        .id(approval.id)
                }
            }
        }
        .font(.system(size: 12.5))
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: Theme.warningBackground), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .padding([.horizontal, .top], 10)
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
                    .padding(.leading, 9)
                    .padding([.vertical, .trailing], 5)
                    .background(Color.themeBubble, in: Capsule())
                }
            }
            .padding(.horizontal, 14)
        }
        .padding(.top, 12)
    }

    /// The row under the text. When it is too narrow for all of it, the model and the access
    /// are only their icons. The space between its controls and around them is their margins,
    /// so each takes clicks up to the next one and to the composer's edges.
    private func controls(compact: Bool) -> some View {
        HStack(spacing: 0) {
            modelMenu(compact: compact)
            effortMenu
            accessMenu(compact: compact)
            Spacer(minLength: 10)
            IconOnlyButton(symbol: "paperclip", help: "Attach files", size: 30, symbolSize: 15, inset: Self.margin(trailing: 4)) {
                chooseFiles()
            }
            .foregroundStyle(Color.themeSecondary)
            primaryButtons
        }
    }

    private static func margin(leading: CGFloat = 1, trailing: CGFloat = 1) -> EdgeInsets {
        EdgeInsets(top: 8, leading: leading, bottom: 8, trailing: trailing)
    }

    private func control(_ title: String?, symbol: String? = nil, agent: Agent? = nil, margin: EdgeInsets) -> some View {
        HStack(spacing: 6) {
            if let agent {
                AgentIcon(agent: agent, size: 14)
            }
            if let symbol {
                Image(systemName: symbol)
                    .font(.system(size: 13, weight: .medium))
            }
            if let title {
                Text(title)
                    .font(.system(size: 12.5, weight: .medium))
                    .lineLimit(1)
            }
            Image(systemName: "chevron.down")
                .font(.system(size: 9, weight: .bold))
                .foregroundStyle(Color.themeTertiary)
        }
        .foregroundStyle(Color.themeSecondary)
        .padding(.horizontal, 9)
        .frame(height: 30)
        .padding(margin)
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

    @ViewBuilder private func modelMenu(compact: Bool) -> some View {
        let models = store.composerModels
        let current = store.composerModel
        let name = current?.name ?? "No agent"
        let margin = Self.margin(leading: 7)
        Menu {
            ForEach(Agent.allCases, id: \.self) { agent in
                let ofAgent = models.filter { $0.agent == agent }
                if !ofAgent.isEmpty {
                    Section(agent.name) {
                        ForEach(ofAgent) { model in
                            choice(model.name, image: agent.menuLogo, chosen: model.id == current?.id) {
                                store.setModel(model)
                            }
                        }
                    }
                }
            }
        } label: {
            control(compact && current != nil ? nil : name, agent: current?.agent, margin: margin)
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .hoverHighlight(radius: 9, inset: margin)
        .disabled(models.isEmpty)
        .help(name)
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
                control(effortLabel(store.composerEffort ?? ""), margin: Self.margin())
            }
            .menuStyle(.button)
            .buttonStyle(.plain)
            .menuIndicator(.hidden)
            .fixedSize()
            .hoverHighlight(radius: 9, inset: Self.margin())
        }
    }

    private func effortLabel(_ effort: String) -> String {
        switch effort {
        case "xhigh": return "Extra high"
        case "": return "Default"
        default: return effort.prefix(1).uppercased() + String(effort.dropFirst())
        }
    }

    private func accessMenu(compact: Bool) -> some View {
        let label = store.composerPlan ? "Plan" : store.composerAccess.label
        return Menu {
            ForEach(Access.allCases) { access in
                choice(access.label, image: NSImage(systemSymbolName: access.symbol, accessibilityDescription: nil), chosen: access == store.composerAccess) {
                    store.setAccess(access)
                }
                .help(access.detail)
            }
            Divider()
            Toggle("Plan mode", isOn: Binding(get: { store.composerPlan }, set: { store.setPlan($0) }))
        } label: {
            control(compact ? nil : label, symbol: store.composerPlan ? "list.bullet.clipboard" : store.composerAccess.symbol, margin: Self.margin())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .hoverHighlight(radius: 9, inset: Self.margin())
        .help(store.composerPlan ? "The agent only reads and proposes." : store.composerAccess.detail)
    }

    @ViewBuilder private var primaryButtons: some View {
        let running = store.activity.running && store.selectedThread != nil
        let sends = !running || store.canSend
        if running {
            Button {
                store.stop()
            } label: {
                RoundedRectangle(cornerRadius: 2.5)
                    .fill(.white)
                    .frame(width: 10, height: 10)
                    .frame(width: 30, height: 30)
                    .background(Color.themeDanger.opacity(0.9), in: Circle())
                    .padding(Self.margin(leading: 4, trailing: sends ? 4 : 8))
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help("Stop (⌘.)")
        }
        if sends {
            Button {
                store.send()
            } label: {
                Image(systemName: "arrow.up")
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(width: 30, height: 30)
                    .background(Color.themePrimary, in: Circle())
                    .opacity(store.canSend ? 1 : 0.4)
                    .padding(Self.margin(leading: 4, trailing: 8))
                    .contentShape(Rectangle())
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
    static let font = NSFont.systemFont(ofSize: 14)
    static let paragraphStyle: NSParagraphStyle = {
        let style = NSMutableParagraphStyle()
        style.lineSpacing = 3
        return style
    }()
    static let verticalInset: CGFloat = 4
    /// Two lines, so the composer only grows when a third one starts.
    static let minimumHeight: CGFloat = {
        let storage = NSTextStorage(string: "1\n2", attributes: [.font: font, .paragraphStyle: paragraphStyle])
        let layout = NSLayoutManager()
        storage.addLayoutManager(layout)
        let container = NSTextContainer(size: NSSize(width: 100, height: CGFloat.greatestFiniteMagnitude))
        layout.addTextContainer(container)
        layout.ensureLayout(for: container)
        return ceil(layout.usedRect(for: container).height) + verticalInset * 2
    }()
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
        view.font = Self.font
        view.textColor = Theme.text
        view.insertionPointColor = Theme.text
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
            .font: Self.font,
            .foregroundColor: Theme.text,
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
        let attributes: [NSAttributedString.Key: Any] = [.font: ComposerTextView.font, .foregroundColor: Theme.tertiary]
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
