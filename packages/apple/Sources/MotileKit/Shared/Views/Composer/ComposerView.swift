import SwiftUI
import UniformTypeIdentifiers

/// Where messages are written. A strip above says what the agent waits for, that it is
/// monitoring, or why it stopped before it finished. On the Mac the model, the reasoning effort
/// and how much the agent may do without asking are under the text, and a strip below says where
/// the thread works: the server, the folder and the branch. On iOS all of that is in the thread's
/// settings, which the model's name opens, and the composer is one line until it is written in.
struct ComposerView: View {
    @Environment(AppStore.self) private var store
    @State private var textHeight: CGFloat = ComposerTextView.minimumHeight
    /// Counts up when the composer is pressed beside its text, which then takes the keyboard.
    @State private var pressed = 0
    #if os(iOS)
    @State private var focused = false
    #endif

    var body: some View {
        GlassGroup {
            VStack(spacing: 0) {
                if store.selectedThread != nil, let approval = store.activity.approvals.first {
                    WaitingStrip(approval: approval, count: store.activity.approvals.count)
                } else if store.selectedThread != nil, store.activity.monitoring {
                    monitoringStrip
                } else if let thread = store.selectedThread, !thread.busy, !store.activity.busy, let interruption = thread.interruption {
                    InterruptionStrip(interruption: interruption)
                }
                box
                    .zIndex(1)
                #if os(macOS)
                if let project = store.composerProject {
                    ContextStrip(project: project, server: store.server(project.serverID))
                }
                #endif
            }
        }
        .composerOutlineShadow()
        .frame(maxWidth: Theme.composerWidth)
        .task(id: store.draftWorktreeKey) { store.readWorktreeStart(fetch: true) }
        .onChange(of: store.composerProject?.git) { store.readWorktreeStart(fetch: false) }
    }

    private var box: some View {
        @Bindable var store = store
        let text = ComposerTextView(
            text: $store.draft,
            height: $textHeight,
            placeholder: placeholder,
            focusKey: "\(store.draftKey)#\(store.composerFocus)#\(pressed)",
            onSubmit: { store.send() },
            onFiles: { store.attach($0) },
            onFileDrag: { store.composerDropTargeted = $0 }
        )
        return VStack(alignment: .leading, spacing: 0) {
            #if os(macOS)
            VStack(alignment: .leading, spacing: 0) {
                aboveText
                text
                    .frame(height: textHeight)
                    .padding(.horizontal, 14)
                    .padding(.top, 12)
            }
            .background { typingArea }

            ViewThatFits(in: .horizontal) {
                controls(compact: false)
                controls(compact: true)
            }
            #else
            aboveText
            let collapsed = !focused && store.draft.isEmpty && store.attachments.isEmpty
            ComposerRows(collapsed: collapsed) {
                text
                    .reporting(focus: $focused)
                    .focusing(on: pressed)
                    .frame(height: textHeight)
                ComposerTouchControls(collapsed: collapsed)
            }
            .animation(.easeOut(duration: 0.22), value: collapsed)
            #endif
        }
        .background {
            Color.clear
                .contentShape(Rectangle())
                .onTapGesture { pressed += 1 }
        }
        .composerSurface(in: RoundedRectangle(cornerRadius: Self.radius, style: .continuous), shadow: .sm)
        .anchorPreference(key: ComposerPlace.self, value: .bounds) { ComposerPlace.Value(box: $0) }
    }

    @ViewBuilder private var aboveText: some View {
        if let thread = store.selectedThread, thread.isDone {
            doneBanner(thread)
        }
        if !store.attachments.isEmpty {
            attachments
        }
    }

    /// Empty room in the composer: a click there starts typing, under the cursor of text. It is
    /// never under a control, whose menu would keep that cursor.
    private var typingArea: some View {
        Color.clear
            .contentShape(Rectangle())
            .onTapGesture { pressed += 1 }
            .textPointer()
    }

    static let radius = Radius.xxl

    private func monitoringLabel(now: Double) -> String {
        guard let thread = store.selectedThread else { return "Monitoring" }
        return "Monitoring for \(Time.elapsed(since: thread.monitoringSince, now: now))"
    }

