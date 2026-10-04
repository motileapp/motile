import SwiftUI
import UniformTypeIdentifiers

/// Where messages are written. A strip above says when the agent is monitoring. On the Mac the
/// model, the reasoning effort and how much the agent may do without asking are under the text,
/// and a strip below says where the thread works: the server, the folder and the branch. On iOS
/// all of that is in the thread's settings, which the model's name opens, and the composer is
/// one line until it is written in.
struct ComposerView: View {
    @Environment(AppStore.self) private var store
    @State private var textHeight: CGFloat = ComposerTextView.minimumHeight
    #if os(iOS)
    @State private var focused = false
    #endif

    var body: some View {
        GlassGroup {
            VStack(spacing: 0) {
                if store.selectedThread != nil, store.activity.monitoring {
                    monitoringStrip
                }
                box
                #if os(macOS)
                if let project = store.composerProject {
                    ContextStrip(project: project, server: store.server(project.serverID))
                }
                #endif
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
            let text = ComposerTextView(
                text: $store.draft,
                height: $textHeight,
                placeholder: placeholder,
                focusKey: "\(store.draftKey)#\(store.composerFocus)",
                onSubmit: { store.send() },
                onFiles: { store.attach($0) },
                onFileDrag: { store.composerDropTargeted = $0 }
            )
            #if os(macOS)
            text
                .frame(height: textHeight)
                .padding(.horizontal, 14)
                .padding(.top, 12)

            ViewThatFits(in: .horizontal) {
                controls(compact: false)
                controls(compact: true)
            }
            #else
            let collapsed = !focused && store.draft.isEmpty && store.attachments.isEmpty
            ComposerRows(collapsed: collapsed) {
                text
                    .reporting(focus: $focused)
                    .frame(height: textHeight)
                ComposerTouchControls(collapsed: collapsed)
            }
            .animation(.easeOut(duration: 0.22), value: collapsed)
            #endif
        }
        .glassSurface(in: RoundedRectangle(cornerRadius: Self.radius, style: .continuous))
        .anchorPreference(key: ComposerPlace.self, value: .bounds) { ComposerPlace.Value(box: $0) }
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
                .font(.ui(size: 12.5, weight: .medium))
                .foregroundStyle(Color.themeText)
            Spacer(minLength: 8)
            Button {
                store.stop()
            } label: {
                Text("Stop")
                    .font(.ui(size: 12.5, weight: .medium))
                    .foregroundStyle(Color.themeSecondary)
                    .padding(.horizontal, 9)
                    .frame(height: 24)
                    .padding(ComposerStrip.margin)
                    .frame(minHeight: Platform.minimumPress)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.highlight(radius: 7, inset: ComposerStrip.margin))
            .help("Stop monitoring (⌘.)")
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

    private static let undoneButtonPadding = 9.0

    private func doneBanner(_ thread: ThreadInfo) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "checkmark.circle")
                .foregroundStyle(Color.themeSuccess)
            Text("Done")
                .fontWeight(.medium)
            Spacer()
            Button {
                store.setDone([thread.id], done: false)
            } label: {
                Text("Mark Undone")
                    .fontWeight(.medium)
                    .foregroundStyle(Color.themePrimary)
                    .padding(.horizontal, Self.undoneButtonPadding)
                    .frame(height: scaled(24))
                    .padding(.vertical, Self.undoneReach)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.highlight(radius: 7, inset: EdgeInsets(top: Self.undoneReach, leading: 0, bottom: Self.undoneReach, trailing: 0), color: .themePrimaryHover))
            .padding(.vertical, -4 - Self.undoneReach)
        }
        .font(.ui(size: 12.5))
        .padding(.leading, 16)
        .padding(.trailing, 16 - Self.undoneButtonPadding)
        .padding(.top, 12)
    }

    /// How far above and under the button a finger still presses it.
    private static let undoneReach = max(0, (Platform.minimumPress - scaled(24)) / 2)

    /// The tool calls the agent waits with: each is allowed or refused, and one that asks
    /// questions is answered.
    private var approvals: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Waiting for you")
                .font(.ui(size: 12, weight: .semibold))
                .foregroundStyle(Color.themeWarning)
            ForEach(store.activity.approvals) { approval in
                if approval.questions.isEmpty {
                    HStack(spacing: 8) {
                        Image(systemName: approval.symbol)
                            .foregroundStyle(Color.themeSecondary)
                        Text(approval.title)
                            .fontWeight(.medium)
                        Text(approval.target)
                            .font(.ui(size: 12, design: .monospaced))
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
        .font(.ui(size: 12.5))
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(platform: Theme.warningBackground), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .padding([.horizontal, .top], 10)
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
            effortMenu
            accessMenu(compact: compact)
            Spacer(minLength: 10)
            IconOnlyButton(symbol: "paperclip", help: "Attach files", size: 30, symbolSize: 15, inset: Self.margin(trailing: 4)) {
                chooseFiles()
            }
            .foregroundStyle(Color.themeSecondary)
            ComposerSendButtons()
        }
    }

    private func control(_ title: String?, symbol: String? = nil, agent: Agent? = nil, margin: EdgeInsets) -> some View {
        HStack(spacing: 6) {
            if let agent {
                AgentIcon(agent: agent, size: 14)
            }
            if let symbol {
                Image(systemName: symbol)
                    .font(.ui(size: 13, weight: .medium))
            }
            if let title {
                Text(title)
                    .font(.ui(size: 12.5, weight: .medium))
                    .lineLimit(1)
            }
            Image(systemName: "chevron.down")
                .font(.ui(size: 9, weight: .bold))
                .foregroundStyle(Color.themeTertiary)
        }
        .foregroundStyle(Color.themeSecondary)
        .padding(.horizontal, 9)
        .frame(height: 30)
        .padding(margin)
        .contentShape(Rectangle())
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
                    choice(Self.effortLabel(effort), chosen: effort == store.composerEffort) {
                        store.setEffort(effort)
                    }
                }
            } label: {
                control(Self.effortLabel(store.composerEffort ?? ""), margin: Self.margin())
            }
            .menuStyle(.button)
            .buttonStyle(.plain)
            .menuIndicator(.hidden)
            .fixedSize()
            .hoverHighlight(radius: 9, inset: Self.margin())
        }
    }

    private func accessMenu(compact: Bool) -> some View {
        let label = store.composerPlan ? "Plan" : store.composerAccess.label
        return Menu {
            ForEach(Access.allCases) { access in
                choice(access.label, image: PlatformImage.symbol(access.symbol, size: 13), chosen: access == store.composerAccess) {
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
                Button {
                    store.stop()
                } label: {
                    RoundedRectangle(cornerRadius: 2.5)
                        .fill(.white)
                        .frame(width: 10, height: 10)
                        .frame(width: 30, height: 30)
                        .background(Color.themeDanger.opacity(0.9), in: Circle())
                        .padding(ComposerView.margin(leading: 4, trailing: sends ? 4 : 8))
                        .contentShape(Rectangle())
                }
                .buttonStyle(DimButtonStyle())
                .help("Stop (⌘.)")
                .accessibilityLabel("Stop")
            }
            if sends {
                Button {
                    store.send()
                } label: {
                    Image(systemName: "arrow.up")
                        .font(.ui(size: 14, weight: .semibold))
                        .foregroundStyle(store.canSend ? Color.white : Color.themeSecondary)
                        .frame(width: 30, height: 30)
                        .background(store.canSend ? Color.themePrimary : Color.themeSelected, in: Circle())
                        .padding(ComposerView.margin(leading: 4, trailing: 8))
                        .contentShape(Rectangle())
                }
                .buttonStyle(DimButtonStyle())
                .disabled(!store.canSend)
                .help(store.attachmentsHold ?? (running ? "Queue message" : "Send"))
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
