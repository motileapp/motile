import SwiftUI

/// An account the settings add or change, on its server.
struct EditedAccount: Identifiable {
    let server: Server
    let account: AgentAccount

    var id: String { "\(server.id)/\(account.id)" }
}

/// An account of an agent to add or change: its name, the folder its sign-in is kept in, and the
/// variables its agent is given, with the command that signs it in.
struct AgentAccountSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    let server: Server
    @State private var account: AgentAccount
    @State private var variables: [EditedVariable]
    @State private var saving = false
    @State private var suggestedFolder = ""
    @State private var copied = false
    @State private var copies = 0

    init(server: Server, account: AgentAccount) {
        self.server = server
        _account = State(initialValue: account)
        _variables = State(initialValue: account.variables.map { EditedVariable(variable: $0) })
    }

    private var installed: [Agent] { Agent.allCases.filter { server.agents[$0] != nil } }
    private var isNew: Bool { account.id.isEmpty }
    private var title: String {
        #if os(macOS)
        isNew ? "New Account on \(server.name)" : "\(account.agent.name) Account on \(server.name)"
        #else
        isNew ? "New Account" : "\(account.agent.name) Account"
        #endif
    }
    private var canSave: Bool { !account.name.trimmingCharacters(in: .whitespaces).isEmpty }

    var body: some View {
        content
            .onChange(of: account.name) { suggestFolder() }
            .onChange(of: account.agent) { suggestFolder() }
    }

    @ViewBuilder private var content: some View {
        #if os(macOS)
        VStack(alignment: .leading, spacing: 14) {
            Text(title)
                .font(.ui(size: 13, weight: .semibold))
            form
            HStack {
                Spacer()
                ActionButton("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                ActionButton("Save", variant: .primary, pending: saving) { save() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(!canSave)
            }
        }
        .padding(16)
        .frame(width: 460)
        #else
        NavigationStack {
            SettingsList { listForm }
                .scrollDismissesKeyboard(.interactively)
                .navigationTitle(title)
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { SheetCloseButton() }
                    ToolbarItem(placement: .confirmationAction) {
                        if saving {
                            Spinner(size: 17)
                        } else {
                            Button("Save") { save() }
                                .disabled(!canSave)
                        }
                    }
                }
        }
        .presentationDragIndicator(.visible)
        .interactiveDismissDisabled(saving)
        #endif
    }

    #if os(iOS)
    @ViewBuilder private var listForm: some View {
        if isNew && installed.count > 1 {
            Section {
                Picker("Agent", selection: $account.agent) {
                    ForEach(installed, id: \.self) { Text($0.name).tag($0) }
                }
                .tint(Color.themeForeground)
            }
        }
        Section("Name") {
            TextField("Personal", text: $account.name)
        }
        if !account.isDefault {
            Section {
                TextField(account.agent == .claude ? "~/.claude-personal" : "~/.codex-personal", text: $account.folder)
                    .font(.system(.body, design: .monospaced))
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
            } header: {
                Text("Folder")
            } footer: {
                Text("Where its sign-in is kept, as \(account.folderVariable)")
                    .foregroundStyle(Color.themeMutedForeground)
            }
            if account.agent == .codex {
                Section {
                    Toggle("Share sessions with the default account", isOn: $account.sharesSessions)
                } footer: {
                    Text("The folder keeps only the sign-in. Threads move between the two and go on where they were.")
                        .foregroundStyle(Color.themeMutedForeground)
                }
            }
        }
        Section {
            ForEach($variables) { $edited in
                listVariableRow($edited)
            }
            .onDelete { variables.remove(atOffsets: $0) }
            Button {
                variables.append(EditedVariable(variable: AgentAccount.Variable(name: "", value: "", sensitive: true)))
            } label: {
                SettingsActionLabel("Add a Variable", symbol: .plus)
            }
        } header: {
            Text("Variables")
        } footer: {
            Text("Given to its agent: an API key or a router. A sensitive value stays on \(server.name) and is never shown again.")
                .foregroundStyle(Color.themeMutedForeground)
        }
        Section {
            Text(account.signInCommand)
                .font(.system(.callout, design: .monospaced))
                .foregroundStyle(Color.themeForeground)
                .textSelection(.enabled)
            Button {
                Platform.copy(account.signInCommand)
                copied = true
                copies += 1
            } label: {
                SettingsActionLabel(copied ? "Copied" : "Copy Command", symbol: copied ? .check : .copy)
            }
            .task(id: copies) {
                guard copied, (try? await Task.sleep(for: .seconds(1.2))) != nil else { return }
                copied = false
            }
            ShareLink(item: account.signInCommand) {
                SettingsActionLabel("Share Command", symbol: .share)
            }
        } header: {
            Text("Sign In")
        } footer: {
            Text("Run it in a terminal on \(server.name). Who is signed in shows in the list once the agent says.")
                .foregroundStyle(Color.themeMutedForeground)
        }
    }

    /// A variable's name and value, with whether the value is sensitive. A swipe takes it out.
    private func listVariableRow(_ edited: Binding<EditedVariable>) -> some View {
        let variable = edited.wrappedValue.variable
        let kept = variable.sensitive && account.variables.contains { $0.name == variable.name && $0.sensitive }
        return HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 4) {
                TextField("NAME", text: edited.variable.name)
                    .font(.system(.footnote, design: .monospaced, weight: .medium))
                    .foregroundStyle(Color.themeMutedForeground)
                Group {
                    if variable.sensitive {
                        SecureField(kept ? "••••••••" : "value", text: edited.variable.value)
                    } else {
                        TextField("value", text: edited.variable.value)
                    }
                }
                .font(.system(.subheadline, design: .monospaced))
            }
            ActionButton(
                icon: variable.sensitive ? .eyeOff : .eye, help: variable.sensitive ? "Sensitive: hidden and kept on \(server.name)" : "Shown: mark it sensitive",
                selected: variable.sensitive
            ) {
                edited.wrappedValue.variable.sensitive.toggle()
            }
        }
        .padding(.vertical, 2)
        .textInputAutocapitalization(.never)
        .autocorrectionDisabled()
    }
    #endif

    #if os(macOS)

    private var form: some View {
        VStack(alignment: .leading, spacing: Platform.scale > 1 ? 28 : 14) {
            if isNew && installed.count > 1 {
                Segmented(installed.map { ($0.name, $0) }, selection: $account.agent)
            }
            field("Name") {
                InputField("Personal", text: $account.name)
            }
            if !account.isDefault {
                field("Folder", caption: "Where its sign-in is kept, as \(account.folderVariable)") {
                    InputField(account.agent == .claude ? "~/.claude-personal" : "~/.codex-personal", text: $account.folder, monospaced: true)
                }
                if account.agent == .codex {
                    HStack(spacing: 10) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Share sessions with the default account")
                                .font(.ui(size: 13, weight: .medium))
                            Text("The folder keeps only the sign-in. Threads move between the two and go on where they were.")
                                .font(.ui(size: 11.5))
                                .foregroundStyle(Color.themeMutedForeground)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                        .padding(.horizontal, 4)
                        Spacer(minLength: 12)
                        Switch(isOn: $account.sharesSessions)
                    }
                }
            }
            field("Variables", caption: "Given to its agent: an API key or a router. A sensitive value stays on \(server.name) and is never shown again.") {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach($variables) { $edited in
                        variableRow($edited)
                    }
                    ActionButton("Add a Variable", icon: .plus, size: .small) {
                        variables.append(EditedVariable(variable: AgentAccount.Variable(name: "", value: "", sensitive: true)))
                    }
                }
            }
            signIn
        }
    }

    /// How the account is signed in: on its server, with its folder.
    private var signIn: some View {
        field("Sign In", caption: "Run it in a terminal on \(server.name). Who is signed in shows in the list once the agent says.") {
            CommandBox(command: account.signInCommand)
        }
    }

    /// A variable's name and value, with whether the value is sensitive and the way to take it out.
    private func variableRow(_ edited: Binding<EditedVariable>) -> some View {
        let variable = edited.wrappedValue.variable
        let kept = variable.sensitive && account.variables.contains { $0.name == variable.name && $0.sensitive }
        return HStack(spacing: 6) {
            InputField("NAME", text: edited.variable.name, size: .small, monospaced: true)
                .frame(width: 150)
            InputField(kept ? "••••••••" : "value", text: edited.variable.value, size: .small, monospaced: true, secure: variable.sensitive)
            ActionButton(
                icon: variable.sensitive ? .eyeOff : .eye, help: variable.sensitive ? "Sensitive: hidden and kept on \(server.name)" : "Shown: mark it sensitive",
                size: .small, selected: variable.sensitive
            ) {
                edited.wrappedValue.variable.sensitive.toggle()
            }
            ActionButton(icon: .x, help: "Remove the variable", size: .small) {
                variables.removeAll { $0.id == edited.wrappedValue.id }
            }
        }
    }

    private func field<Content: View>(_ title: String, caption: String? = nil, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(.ui(size: 12, weight: .medium))
                .padding(.horizontal, 4)
            content()
            if let caption {
                Text(caption)
                    .font(.ui(size: 11.5))
                    .foregroundStyle(Color.themeMutedStrongerForeground)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 4)
            }
        }
    }

    #endif

    /// Fills a new account's folder from its name until the folder is typed by hand.
    private func suggestFolder() {
        guard isNew, account.folder.isEmpty || account.folder == suggestedFolder else { return }
        suggestedFolder = account.suggestedFolder(besides: server.agentAccounts)
        account.folder = suggestedFolder
    }

    private func save() {
        var saved = account
        saved.variables = variables.map(\.variable).filter { !$0.name.trimmingCharacters(in: .whitespaces).isEmpty }
        if saved.agent != .codex { saved.sharesSessions = false }
        saving = true
        store.saveAgentAccount(saved, on: server) { kept in
            saving = false
            if kept { dismiss() }
        }
    }
}

/// A variable as the account's sheet edits it.
private struct EditedVariable: Identifiable {
    let id = UUID()
    var variable: AgentAccount.Variable
}
