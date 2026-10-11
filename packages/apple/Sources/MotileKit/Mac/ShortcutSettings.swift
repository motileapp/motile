#if os(macOS)
import SwiftUI

/// The keyboard shortcuts' page of the settings: every command under its group with its keys, to
/// record new ones, put a condition on, add more of, or give back their defaults.
struct ShortcutSettings: View {
    @Environment(AppStore.self) private var store
    @State private var query = ""
    /// The shortcut being added, with the command it is for once one is chosen.
    @State private var adding: String??

    var body: some View {
        let shortcuts = store.shortcuts
        SettingsGroup(
            "shortcuts", "Keyboard Shortcuts",
            caption: "Click a shortcut to record new keys. Where two take the same keys, the one added last wins.", carded: false
        ) {
            HStack(spacing: 8) {
                InputField("Search shortcuts", text: $query, icon: .search, size: .large, clearable: true)
                ActionButton("Add Shortcut", icon: .plus, variant: .outline, size: .large) { adding = .some(nil) }
                    .disabled(adding != nil)
                ActionButton(icon: .fileBraces, help: "Open keybindings.json", variant: .outline, size: .large) { store.openShortcutsFile() }
            }
        }
        if !shortcuts.issues.isEmpty {
            issues(shortcuts.issues)
        }
        if let adding {
            SettingsGroup("new-shortcut", "New shortcut") {
                NewShortcutRow(command: adding) { self.adding = nil }
                    .id(adding ?? "")
            }
        }
        let groups = found(in: shortcuts.groups)
        ForEach(groups) { group in
            SettingsGroup("shortcuts-\(group.title)", group.title) {
                ForEach(group.rows) { row in
                    ShortcutRowView(row: row) { adding = .some($0) }
                        .id(row.id == group.rows.first(where: { $0.command == row.command })?.id ? "shortcut-\(row.command)" : row.id)
                    if row.id != group.rows.last?.id { ThemeDivider() }
                }
            }
        }
        if groups.isEmpty {
            SettingsNote("No shortcuts found")
        }
    }

    /// The rows every word of the search is found in: by name, command, keys or condition.
    private func found(in groups: [ShortcutGroup]) -> [ShortcutGroup] {
        let words = query.split(whereSeparator: \.isWhitespace).map(String.init)
        guard !words.isEmpty else { return groups }
        return groups.compactMap { group in
            let rows = group.rows.filter { row in
                let text = "\(row.label) \(row.command) \(row.caps.joined()) \(row.key) \(row.when) \(group.title)"
                return words.allSatisfy { text.localizedCaseInsensitiveContains($0) }
            }
            return rows.isEmpty ? nil : ShortcutGroup(title: group.title, rows: rows)
        }
    }

    private func issues(_ issues: [String]) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Image(.triangleAlert, size: 13)
                    .foregroundStyle(Color.themeWarning)
                Text("Some of keybindings.json is skipped")
                    .font(.ui(size: 13, weight: .medium))
                Spacer(minLength: 12)
                ActionButton("Open", variant: .outline, size: .large) { store.openShortcutsFile() }
                    .padding(.trailing, -rowOutset(for: ControlSize.large.height))
            }
            ForEach(issues, id: \.self) { issue in
                Text(issue)
                    .font(.ui(size: 11.5))
                    .foregroundStyle(Color.themeMutedForeground)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.horizontal, settingsInset)
        .padding(.vertical, scaled(12))
        .card()
    }
}

/// A command and one of its keys, or a command that has none yet.
private struct ShortcutRowView: View {
    @Environment(AppStore.self) private var store
    let row: ShortcutRow
    /// Starts another shortcut for the command.
    let addAnother: (String) -> Void
    @State private var editingCondition = false

    var body: some View {
        SettingsRow {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(row.label)
                        .font(.ui(size: 13, weight: .medium))
                        .help(row.command)
                    if row.custom {
                        Chip("Custom")
                    }
                }
                if !row.when.isEmpty {
                    ConditionButton(when: row.when) { editingCondition = true }
                }
            }
            .popover(isPresented: $editingCondition, arrowEdge: .bottom) {
                ConditionEditor(key: row.key, row: row, when: row.when) { when in
                    store.setShortcut(row.command, key: row.key, when: when, replacing: row)
                    editingCondition = false
                } cancel: {
                    editingCondition = false
                }
            }
        } trailing: {
            ConflictMark(conflicts: row.conflicts)
            ShortcutField(row.caps) { key in
                store.setShortcut(row.command, key: key, when: row.when, replacing: row)
            }
            ActionMenu(icon: .ellipsis, help: "What to do with the shortcut", size: .large) {
                if !row.key.isEmpty {
                    menuItem(row.when.isEmpty ? "Add a Condition…" : "Edit the Condition…", symbol: .pencil) { editingCondition = true }
                    menuItem("Add Another Shortcut", symbol: .plus) { addAnother(row.command) }
                }
                if row.resettable {
                    menuItem("Reset to Default", symbol: .undo2) { store.resetShortcut(row.command) }
                }
                if !row.key.isEmpty {
                    Divider()
                    menuItem("Remove", symbol: .trash2, role: .destructive) { store.removeShortcut(row) }
                }
            }
            .disabled(row.key.isEmpty && !row.resettable)
        }
    }
}

/// A shortcut being added: its command, its keys and its condition, kept once all are right.
private struct NewShortcutRow: View {
    @Environment(AppStore.self) private var store
    @State var command: String?
    let done: () -> Void
    @State private var key = ""
    @State private var when = ""
    @State private var check = ShortcutCheck()
    @State private var editingCondition = false