    private var monitoringStrip: some View {
        HStack(spacing: 0) {
            Circle()
                .fill(Color.themeForeground)
                .frame(width: 6, height: 6)
                .padding(.leading, 14)
                .padding(.trailing, 8)
            TimelineView(.periodic(from: .now, by: 1)) { context in
                Text(monitoringLabel(now: context.date.timeIntervalSince1970))
                    .font(.ui(size: 12.5, weight: .medium))
                    .foregroundStyle(Color.themeForeground)
            }
            Spacer(minLength: 8)
            ActionButton("Stop", help: "Stop monitoring (⌘.)", variant: .ghost, margin: ComposerStrip.margin) { store.stop() }
        }
        .modifier(ComposerStrip(edge: .top))
    }

    private var placeholder: String {
        guard let server = store.composerServer else { return "Ask anything" }
        if store.selectedThread?.isDone == true { return "Message to bring it back" }
        if server.state != .connected { return "Waiting for \(server.name)…" }
        if server.models.isEmpty && server.known { return "Install Claude Code or Codex" }
        if store.activity.running { return "Send a follow-up" }
        return "Ask anything"
    }

    private static let undoneButtonPadding = ControlSize.small.padding

    private func doneBanner(_ thread: ThreadInfo) -> some View {
        HStack(spacing: 8) {
            Image(.circleCheck, size: 13)
                .foregroundStyle(Color.themeSuccess)
            Text("Done")
                .fontWeight(.medium)
            Spacer()
            ActionButton("Mark Undone", variant: .link, size: .small) { store.setDone([thread.id], done: false) }
                .padding(.vertical, -4)
        }
        .font(.ui(size: 12.5))
        .padding(.leading, 16)
        .padding(.trailing, 16 - Self.undoneButtonPadding)
        .padding(.top, 12)
    }

