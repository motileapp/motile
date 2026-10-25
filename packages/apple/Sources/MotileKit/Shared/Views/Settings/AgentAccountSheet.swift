import SwiftUI

/// An account the settings add or change, on its server.
struct EditedAccount: Identifiable {
    let server: Server
    let account: AgentAccount

    var id: String { "\(server.id)/\(account.id)" }
}

/// An account of an agent to add or change: its name, the folder its sign-in is kept in, and the
/// variables its agent is given, with the way to sign it in.
struct AgentAccountSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.closeModal) private var close
    let server: Server
    private let opened: AgentAccount
    @State private var account: AgentAccount
    @State private var signIn = AccountSignIn()
    /// The account as signing in saved it, until something else changes.
    @State private var savedBySignIn: AgentAccount?
    @State private var variables: [EditedVariable]
    @State private var saving = false
    @State private var suggestedFolder = ""
    @State private var copied = false
    @State private var copies = 0
    @FocusState private var codeFocused: Bool

    init(server: Server, account: AgentAccount) {
        self.server = server
        opened = account
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
    /// Signing in saved everything the form has, so there is nothing left to save or to throw away.
    private var savedAsIs: Bool { savedBySignIn == edited }

    /// The account as the form has it.
    private var edited: AgentAccount {
        var edited = account
        edited.variables = variables.map(\.variable).filter { !$0.name.trimmingCharacters(in: .whitespaces).isEmpty }
        if edited.agent != .codex { edited.sharesSessions = false }
        return edited
    }

    /// Claude Code signs in here; Codex, and a server too old for it, with a command.
    private var signsInHere: Bool { account.agent == .claude && server.signsInAccounts }
    private var folderCaption: String {
        "Where its sign-in is kept, as \(account.folderVariable). Empty for \(account.usualFolder)."
    }
    private var signingIn: Bool {
        if case .waiting = signIn.step { return true }
        return false
    }

    /// Who is signed in to the folder the form has, as far as is known.
    private var signedInAs: String? {
        if case .signedIn(let who) = signIn.step { return who }
        guard account.folder == opened.folder, let email = opened.email else { return nil }
        return [email, opened.plan].compactMap { $0 }.joined(separator: " · ")
    }

    private var signInCaption: String {
        if signingIn { return "Sign in on the page, then paste the code it shows." }
        if savedAsIs { return "Signing in saved the account." }
        return "Opens Claude's sign-in page. Signing in saves the account on \(server.name)."
    }

    var body: some View {
        content
            .onChange(of: account.name) { suggestFolder() }
            .onChange(of: account.agent) {
                signIn.cancel(in: store)
                suggestFolder()
            }
            .onChange(of: account.folder) { signIn.cancel(in: store) }
            .onDisappear { signIn.cancel(in: store) }
    }

    private func startSignIn() {
        signIn.start(edited, on: server, in: store)
    }

    private func finishSignIn() {
        signIn.finish(edited, in: store) { id in
            account.id = id
            savedBySignIn = edited
        }
    }

    @ViewBuilder private var content: some View {
        #if os(macOS)
        VStack(alignment: .leading, spacing: 14) {
            Text(title)
                .font(.ui(size: 15, weight: .semibold))
            form
            HStack {
                Spacer()
                if savedAsIs {
                    ActionButton("Done", variant: .primary) { close() }
                        .keyboardShortcut(.defaultAction)
                } else {
                    ActionButton("Cancel") { close() }
                        .keyboardShortcut(.cancelAction)
                    ActionButton("Save", variant: .primary, pending: saving) { save() }
                        .keyboardShortcut(.defaultAction)
                        .disabled(!canSave)
                }
            }
        }
        .padding(20)
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
                        } else if savedAsIs {
                            Button("Done") { close() }
                        } else {
                            Button("Save") { save() }
                                .disabled(!canSave)
                        }
                    }
                }
        }
        .presentationDragIndicator(.visible)
        .interactiveDismissDisabled(saving || signIn.finishing)
        #endif
    }

    #if os(iOS)
    @ViewBuilder private var listForm: some View {
        if isNew && installed.count > 1 {
            Section {
                Picker("Agent", selection: $account.agent) {
                    ForEach(installed, id: \.self) { Text($0.name).tag($0) }
                }
            }
        }
        Section("Name") {
            TextField("Personal", text: $account.name)
        }
        Section {
            TextField(account.usualFolder, text: $account.folder)
                .font(.system(.body, design: .monospaced))
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
        } header: {
            Text("Folder")
        } footer: {
            Text(folderCaption)
                .foregroundStyle(Color.themeMutedForeground)
        }
        if account.agent == .codex {
            Section {
                Toggle("Share sessions with \(account.usualFolder)", isOn: $account.sharesSessions)
            } footer: {
                Text("The folder keeps only the sign-in. Threads move between the two and go on where they were.")
                    .foregroundStyle(Color.themeMutedForeground)
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
        if signsInHere {
            listSignIn
        } else {
            listSignInCommand
        }
    }

    @ViewBuilder private var listSignIn: some View {
        @Bindable var signIn = signIn
        Section {
            switch signIn.step {
            case .waiting(_, let url):
                Text(url.absoluteString)
                    .font(.system(.callout, design: .monospaced))
                    .foregroundStyle(Color.themeMutedForeground)
                    .lineLimit(1)
                Link(destination: url) {
                    SettingsActionLabel("Open the Sign-In Page", symbol: .squareArrowOutUpRight)
                }
                Button {
                    Platform.copy(url.absoluteString)
                    copied = true
                    copies += 1
                } label: {
                    SettingsActionLabel(copied ? "Copied" : "Copy the Link", symbol: copied ? .check : .copy)
                }
                .task(id: copies) {
                    guard copied, (try? await Task.sleep(for: .seconds(1.2))) != nil else { return }
                    copied = false
                }
                HStack {
                    TextField("Paste the code", text: $signIn.code)
                        .font(.system(.body, design: .monospaced))
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .submitLabel(.done)
                        .onSubmit(finishSignIn)
                    if signIn.finishing {
                        Spinner(size: 17)
                    } else {
                        Button("Sign In", action: finishSignIn)
                            .disabled(!signIn.canFinish || !canSave)
                    }
                }
            default:
                if let who = signedInAs {
                    Text("Signed in as \(who)")
                        .foregroundStyle(Color.themeForeground)
                }
                Button(action: startSignIn) {
                    HStack {
                        SettingsActionLabel(signedInAs == nil ? "Sign In" : "Sign In Again", symbol: .circleUser)
                        Spacer()
                        if signIn.step == .starting { Spinner(size: 17) }
                    }
                }
                .disabled(signIn.step == .starting)
            }
        } header: {
            Text("Sign In")
        } footer: {
            Text(signIn.error ?? signInCaption)
                .foregroundStyle(signIn.error == nil ? Color.themeMutedForeground : Color.themeDestructive)
        }
    }

    @ViewBuilder private var listSignInCommand: some View {
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
                ZStack(alignment: .leading) {
                    SecureField(kept ? "••••••••" : "value", text: edited.variable.value)
                        .shown(variable.sensitive)
                    TextField("value", text: edited.variable.value)
                        .shown(!variable.sensitive)
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
            field("Folder", caption: folderCaption) {
                InputField(account.usualFolder, text: $account.folder, monospaced: true)
            }
            if account.agent == .codex {
                HStack(spacing: 10) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Share sessions with \(account.usualFolder)")
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
            signInField
        }
    }

    /// How the account is signed in: on its server, with its folder.
    @ViewBuilder private var signInField: some View {
        if signsInHere {
            field("Sign In", caption: signInCaption, failure: signIn.error) { signInSteps }
        } else {
            field("Sign In", caption: "Run it in a terminal on \(server.name). Who is signed in shows in the list once the agent says.") {
                CommandBox(command: account.signInCommand)
            }
        }
    }

    @ViewBuilder private var signInSteps: some View {
        @Bindable var signIn = signIn
        switch signIn.step {
        case .waiting(_, let url):
            VStack(alignment: .leading, spacing: 6) {
                CommandBox(command: url.absoluteString, link: url)
                HStack(spacing: 6) {
                    InputField("Paste the code", text: $signIn.code, monospaced: true, focus: $codeFocused)
                        .onSubmit(finishSignIn)
                        .onAppear { codeFocused = true }
                    ActionButton("Sign In", variant: .primary, pending: signIn.finishing, action: finishSignIn)
                        .disabled(!signIn.canFinish || !canSave)
                }
            }
        default:
            HStack(spacing: 10) {
                Text(signedInAs.map { "Signed in as \($0)" } ?? "Not signed in")
                    .font(.ui(size: 13, weight: .medium))
                    .foregroundStyle(signedInAs == nil ? Color.themeMutedForeground : Color.themeForeground)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .padding(.horizontal, 4)
                Spacer(minLength: 12)
                ActionButton(signedInAs == nil ? "Sign In" : "Sign In Again", pending: signIn.step == .starting, action: startSignIn)
            }
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

    private func field<Content: View>(
        _ title: String, caption: String? = nil, failure: String? = nil, @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(.ui(size: 12, weight: .medium))
                .padding(.horizontal, 4)
            content()
            if let caption = failure ?? caption {
                Text(caption)
                    .font(.ui(size: 11.5))
                    .foregroundStyle(failure == nil ? Color.themeMutedStrongerForeground : Color.themeDestructive)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 4)
            }
        }
    }

    #endif

    /// Fills a new account's folder from its name until the folder is typed by hand.
    private func suggestFolder() {
        guard isNew, signIn.step == .idle, account.folder.isEmpty || account.folder == suggestedFolder else { return }
        suggestedFolder = account.suggestedFolder(besides: server.agentAccounts)
        account.folder = suggestedFolder
    }

    private func save() {
        saving = true
        store.saveAgentAccount(edited, on: server) { kept in
            saving = false
            if kept { close() }
        }
    }
}

/// A variable as the account's sheet edits it.
private struct EditedVariable: Identifiable {
    let id = UUID()
    var variable: AgentAccount.Variable
}

#if os(iOS)
extension View {
    /// Keeps its place in the layout while hidden, so that swapping it for another doesn't move what is around it.
    fileprivate func shown(_ shown: Bool) -> some View {
        opacity(shown ? 1 : 0)
            .allowsHitTesting(shown)
            .accessibilityHidden(!shown)
    }
}
#endif
