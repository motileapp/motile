import Foundation

/// An issue tracker whose tab the right panel offers once the settings turn it on.
enum Integration: String, CaseIterable, Identifiable {
    case linear, ricrac

    var id: String { rawValue }

    var title: String {
        switch self {
        case .linear: "Linear"
        case .ricrac: "Ricrac"
        }
    }

    var summary: String {
        switch self {
        case .linear: "Browse, file and change Linear issues, and hand them to an agent."
        case .ricrac: "Browse, file and change Ricrac issues, and hand them to an agent."
        }
    }

    var symbol: Symbol {
        switch self {
        case .linear: .linear
        case .ricrac: .ricrac
        }
    }

    /// The shortcut's command that shows its tab.
    var command: String { "rightPanel.\(rawValue)" }

    var defaultsKey: String { "integration.\(rawValue)" }

    var onByDefault: Bool { self == .linear }

    /// The integrations the settings have on.
    static func kept(in defaults: UserDefaults) -> Set<Integration> {
        Set(allCases.filter { defaults.object(forKey: $0.defaultsKey) as? Bool ?? $0.onByDefault })
    }
}

extension PanelTab {
    /// The integration the tab is of, which hides it while it is off.
    var integration: Integration? {
        switch self {
        case .linear, .linearIssue: .linear
        case .ricrac, .ricracIssue: .ricrac
        default: nil
        }
    }
}
