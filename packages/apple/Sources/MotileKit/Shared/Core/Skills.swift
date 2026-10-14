import Foundation

/// The skills that fit the word being typed before the composer's cursor, as the core ranks them,
/// and the word they would replace.
struct SkillMenu: Equatable {
    let typed: String
    let rows: [SkillRow]

    init(json: JSON) {
        typed = json.string("typed")
        rows = json.objects("rows").map(SkillRow.init(json:))
    }
}

struct SkillRow: Equatable, Identifiable {
    let name: String
    let label: String
    let description: String
    /// Where it comes from, and the symbol that says so.
    let source: String
    let symbol: Symbol

    var id: String { name }

    init(json: JSON) {
        name = json.string("name")
        label = json.string("label")
        description = json.string("description")
        source = json.string("source")
        symbol = Symbol(lucide: json.string("symbol")) ?? .box
    }
}

/// A skill a text starts, where its `$name` is.
struct SkillSpan {
    let range: NSRange
    let name: String
    let label: String

    init(json: JSON) {
        range = NSRange(location: json.int("start"), length: json.int("len"))
        name = json.string("name")
        label = json.string("label")
    }
}

/// The skill the composer's text puts in place of the word typed before its cursor.
struct SkillPick: Equatable {
    let id: Int
    let name: String
    let label: String
}

/// What the composer's text does with skills: says what is typed before its cursor, gives the
/// menu its keys while it is open, puts in the skill picked, and asks which `$name`s are skills.
struct SkillTyping {
    var menuOpen: Bool
    var pick: SkillPick?
    var typing: (String) -> Void
    var key: (SkillMenuKey) -> Void
    var spans: (String, @escaping ([SkillSpan]) -> Void) -> Void
}

enum SkillMenuKey {
    case up, down, pick, dismiss
}

extension Symbol {
    /// The symbols the core names, by Lucide's names.
    init?(lucide name: String) {
        switch name {
        case "folder": self = .folder
        case "user-round": self = .userRound
        case "blocks": self = .blocks
        case "box": self = .box
        default: return nil
        }
    }
}

extension AppStore {
    var skillTyping: SkillTyping {
        SkillTyping(
            menuOpen: skillMenu != nil,
            pick: skillPick,
            typing: { [weak self] in self?.readSkillMenu(beforeCursor: $0) },
            key: { [weak self] key in
                switch key {
                case .up: self?.moveSkillHighlight(by: -1)
                case .down: self?.moveSkillHighlight(by: 1)
                case .pick: self?.pickSkill()
                case .dismiss: self?.dismissSkillMenu()
                }
            },
            spans: { [weak self] text, then in self?.readSkillSpans(in: text, then: then) }
        )
    }

    /// Asks the core which skills fit the word that ends `beforeCursor`, the composer's text up
    /// to its cursor. Only the last answer counts.
    func readSkillMenu(beforeCursor: String) {
        guard beforeCursor.contains("$") || beforeCursor.contains("/") else {
            skillMenuDismissed = nil
            return closeSkillMenu()
        }
        guard let project = composerProject, let account = composerAccount else { return closeSkillMenu() }
        skillMenusAsked += 1
        let asked = skillMenusAsked
        let fields: JSON = [
            "server_id": project.serverID, "project_id": project.id, "account": account.id,
            "before_cursor": String(beforeCursor.suffix(200)),
        ]
        core.send("skill_menu", fields) { [weak self] result in
            guard let self, asked == skillMenusAsked, case .success(let answer) = result else { return }
            let menu = SkillMenu(json: answer)
            if menu.typed != skillMenuDismissed { skillMenuDismissed = nil }
            guard !menu.rows.isEmpty, skillMenuDismissed == nil else {
                skillMenu = nil
                return
            }
            if menu.rows.map(\.name) != skillMenu?.rows.map(\.name) { skillHighlight = 0 }
            skillMenu = menu
        }
    }

    func closeSkillMenu() {
        skillMenusAsked += 1
        skillMenu = nil
    }

    /// Leaves the menu closed until something else is typed.
    func dismissSkillMenu() {
        skillMenuDismissed = skillMenu?.typed
        closeSkillMenu()
    }

    func moveSkillHighlight(by step: Int) {
        guard let count = skillMenu?.rows.count, count > 0 else { return }
        skillHighlight = (skillHighlight + step + count) % count
    }

    /// Puts the skill, or the lit one, in place of the word typed.
    func pickSkill(_ row: SkillRow? = nil) {
        guard let menu = skillMenu, let row = row ?? (menu.rows.indices.contains(skillHighlight) ? menu.rows[skillHighlight] : nil) else { return }
        skillPick = SkillPick(id: (skillPick?.id ?? 0) + 1, name: row.name, label: row.label)
        closeSkillMenu()
    }

    /// The skills of the composer's agent that `text` names by their `$name`s.
    func readSkillSpans(in text: String, then: @escaping ([SkillSpan]) -> Void) {
        guard text.contains("$"), let project = composerProject, let account = composerAccount else { return }
        let fields: JSON = ["server_id": project.serverID, "project_id": project.id, "account": account.id, "text": text]
        core.send("skill_chips", fields) { result in
            guard case .success(let answer) = result else { return }
            then(answer.objects("chips").map(SkillSpan.init(json:)))
        }
    }
}
