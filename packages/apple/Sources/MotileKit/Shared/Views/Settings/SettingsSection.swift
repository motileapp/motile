import Foundation

/// A page of the settings, listed in the settings' sidebar.
enum SettingsSection: String, CaseIterable, Identifiable, Hashable {
    case general, keyboard, servers, agents, projects, textGeneration, pullRequests

    var id: String { rawValue }

    /// The sections this client has: the keyboard's only where there is one to set.
    static var shown: [SettingsSection] {
        Platform.name == "macos" ? allCases : allCases.filter { $0 != .keyboard }
    }

    var title: String {
        switch self {
        case .general: "General"
        case .keyboard: "Keyboard Shortcuts"
        case .servers: "Servers"
        case .agents: "Agents"
        case .projects: "Projects"
        case .textGeneration: "Text Generation"
        case .pullRequests: "Pull Requests"
        }
    }

    var symbol: Symbol {
        switch self {
        case .general: .slidersHorizontal
        case .keyboard: .keyboard
        case .servers: .server
        case .agents: .circleUser
        case .projects: .folder
        case .textGeneration: .pencilLine
        case .pullRequests: .gitPullRequest
        }
    }
}

/// One group of settings, as the settings' search finds it. Its `id` is the group's on its page.
struct SettingsEntry: Identifiable, Hashable {
    let id: String
    let title: String
    let section: SettingsSection
    /// Words the group is also found by, beyond its title and its section's.
    let keywords: String

    static let all: [SettingsEntry] = [
        SettingsEntry(id: "account", title: "Account", section: .general, keywords: "signed in email sign out"),
        SettingsEntry(id: "updates", title: Platform.name == "macos" ? "Updates" : "Version", section: .general, keywords: "version check for updates release"),
        SettingsEntry(id: "appearance", title: "Theme", section: .general, keywords: "appearance light dark system mode"),
        SettingsEntry(id: "messages", title: "Sent while the agent works", section: .general, keywords: "messages queue steer interrupt"),
        SettingsEntry(id: "storage", title: "Images and videos", section: .general, keywords: "storage cache clear media disk"),
        SettingsEntry(id: "shortcuts", title: "Keyboard shortcuts", section: .keyboard, keywords: "keys keybindings hotkeys commands record keybindings.json"),
        SettingsEntry(id: "servers", title: "Servers", section: .servers, keywords: "add remove machine agents connected"),
        SettingsEntry(id: "continue-limits", title: "Continue after usage limits", section: .servers, keywords: "rate limit reset resume quota wait"),
        SettingsEntry(id: "continue-restarts", title: "Continue after restarts", section: .servers, keywords: "restart update crash resume interrupted"),
        SettingsEntry(id: "agent-accounts", title: "Accounts", section: .agents, keywords: "claude codex sign in login folder subscription plan api key router"),
        SettingsEntry(id: "projects", title: "Projects", section: .projects, keywords: "add remove folder icon setup worktree script"),
        SettingsEntry(id: "text-model", title: "Model", section: .textGeneration, keywords: "titles branch names commit messages pull requests writer"),
        SettingsEntry(id: "branch-names", title: "Branch names", section: .textGeneration, keywords: "instructions prefix naming"),
        SettingsEntry(id: "merged", title: "Mark the thread done after merge or close", section: .pullRequests, keywords: "merge close finished"),
        SettingsEntry(id: "worktrees", title: "Remove the thread's worktree after merge", section: .pullRequests, keywords: "merge pushed branch clean up"),
    ]

    /// The groups every word of the query is found in, by title, section or keywords, and the
    /// shortcuts' commands it finds by name.
    static func matching(_ query: String, shortcuts: Shortcuts = Shortcuts()) -> [SettingsEntry] {
        let words = query.split(whereSeparator: \.isWhitespace).map(String.init)
        guard !words.isEmpty else { return [] }
        let commands = SettingsSection.shown.contains(.keyboard)
            ? shortcuts.commands.map { SettingsEntry(id: "shortcut-\($0.id)", title: $0.label, section: .keyboard, keywords: "shortcut keys") }
            : []
        return (all + commands).filter { entry in
            let text = "\(entry.title) \(entry.section.title) \(entry.keywords)"
            return SettingsSection.shown.contains(entry.section) && words.allSatisfy { text.localizedCaseInsensitiveContains($0) }
        }
    }
}
