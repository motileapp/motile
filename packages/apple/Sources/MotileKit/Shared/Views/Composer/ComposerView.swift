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

    private static let undoneButtonPadding = 9.0

    private func doneBanner(_ thread: ThreadInfo) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "checkmark.circle")
                .foregroundStyle(Color.themeSuccess)
            Text("This thread is done.")
                .fontWeight(.medium)
            Text("Send a message to bring it back.")
                .foregroundStyle(Color.themeSecondary)
            Spacer()
            Button {
                store.setDone([thread.id], done: false)
            } label: {
                Text("Mark Undone")
                    .fontWeight(.medium)
                    .foregroundStyle(Color.themePrimary)
                    .padding(.horizontal, Self.undoneButtonPadding)
                    .frame(height: 24)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .hoverHighlight(radius: 7, color: .themePrimaryHover)
            .padding(.vertical, -4)
        }
        .font(.ui(size: 12.5))
        .padding(.leading, 16)
        .padding(.trailing, 16 - Self.undoneButtonPadding)
        .padding(.top, 12)
    }

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

    /// The row under the text. When it is too narrow for all of it, the model and the access
    /// are only their icons. The space between its controls and around them is their margins,
    /// so each takes clicks up to the next one and to the composer's edges.
    private func controls(compact: Bool) -> some View {
        HStack(spacing: 0) {
            #if os(iOS)
            AttachMenu()
                .padding(.leading, 6)
            #endif
            modelMenu(compact: compact)
            effortMenu
            accessMenu(compact: compact)
            Spacer(minLength: 10)
            #if os(macOS)
            IconOnlyButton(symbol: "paperclip", help: "Attach files", size: 30, symbolSize: 15, inset: Self.margin(trailing: 4)) {
                chooseFiles()
            }
            .foregroundStyle(Color.themeSecondary)
            #endif
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
                    .font(.ui(size: 14, weight: .semibold))
                    .foregroundStyle(store.canSend ? Color.white : Color.themeSecondary)
                    .frame(width: 30, height: 30)
                    .background(store.canSend ? Color.themePrimary : Color.themeSelected, in: Circle())
                    .padding(Self.margin(leading: 4, trailing: 8))
                    .contentShape(Rectangle())
            }
            .buttonStyle(SendButtonStyle())
            .disabled(!store.canSend)
            .help(store.attachmentsHold ?? (running ? "Queue message" : "Send"))
        }
    }

    #if os(macOS)
    private func chooseFiles() {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = true
        panel.canChooseDirectories = false
        guard panel.runModal() == .OK else { return }
        store.attach(panel.urls)
    }
    #endif
}

/// The plain style dims a disabled label; this one leaves the disabled look to the label.
private struct SendButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label.opacity(configuration.isPressed ? 0.7 : 1)
    }
}
