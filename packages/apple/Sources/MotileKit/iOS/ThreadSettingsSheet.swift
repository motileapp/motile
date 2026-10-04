#if os(iOS)
import SwiftUI

/// What the Mac has around its composer, as a sheet: the model, the reasoning effort, how much
/// the agent may do without asking, and where the thread works.
struct ThreadSettingsSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    /// The agents whose models are listed. The others are only their names until they are tapped.
    @State private var listed: Set<Agent> = []

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
            .background(Color.themeSheet)
            .navigationTitle("Thread settings")
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
        .onAppear { listed = Set([store.composerModel?.agent].compactMap { $0 }) }
    }

    @ViewBuilder private var models: some View {
        let current = store.composerModel
        ForEach(Agent.allCases, id: \.self) { agent in
            let ofAgent = store.composerModels.filter { $0.agent == agent }
            if !ofAgent.isEmpty {
                Section {
                    ForEach(listed.contains(agent) ? ofAgent : []) { model in
                        Button {
                            store.setModel(model)
                        } label: {
                            HStack {
                                Text(model.name)
                                    .foregroundStyle(Color.themeText)
                                Spacer()
                                if model.id == current?.id {
                                    Image(.check, size: 13)
                                        .foregroundStyle(Color.themeText)
                                }
                            }
                        }
                    }
                } header: {
                    Button {
                        withAnimation { _ = listed.remove(agent) ?? listed.insert(agent).memberAfterInsert }
                    } label: {
                        HStack(spacing: 6) {
                            AgentIcon(agent: agent, size: 14)
                            Text(agent.name)
                            Spacer()
                            Image(.chevronDown, size: 10.5)
                                .rotationEffect(.degrees(listed.contains(agent) ? 180 : 0))
                        }
                        .frame(minHeight: 32)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
                .listRowBackground(Color.themeField)
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
        } footer: {
            Text(store.composerPlan ? "The agent only reads and proposes." : store.composerAccess.detail)
        }
        .listRowBackground(Color.themeField)
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
                if project.worktree == nil, store.canSwitchBranches(of: project) {
                    Button {
                        store.showBranches(of: project)
                    } label: {
                        HStack(spacing: 8) {
                            LabeledContent(title, value: branch)
                            Image(.chevronRight, size: 11)
                                .foregroundStyle(Color.themeTertiary)
                        }
                        .foregroundStyle(Color.themeText)
                    }
                } else {
                    LabeledContent(title, value: branch)
                }
            }
        }
        .listRowBackground(Color.themeField)
    }
}
#endif