    private var label: String? {
        store.shortcuts.commands.first { $0.id == command }?.label
    }

    var body: some View {
        SettingsRow {
            VStack(alignment: .leading, spacing: 2) {
                ActionMenu(label ?? "Choose a Command", variant: .ghost, size: .large, tint: label == nil ? nil : .themeForeground) {
                    ForEach(store.shortcuts.groups) { group in
                        Section(group.title) {
                            ForEach(commands(of: group), id: \.id) { choice in
                                Button(choice.label) { command = choice.id }
                            }
                        }
                    }
                }
                .padding(.leading, -ControlSize.large.padding)
                ConditionButton(when: when) { editingCondition = true }
            }
            .popover(isPresented: $editingCondition, arrowEdge: .bottom) {
                ConditionEditor(key: key, row: nil, when: when) { changed in
                    when = changed
                    editingCondition = false
                } cancel: {
                    editingCondition = false
                }
            }
        } trailing: {
            ConflictMark(conflicts: check.conflicts)
            ShortcutField(check.caps, placeholder: "Record Keys") { key = $0 }
            ActionButton("Add", variant: .primary, size: .large) {
                guard let command else { return }
                store.setShortcut(command, key: key, when: when)
                done()
            }
            .disabled(command == nil || key.isEmpty || check.whenError != nil)
            ActionButton(icon: .x, help: "Cancel", size: .large, action: done)
        }
        .task(id: "\(key)\u{0}\(when)") {
            guard !key.isEmpty else { return }
            store.checkShortcut(key: key, when: when, row: nil) { check = $0 }
        }
    }

    private func commands(of group: ShortcutGroup) -> [(id: String, label: String)] {
        var seen: Set<String> = []
        return group.rows.compactMap { row in
            seen.insert(row.command).inserted ? (row.command, row.label) : nil
        }
    }
}

/// The condition under a shortcut's name, which opens it to edit.
private struct ConditionButton: View {
    let when: String
    let edit: () -> Void

    var body: some View {
        ActionButton(when.isEmpty ? "Always" : "When \(when)", variant: .ghost, size: .small, opens: true, action: edit)
            .truncationMode(.tail)
            .padding(.leading, -ControlSize.small.padding)
            .help(when.isEmpty ? "Runs wherever the keys are pressed. Click to add a condition." : "Runs only while this holds. Click to edit it.")
    }
}

/// Says which other commands take the same keys.
private struct ConflictMark: View {
    let conflicts: [String]

    var body: some View {
        if !conflicts.isEmpty {
            Image(.triangleAlert, size: 13)
                .foregroundStyle(Color.themeWarning)
                .help("Also on \(conflicts.joined(separator: ", ")). Where both can run, the one added last wins.")
        }
    }
}

/// Where a shortcut runs: names joined by ! for not, && for and, || for or, and parentheses.
private struct ConditionEditor: View {
    @Environment(AppStore.self) private var store
    /// The keys it is checked with, for the commands they would clash with there.
    let key: String
    let row: ShortcutRow?
    @State var when: String
    let save: (String) -> Void
    let cancel: () -> Void
    @State private var check = ShortcutCheck()
    @FocusState private var typing: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                Text("When")
                    .font(.ui(size: 13, weight: .semibold))
                Text("The shortcut runs only while this holds. Join names with ! for not, && for and, || for or.")
                    .font(.ui(size: 11.5))
                    .foregroundStyle(Color.themeMutedForeground)
                    .fixedSize(horizontal: false, vertical: true)
            }
            InputField("Always", text: $when, size: .large, monospaced: true, focus: $typing)
            if let error = check.whenError {
                note(error, symbol: .circleX, color: .themeDestructive)
            } else if !check.unknown.isEmpty {
                note("Motile doesn't know \(check.unknown.joined(separator: ", ")), which never holds.", symbol: .triangleAlert, color: .themeWarning)
            }
            HStack(spacing: 8) {
                ActionMenu("Add", icon: .plus, variant: .outline) {
                    Section("While") {
                        ForEach(store.shortcuts.conditions, id: \.self) { name in
                            Button(name) { add(name) }
                        }
                    }
                    Section("Unless") {
                        ForEach(store.shortcuts.conditions, id: \.self) { name in
                            Button("!\(name)") { add("!\(name)") }
                        }
                    }
                }
                Spacer()
                ActionButton("Cancel", action: cancel)
                    .keyboardShortcut(.cancelAction)
                ActionButton("Save", variant: .primary) { save(when.trimmingCharacters(in: .whitespaces)) }
                    .keyboardShortcut(.defaultAction)
                    .disabled(check.whenError != nil)
            }
        }
        .padding(16)
        .frame(width: 420)
        .onAppear { typing = true }
        .task(id: when) {
            store.checkShortcut(key: key, when: when, row: row) { check = $0 }
        }
    }

    private func add(_ name: String) {
        let written = when.trimmingCharacters(in: .whitespaces)
        when = written.isEmpty ? name : "\(written) && \(name)"
    }

    private func note(_ text: String, symbol: Symbol, color: Color) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Image(symbol, size: 12)
            Text(text)
                .fixedSize(horizontal: false, vertical: true)
        }
        .font(.ui(size: 11.5))
        .foregroundStyle(color)
    }
}

private func menuItem(_ title: String, symbol: Symbol, role: ButtonRole? = nil, action: @escaping () -> Void) -> some View {
    Button(role: role, action: action) {
        Label {
            Text(title)
        } icon: {
            Image(platform: .symbol(symbol, size: 13))
        }
    }
}
#endif
