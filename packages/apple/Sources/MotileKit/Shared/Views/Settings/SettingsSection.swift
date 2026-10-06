import Foundation

/// A page of the settings, listed in the settings' sidebar.
enum SettingsSection: String, CaseIterable, Identifiable, Hashable {
    case general, servers, projects, textGeneration, pullRequests, usage

    var id: String { rawValue }

    var title: String {
        switch self {
        case .general: "General"
        case .servers: "Servers"
        case .projects: "Projects"
        case .textGeneration: "Text generation"
        case .pullRequests: "Pull requests"
        case .usage: "Usage"
        }
    }

    var symbol: Symbol {
        switch self {
        case .general: .slidersHorizontal
        case .servers: .server
        case .projects: .folder
        case .textGeneration: .pencilLine
        case .pullRequests: .gitPullRequest
        case .usage: .chartColumn
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
        SettingsEntry(id: "servers", title: "Servers", section: .servers, keywords: "add remove machine agents connected"),
        SettingsEntry(id: "continue-limits", title: "Continue after usage limits", section: .servers, keywords: "rate limit reset resume quota wait"),
        SettingsEntry(id: "continue-restarts", title: "Continue after restarts", section: .servers, keywords: "restart update crash resume interrupted"),
        SettingsEntry(id: "projects", title: "Projects", section: .projects, keywords: "add remove folder icon setup worktree script"),
        SettingsEntry(id: "text-model", title: "Model", section: .textGeneration, keywords: "titles branch names commit messages pull requests writer"),
        SettingsEntry(id: "branch-names", title: "Branch names", section: .textGeneration, keywords: "instructions prefix naming"),
        SettingsEntry(id: "merged", title: "Mark the thread done after merge or close", section: .pullRequests, keywords: "merge close finished"),
        SettingsEntry(id: "worktrees", title: "Remove the thread's worktree after merge", section: .pullRequests, keywords: "merge pushed branch clean up"),
        SettingsEntry(id: "usage", title: "Tokens and cost", section: .usage, keywords: "spent price models chart"),
    ]

    /// The groups every word of the query is found in, by title, section or keywords.
    static func matching(_ query: String) -> [SettingsEntry] {
        let words = query.split(whereSeparator: \.isWhitespace).map(String.init)
        guard !words.isEmpty else { return [] }
        return all.filter { entry in
            let text = "\(entry.title) \(entry.section.title) \(entry.keywords)"
            return words.allSatisfy { text.localizedCaseInsensitiveContains($0) }
        }
    }
}
