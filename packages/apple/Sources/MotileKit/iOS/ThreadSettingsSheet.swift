#if os(iOS)
import SwiftUI

/// What the Mac has around its composer, as a sheet: the model, the reasoning effort, how much
/// the agent may do without asking, and where the thread works.
struct ThreadSettingsSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.surface) private var surface
    /// The accounts whose models are listed. The others are only their names until they are tapped.
    @State private var listed: Set<String> = []
    @State private var tmpReasoning = false

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
                GlassToolbarItem(placement: .topBarLeading) { SheetCloseButton() }
            }
            .navigationDestination(isPresented: $tmpReasoning) {
                if let model = store.composerModel {
                    ChoiceList(
                        title: "Reasoning", options: model.effortChoices, chosen: store.composerEffort ?? "", label: ComposerView.effortLabel,
                        symbol: ComposerView.effortSymbol
                    ) { store.setEffort($0) }
                    .glassBackButton()
                }
            }
            .onAppear { tmpReasoning = UserDefaults.standard.bool(forKey: "tmpReasoning") }
            .navigationDestination(isPresented: $store.showsBranches) {
                if let project = store.composerProject {
                    BranchPicker(project: project, base: store.draftUsesWorktree ? store.draftBase : nil)
                        .glassBackButton()
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
                                    .foregroundStyle(Color.themeForeground)
                                Spacer()
                                if model.id == current?.id && id == account?.id {
                                    Image(.check, size: .md)
                                        .foregroundStyle(Color.themeForeground)
                                }
                            }
                        }
                    }
                } header: {
                    Button {
                        withAnimation { _ = listed.remove(id) ?? listed.insert(id).memberAfterInsert }
                    } label: {
                        HStack(spacing: 6) {
                            AgentIcon(agent: choices.account.agent, size: .sm)
                            Text(choices.title)
                            Spacer()
                            Image(.chevronDown, size: .sm)
                                .rotationEffect(.degrees(listed.contains(id) ? 180 : 0))
                        }
                        .frame(minHeight: 32)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
                .listRowBackground(surface.color(.box))
            }
        }
    }

    private var options: some View {
        Section {
            if let model = store.composerModel, !model.efforts.isEmpty {
                let effort = store.composerEffort ?? ""
                NavigationLink {
                    ChoiceList(
                        title: "Reasoning", options: model.effortChoices, chosen: effort, label: ComposerView.effortLabel,
                        symbol: ComposerView.effortSymbol
                    ) { store.setEffort($0) }
                    .glassBackButton()
                } label: {
                    LabeledContent("Reasoning") { SymbolValue(ComposerView.effortLabel(effort), symbol: ComposerView.effortSymbol(effort)) }
                }
            }
            NavigationLink {
                ChoiceList(title: "Access", options: Access.allCases, chosen: store.composerAccess, label: \.label, symbol: \.symbol) {
                    store.setAccess($0)
                }
                .glassBackButton()
            } label: {
                LabeledContent("Access") { SymbolValue(store.composerAccess.label, symbol: store.composerAccess.symbol) }
            }
        } header: {
            Text("Options")
        }
        .listRowBackground(surface.color(.box))
    }

    /// Where the thread works: the server, the folder or a worktree of its own, and the branch
    /// checked out there. A thread that starts in a new worktree picks the branch it starts from.
    private func workspace(_ project: Project) -> some View {
        Section {
            let copies = store.draftProjectCopies
            if !copies.isEmpty {
                Picker("Server", selection: Binding(get: { project.id }, set: { store.setNewThreadProject($0) })) {
                    ForEach(copies) { copy in
                        Text(store.server(copy.serverID)?.name ?? "").tag(copy.id)
                    }
                }
                .tint(Color.themePrimary)
            } else if let server = store.server(project.serverID) {
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
                .tint(Color.themePrimary)
            } else if store.selectedThread == nil, project.branch != nil, project.git?.hasCommit == false {
                LabeledContent("Works in", value: "Current checkout")
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
                            Image(.chevronRight, size: .sm)
                                .foregroundStyle(Color.themeMutedStrongerForeground)
                        }
                        .foregroundStyle(Color.themeForeground)
                    }
                } else {
                    LabeledContent(title, value: value)
                }
            }
        } header: {
            Text("Workspace")
        } footer: {
            if store.selectedThread == nil, let branch = project.branch, project.git?.hasCommit == false {
                Text("A worktree can start once \(branch) has a commit.")
                    .foregroundStyle(Color.themeMutedForeground)
            }
        }
        .listRowBackground(surface.color(.box))
    }
}

/// A setting's options on a page of their own, drawn like the sheet's list. Picking one goes back.
/// Words with their symbol before them, as a row's value or a choice.
private struct SymbolValue: View {
    let title: String
    let symbol: Symbol

    init(_ title: String, symbol: Symbol) {
        self.title = title
        self.symbol = symbol
    }

    var body: some View {
        HStack(spacing: 8) {
            Image(symbol, size: .md)
            Text(title)
        }
    }
}

private struct ChoiceList<Option: Hashable>: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.surface) private var surface
    let title: String
    let options: [Option]
    let chosen: Option
    let label: (Option) -> String
    let symbol: (Option) -> Symbol
    let choose: (Option) -> Void

    var body: some View {
        List {
            Section {
                ForEach(options, id: \.self) { option in
                    Button {
                        choose(option)
                        dismiss()
                    } label: {
                        HStack {
                            SymbolValue(label(option), symbol: symbol(option))
                                .foregroundStyle(Color.themeForeground)
                            Spacer()
                            if option == chosen {
                                Image(.check, size: .md)
                                    .foregroundStyle(Color.themeForeground)
                            }
                        }
                    }
                }
            }
            .listRowBackground(surface.color(.box))
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
        .background(Color.themeBackground.ignoresSafeArea())
        .navigationTitle(title)
        .navigationBarTitleDisplayMode(.inline)
    }
}
#endif
