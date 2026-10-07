#if os(iOS)
import SwiftUI

/// What the Mac has around its composer, as a sheet: the model, the reasoning effort, how much
/// the agent may do without asking, and where the thread works.
struct ThreadSettingsSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    @Environment(\.surface) private var surface
    /// The accounts whose models are listed. The others are only their names until they are tapped.
    @State private var listed: Set<String> = []

    var body: some View {
        @Bindable var store = store
        NavigationStack {
            List {
                models
                options
                if let project = store.composerProject {
                    workspace(project)
                }
            }
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
            .navigationTitle("Thread Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .navigationDestination(isPresented: $store.showsBranches) {
                if let project = store.composerProject {
                    BranchPicker(project: project, base: store.draftUsesWorktree ? store.draftBase : nil)
                }
            }
        }
        .presentationDragIndicator(.visible)
        .onAppear { listed = Set([store.composerAccount?.id].compactMap { $0 }) }
    }

    @ViewBuilder private var models: some View {
        let current = store.composerModel
        let account = store.composerAccount
        ForEach(store.composerChoices, id: \.account.id) { choices in
            let id = choices.account.id
            if !choices.models.isEmpty {
                Section {
                    ForEach(listed.contains(id) ? choices.models : []) { model in
                        Button {
                            store.setModel(model, account: choices.account)
                        } label: {
                            HStack {
                                Text(model.name)
                                    .foregroundStyle(Color.themeText)
                                Spacer()
                                if model.id == current?.id && id == account?.id {
                                    Image(.check, size: 13)
                                        .foregroundStyle(Color.themeText)
                                }
                            }
                        }
                    }
                } header: {
                    Button {
                        withAnimation { _ = listed.remove(id) ?? listed.insert(id).memberAfterInsert }
                    } label: {
                        HStack(spacing: 6) {
                            AgentIcon(agent: choices.account.agent, size: 14)
                            Text(choices.title)
                            Spacer()
                            Image(.chevronDown, size: 10.5)
                                .rotationEffect(.degrees(listed.contains(id) ? 180 : 0))
                        }
                        .frame(minHeight: 32)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
                .listRowBackground(surface.next.color)
            }
        }
    }

    private var options: some View {
        Section {
            if let model = store.composerModel, !model.efforts.isEmpty {
                Picker("Reasoning", selection: Binding(get: { store.composerEffort ?? "" }, set: { store.setEffort($0) })) {
                    ForEach(model.efforts, id: \.self) { effort in
                        Text(ComposerView.effortLabel(effort)).tag(effort)
                    }
                }
                .pickerStyle(.navigationLink)
            }
            Picker("Access", selection: Binding(get: { store.composerAccess }, set: { store.setAccess($0) })) {
                ForEach(Access.allCases) { access in
                    Text(access.label).tag(access)
                }
            }
            .pickerStyle(.navigationLink)
            Toggle("Plan mode", isOn: Binding(get: { store.composerPlan }, set: { store.setPlan($0) }))
        } header: {
            Text("Options")
        }
        .listRowBackground(surface.next.color)
    }

    /// Where the thread works: the server, the folder or a worktree of its own, and the branch
    /// checked out there. A thread that starts in a new worktree picks the branch it starts from.
    private func workspace(_ project: Project) -> some View {
        Section("Workspace") {
            if let server = store.server(project.serverID) {
                LabeledContent("Server", value: server.name)
            }
            LabeledContent("Project", value: project.name)
            if project.worktree != nil {
                LabeledContent("Works in", value: "Worktree")
            } else if store.selectedThread != nil, project.branch != nil {
                LabeledContent("Works in", value: "Local checkout")
            } else if store.selectedThread == nil, store.canUseWorktrees(of: project) {
                Picker("Works in", selection: Binding(get: { store.draftUsesWorktree }, set: { store.setDraftWorktree($0) })) {
                    Text("Current checkout").tag(false)
                    Text("New worktree").tag(true)
                }
            }
            let startsInWorktree = store.draftUsesWorktree
            if let branch = startsInWorktree ? store.draftBase : project.branch {
                let title = startsInWorktree ? "Starts from" : "Branch"
                let value = startsInWorktree ? store.draftStart ?? branch : branch
                if project.worktree == nil, store.canSwitchBranches(of: project) {
                    Button {
                        store.showBranches(of: project)
                    } label: {
                        HStack(spacing: 8) {
                            LabeledContent(title, value: value)
                            Image(.chevronRight, size: 11)
                                .foregroundStyle(Color.themeTertiary)
                        }
                        .foregroundStyle(Color.themeText)
                    }
                } else {
                    LabeledContent(title, value: value)
                }
            }
        }
        .listRowBackground(surface.next.color)
    }
}
#endif