    private var attachments: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(alignment: .bottom, spacing: 8) {
                let pictured = store.attachments.filter(\.pictured)
                ForEach(pictured) { attachment in
                    AttachmentTile(attachment: attachment) {
                        store.view(pictured.compactMap(\.viewed), at: pictured.firstIndex(of: attachment) ?? 0)
                    }
                }
                ForEach(store.attachments.filter { !$0.pictured }) { attachment in
                    AttachmentChip(attachment: attachment)
                }
            }
            .padding(.horizontal, 14)
            .padding(.top, 12)
        }
    }

    static func effortLabel(_ effort: String) -> String {
        switch effort {
        case "xhigh": return "Extra high"
        case "": return "Default"
        default: return effort.prefix(1).uppercased() + String(effort.dropFirst())
        }
    }

    static func margin(leading: CGFloat = 1, trailing: CGFloat = 1) -> EdgeInsets {
        EdgeInsets(top: 8, leading: leading, bottom: 8, trailing: trailing)
    }

    #if os(macOS)
    /// The row under the text. When it is too narrow for all of it, the model and the access
    /// are only their icons. The space between its controls and around them is their margins,
    /// so each takes clicks up to the next one and to the composer's edges.
    private func controls(compact: Bool) -> some View {
        HStack(spacing: 0) {
            modelMenu(compact: compact)
            ComposerDivider()
            effortMenu
            accessMenu(compact: compact)
            typingArea
                .frame(minWidth: 10, maxWidth: .infinity, maxHeight: .infinity)
            ActionButton(icon: .paperclip, help: "Attach files", round: true, margin: Self.margin(trailing: 4)) { chooseFiles() }
            ComposerSendButtons()
        }
        .fixedSize(horizontal: false, vertical: true)
    }

    /// A menu item with a check mark when it is the one in use.
    private func choice(_ title: String, image: PlatformImage? = nil, chosen: Bool, choose: @escaping () -> Void) -> some View {
        Toggle(isOn: Binding(get: { chosen }, set: { _ in choose() })) {
            if let image {
                Label {
                    Text(title)
                } icon: {
                    Image(platform: image)
                }
            } else {
                Text(title)
            }
        }
    }

    @ViewBuilder private func modelMenu(compact: Bool) -> some View {
        let current = store.composerModel
        let account = store.composerAccount
        let name = store.composerModelLabel
        let margin = Self.margin(leading: 8, trailing: 4)
        let logo = current.map { AnyView(AgentIcon(agent: $0.agent, size: ControlSize.regular.symbol)) }
        ActionMenu(
            compact && current != nil ? nil : name, picture: logo, help: current?.name ?? name, symbolSize: ControlSize.regular.symbol,
            gap: 5, margin: margin
        ) {
            ForEach(store.composerChoices, id: \.account.id) { choices in
                Section(choices.title) {
                    ForEach(choices.models) { model in
                        let chosen = model.id == current?.id && choices.account.id == account?.id
                        choice(model.name, image: model.agent.menuLogo, chosen: chosen) {
                            store.setModel(model, account: choices.account)
                        }
                    }
                }
            }
        }
        .disabled(store.composerModels.isEmpty)
    }

    @ViewBuilder private var effortMenu: some View {
        if let model = store.composerModel, !model.efforts.isEmpty {
            ActionMenu(Self.effortLabel(store.composerEffort ?? ""), margin: Self.margin(leading: 4, trailing: 4)) {
                ForEach(model.efforts, id: \.self) { effort in
                    choice(Self.effortLabel(effort), chosen: effort == store.composerEffort) {
                        store.setEffort(effort)
                    }
                }
            }
            ComposerDivider()
        }
    }

    private func accessMenu(compact: Bool) -> some View {
        let label = store.composerPlan ? "Plan" : store.composerAccess.label
        return ActionMenu(
            compact ? nil : label, icon: store.composerPlan ? .clipboardList : store.composerAccess.symbol,
            help: store.composerPlan ? "The agent only reads and proposes." : store.composerAccess.detail,
            symbolSize: ControlSize.small.symbol, gap: 5, margin: Self.margin(leading: 4)
        ) {
            ForEach(Access.allCases) { access in
                choice(access.label, image: PlatformImage.symbol(access.symbol, size: 13), chosen: access == store.composerAccess) {
                    store.setAccess(access)
                }
                .help(access.detail)
            }
            Divider()
            Toggle("Plan mode", isOn: Binding(get: { store.composerPlan }, set: { store.setPlan($0) }))
        }
    }

    private func chooseFiles() {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = true
        panel.canChooseDirectories = false
        guard panel.runModal() == .OK else { return }
        store.attach(panel.urls)
    }
    #endif
}

/// Stops the turn that runs, and sends what is written or queues it behind that turn.
struct ComposerSendButtons: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        let running = store.activity.running && store.selectedThread != nil
        let sends = !running || store.canSend
        HStack(spacing: 0) {
            if running {
                ActionButton(
                    "", picture: AnyView(RoundedRectangle(cornerRadius: 2.5).frame(width: 10, height: 10)), help: "Stop (⌘.)", variant: .danger,
                    round: true, margin: ComposerView.margin(leading: 4, trailing: sends ? 4 : 8)
                ) {
                    store.stop()
                }
                .accessibilityLabel("Stop")
            }
            if sends {
                ActionButton(
                    icon: .arrowUp, help: store.attachmentsHold ?? (running ? "Queue message" : "Send"), variant: .primary, round: true,
                    margin: ComposerView.margin(leading: 4, trailing: 8)
                ) {
                    store.send()
                }
                .disabled(!store.canSend)
                .accessibilityLabel("Send")
            }
        }
    }
}

/// Where the composer is, for the transcript behind it.
struct ComposerPlace: PreferenceKey {
    struct Value {
        /// The composer with the room around it, which the rows end above.
        var room: Anchor<CGRect>?
        var box: Anchor<CGRect>?
    }

    static var defaultValue: Value { Value() }

    static func reduce(value: inout Value, nextValue: () -> Value) {
        let next = nextValue()
        value.room = next.room ?? value.room
        value.box = next.box ?? value.box
    }
}
