import SwiftUI

/// A condition of a rule, on what the client is doing: names joined by not, and, or.
indirect enum ShortcutCondition: Equatable {
    case name(String)
    case not(ShortcutCondition)
    case and(ShortcutCondition, ShortcutCondition)
    case or(ShortcutCondition, ShortcutCondition)

    init?(json: JSON?) {
        guard let json else { return nil }
        let left = ShortcutCondition(json: json.object("left"))
        let right = ShortcutCondition(json: json.object("right"))
        switch json.string("type") {
        case "name": self = .name(json.string("name"))
        case "not":
            guard let of = ShortcutCondition(json: json.object("of")) else { return nil }
            self = .not(of)
        case "and":
            guard let left, let right else { return nil }
            self = .and(left, right)
        case "or":
            guard let left, let right else { return nil }
            self = .or(left, right)
        default: return nil
        }
    }

    func holds(in context: Set<String>) -> Bool {
        switch self {
        case .name(let name): name == "true" || context.contains(name)
        case .not(let of): !of.holds(in: context)
        case .and(let left, let right): left.holds(in: context) && right.holds(in: context)
        case .or(let left, let right): left.holds(in: context) || right.holds(in: context)
        }
    }
}

/// A key and the modifiers held with it, as the system has them.
struct ShortcutKeys: Equatable {
    let key: String
    let command: Bool
    let control: Bool
    let option: Bool
    let shift: Bool
}

/// A rule the core sent: the key that runs the command, where its condition holds.
struct ShortcutRule: Equatable {
    let command: String
    let keys: ShortcutKeys
    let when: ShortcutCondition?
    /// The keys as they are drawn, one cap each.
    let caps: [String]

    init(json: JSON) {
        command = json.string("command")
        let shortcut = json.object("shortcut") ?? [:]
        keys = ShortcutKeys(
            key: shortcut.string("key"), command: shortcut.bool("command"), control: shortcut.bool("control"),
            option: shortcut.bool("option"), shift: shortcut.bool("shift")
        )
        when = ShortcutCondition(json: json.object("when"))
        caps = json.strings("caps")
    }

    func applies(in context: Set<String>) -> Bool {
        when?.holds(in: context) ?? true
    }
}

/// One rule as the settings list it, or a command that has no key.
struct ShortcutRow: Identifiable, Equatable {
    let id: String
    let command: String
    let label: String
    /// As the file writes it, as in `mod+shift+k`. Empty without a key.
    let key: String
    let caps: [String]
    let when: String
    let custom: Bool
    let resettable: Bool
    let conflicts: [String]

    init(json: JSON) {
        id = json.string("id")
        command = json.string("command")
        label = json.string("label")
        key = json.string("key")
        caps = json.strings("caps")
        when = json.string("when")
        custom = json.bool("custom")
        resettable = json.bool("resettable")
        conflicts = json.strings("conflicts")
    }

    /// The rule as the core takes it back, to replace or remove.
    var rule: JSON {
        var rule: JSON = ["command": command, "key": key]
        if !when.isEmpty { rule["when"] = when }
        return rule
    }
}

struct ShortcutGroup: Identifiable, Equatable {
    let title: String
    let rows: [ShortcutRow]
    var id: String { title }
}

/// The keyboard shortcuts as the core last sent them.
struct Shortcuts: Equatable {
    var rules: [ShortcutRule] = []
    var groups: [ShortcutGroup] = []
    var commands: [(id: String, label: String)] = []
    var conditions: [String] = []
    var issues: [String] = []
    var path = ""

    init() {}

    init(json: JSON) {
        rules = json.objects("rules").map(ShortcutRule.init)
        groups = json.objects("groups").map { group in
            ShortcutGroup(title: group.string("title"), rows: group.objects("rows").map(ShortcutRow.init))
        }
        commands = json.objects("commands").map { ($0.string("id"), $0.string("label")) }
        conditions = json.strings("conditions")
        issues = json.strings("issues")
        path = json.string("path")
    }

    static func == (one: Shortcuts, other: Shortcuts) -> Bool {
        one.rules == other.rules && one.groups == other.groups && one.issues == other.issues && one.path == other.path
    }

    /// The command the keys run where `context` holds: the last rule that matches. `candidates`
    /// are the names the pressed key goes by.
    func command(for candidates: Set<String>, command: Bool, control: Bool, option: Bool, shift: Bool, in context: Set<String>) -> String? {
        rules.last { rule in
            rule.keys.command == command && rule.keys.control == control && rule.keys.option == option && rule.keys.shift == shift
                && candidates.contains(rule.keys.key) && rule.applies(in: context)
        }?.command
    }

    /// The rules of the command that run where `context` holds, each not taken by a later rule
    /// on its keys, in the order they were given.
    func effective(_ command: String, in context: Set<String> = []) -> [ShortcutRule] {
        var taken: [ShortcutKeys] = []
        var found: [ShortcutRule] = []
        for rule in rules.reversed() where rule.applies(in: context) && !taken.contains(rule.keys) {
            taken.append(rule.keys)
            if rule.command == command { found.insert(rule, at: 0) }
        }
        return found
    }

    /// The keys of the command, as said in a tooltip: "⌘B".
    func label(_ command: String, in context: Set<String> = []) -> String? {
        effective(command, in: context).first.map { $0.caps.joined() }
    }

    /// The text with the command's keys after it in parentheses, as a tooltip says them.
    func help(_ text: String, _ command: String, in context: Set<String> = []) -> String {
        guard let label = label(command, in: context) else { return text }
        return "\(text) (\(label))"
    }

    /// The keys a menu item shows. Only a rule without a condition can be shown, since the menu
    /// would run it anywhere.
    func menuShortcut(_ command: String) -> KeyboardShortcut? {
        guard let rule = effective(command).first(where: { $0.when == nil }) else { return nil }
        return rule.keys.keyboardShortcut
    }
}

extension ShortcutKeys {
    var keyboardShortcut: KeyboardShortcut? {
        let equivalent: KeyEquivalent
        switch key {
        case "enter": equivalent = .return
        case "tab": equivalent = .tab
        case "space": equivalent = .space
        case "escape": equivalent = .escape
        case "backspace": equivalent = .delete
        case "delete": equivalent = .deleteForward
        case "arrowup": equivalent = .upArrow
        case "arrowdown": equivalent = .downArrow
        case "arrowleft": equivalent = .leftArrow
        case "arrowright": equivalent = .rightArrow
        case "home": equivalent = .home
        case "end": equivalent = .end
        case "pageup": equivalent = .pageUp
        case "pagedown": equivalent = .pageDown
        default:
            guard key.count == 1, let character = key.first else { return nil }
            equivalent = KeyEquivalent(character)
        }
        var modifiers: EventModifiers = []
        if command { modifiers.insert(.command) }
        if control { modifiers.insert(.control) }
        if option { modifiers.insert(.option) }
        if shift { modifiers.insert(.shift) }
        return KeyboardShortcut(equivalent, modifiers: modifiers)
    }
}

extension View {
    /// Gives a menu item the keys of the command, as the user set them.
    @ViewBuilder func shortcut(_ command: String, in shortcuts: Shortcuts) -> some View {
        if let shortcut = shortcuts.menuShortcut(command) {
            keyboardShortcut(shortcut)
        } else {
            self
        }
    }
}
