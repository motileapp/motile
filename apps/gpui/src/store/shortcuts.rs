//! The keyboard shortcuts as the core last sent them, and how a key press is matched against
//! their rules: the last rule whose keys and condition hold wins.

use gpui_kit::{Keystroke, Modifiers};
use motile_core::api::Command;
use motile_core::keybindings::{ActiveRule, CommandInfo, Condition, Group, Row, Rule, Shortcut, View};
use serde::Deserialize;

use super::Store;

pub struct Shortcuts {
    pub rules: Vec<ActiveRule>,
    pub groups: Vec<Group>,
    pub commands: Vec<CommandInfo>,
    pub conditions: Vec<String>,
    pub issues: Vec<String>,
    /// The system the keys are drawn for, as `std::env::consts::OS` names it.
    pub platform: &'static str,
}

impl Default for Shortcuts {
    fn default() -> Self {
        Self {
            rules: Vec::new(),
            groups: Vec::new(),
            commands: Vec::new(),
            conditions: Vec::new(),
            issues: Vec::new(),
            platform: std::env::consts::OS,
        }
    }
}

impl From<View> for Shortcuts {
    fn from(view: View) -> Self {
        Self {
            rules: view.rules,
            groups: view.groups,
            commands: view.commands,
            conditions: view.conditions,
            issues: view.issues,
            ..Self::default()
        }
    }
}

fn is_apple(platform: &str) -> bool {
    matches!(platform, "macos" | "ios")
}

/// Caps read as one label: run together on the Mac, "⇧⌘O", and joined by "+" elsewhere,
/// "Ctrl+Shift+O".
pub fn join_caps(caps: &[impl AsRef<str>], platform: &str) -> String {
    let caps: Vec<&str> = caps.iter().map(AsRef::as_ref).collect();
    caps.join(if is_apple(platform) { "" } else { "+" })
}

/// How keys being chosen read, what they clash with and what is wrong with their condition.
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
pub struct ShortcutCheck {
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub conflicts: Vec<String>,
    /// The condition's names the client doesn't know, which are never true.
    #[serde(default)]
    pub unknown: Vec<String>,
    pub when_error: Option<String>,
}

/// The rule a row of the settings shows, as the file writes it.
pub fn rule_of(row: &Row) -> Rule {
    Rule { key: row.key.clone(), command: row.command.clone(), when: (!row.when.is_empty()).then(|| row.when.clone()) }
}

impl Store {
    /// Gives the command the key, in place of `replacing`'s rule when that row has a key.
    pub fn set_shortcut(&mut self, command: &str, key: &str, when: &str, replacing: Option<&Row>) {
        let when = Some(when.trim().to_string()).filter(|when| !when.is_empty());
        let replace = replacing.filter(|row| !row.key.is_empty()).map(rule_of);
        self.change_shortcuts(Command::SetKeybinding { command: command.into(), key: key.into(), when, replace });
    }

    pub fn remove_shortcut(&mut self, row: &Row) {
        self.change_shortcuts(Command::RemoveKeybinding { rule: rule_of(row) });
    }

    pub fn reset_shortcut(&mut self, command: &str) {
        self.change_shortcuts(Command::ResetKeybinding { command: command.into() });
    }

    /// Reads the key and the condition, as the rule of `row` if it has one, and says what they
    /// clash with.
    pub fn check_shortcut(
        &mut self,
        key: &str,
        when: &str,
        row: Option<&Row>,
        done: impl FnOnce(ShortcutCheck, &mut gpui_kit::App) + 'static,
    ) {
        let row = row.filter(|row| !row.key.is_empty()).map(rule_of);
        let command = Command::CheckKeybinding { key: key.into(), when: Some(when.into()), row };
        self.ask(command, move |_, result, cx| {
            let Ok(value) = result else { return };
            done(serde_json::from_value(value).unwrap_or_default(), cx);
        });
    }

    /// Opens `keybindings.json` in the app that edits JSON here.
    pub fn open_shortcuts_file(&mut self) {
        self.ask(Command::KeybindingsFile, |store, result, cx| match result {
            Ok(value) => {
                let Some(path) = value["path"].as_str() else { return };
                cx.open_with_system(std::path::Path::new(path));
            }
            Err(error) => store.fail(error),
        });
    }

    fn change_shortcuts(&mut self, command: Command) {
        self.ask(command, |store, result, _| {
            if let Err(error) = result {
                store.fail(error);
            }
        });
    }
}

/// The names that hold now for the rules' conditions.
pub type ShortcutContext = Vec<&'static str>;

fn holds(condition: &Condition, context: &[&str]) -> bool {
    match condition {
        Condition::Name { name } => name == "true" || context.contains(&name.as_str()),
        Condition::Not { of } => !holds(of, context),
        Condition::And { left, right } => holds(left, context) && holds(right, context),
        Condition::Or { left, right } => holds(left, context) || holds(right, context),
    }
}

fn applies(rule: &ActiveRule, context: &[&str]) -> bool {
    rule.when.as_ref().is_none_or(|when| holds(when, context))
}

/// A key press as the rules name it: every name its key goes by, the first the one it is
/// recorded by, and the modifiers held with it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct KeyPress {
    pub names: Vec<String>,
    pub command: bool,
    pub control: bool,
    pub option: bool,
    pub shift: bool,
}

/// The keys with names of their own, by their key code.
#[cfg(any(target_os = "macos", test))]
const NAMED: [(u16, &str); 27] = [
    (36, "enter"),
    (76, "enter"),
    (48, "tab"),
    (49, "space"),
    (51, "backspace"),
    (117, "delete"),
    (53, "escape"),
    (123, "arrowleft"),
    (124, "arrowright"),
    (125, "arrowdown"),
    (126, "arrowup"),
    (115, "home"),
    (119, "end"),
    (116, "pageup"),
    (121, "pagedown"),
    (122, "f1"),
    (120, "f2"),
    (99, "f3"),
    (118, "f4"),
    (96, "f5"),
    (97, "f6"),
    (98, "f7"),
    (100, "f8"),
    (101, "f9"),
    (109, "f10"),
    (103, "f11"),
    (111, "f12"),
];

/// The digits and signs where they are on a US keyboard, by key code.
#[cfg(any(target_os = "macos", test))]
const SIGNS: [(u16, &str); 21] = [
    (29, "0"),
    (18, "1"),
    (19, "2"),
    (20, "3"),
    (21, "4"),
    (23, "5"),
    (22, "6"),
    (26, "7"),
    (28, "8"),
    (25, "9"),
    (27, "-"),
    (24, "="),
    (33, "["),
    (30, "]"),
    (42, "\\"),
    (41, ";"),
    (39, "'"),
    (43, ","),
    (47, "."),
    (44, "/"),
    (50, "`"),
];

/// The letters where they are on a US keyboard, by key code.
#[cfg(any(target_os = "macos", test))]
const LETTERS: [(u16, &str); 26] = [
    (0, "a"),
    (11, "b"),
    (8, "c"),
    (2, "d"),
    (14, "e"),
    (3, "f"),
    (5, "g"),
    (4, "h"),
    (34, "i"),
    (38, "j"),
    (40, "k"),
    (37, "l"),
    (46, "m"),
    (45, "n"),
    (31, "o"),
    (35, "p"),
    (12, "q"),
    (15, "r"),
    (1, "s"),
    (17, "t"),
    (32, "u"),
    (9, "v"),
    (13, "w"),
    (7, "x"),
    (16, "y"),
    (6, "z"),
];

#[cfg(any(target_os = "macos", test))]
fn by_code(table: &[(u16, &'static str)], code: u16) -> Option<&'static str> {
    table.iter().find(|(key, _)| *key == code).map(|(_, name)| *name)
}

#[cfg(any(target_os = "macos", test))]
fn is_letter(name: &str) -> bool {
    let mut chars = name.chars();
    matches!((chars.next(), chars.next()), (Some(letter), None) if letter.is_ascii_alphabetic())
}

/// A key as the system gives it: its code, and what the keyboard's layout types on it without
/// the modifiers but Shift.
#[cfg(any(target_os = "macos", test))]
pub struct TypedKey<'a> {
    pub code: u16,
    pub typed: Option<&'a str>,
}

#[cfg(any(target_os = "macos", test))]
impl TypedKey<'_> {
    fn typed(&self) -> Option<String> {
        self.typed.map(str::to_lowercase).filter(|typed| !typed.is_empty())
    }

    /// The one name the key is recorded by: a letter is the one the layout types, a digit or a
    /// sign the key where it is on a US keyboard, so that ⌘[ is the same key in every layout and
    /// with Shift.
    fn name(&self) -> Option<String> {
        if let Some(name) = by_code(&NAMED, self.code) {
            return Some(name.into());
        }
        let typed = self.typed();
        if let Some(typed) = typed.as_ref().filter(|typed| is_letter(typed)) {
            return Some(typed.clone());
        }
        by_code(&SIGNS, self.code).or_else(|| by_code(&LETTERS, self.code)).map(String::from).or(typed)
    }

    /// Every name the key matches: its own, what the layout types, and the letter of its place
    /// when the layout types no letter there.
    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.name().into_iter().collect();
        if let Some(typed) = self.typed().filter(|typed| !names.contains(typed)) {
            names.push(typed);
        }
        if let Some(letter) = by_code(&LETTERS, self.code)
            && !names.iter().any(|name| is_letter(name))
        {
            names.push(letter.into());
        }
        names
    }
}

/// The digits and signs a US keyboard types with Shift, by the key they are on, for a press
/// that comes without its key code.
const SHIFTED: [(char, char); 21] = [
    ('!', '1'),
    ('@', '2'),
    ('#', '3'),
    ('$', '4'),
    ('%', '5'),
    ('^', '6'),
    ('&', '7'),
    ('*', '8'),
    ('(', '9'),
    (')', '0'),
    ('_', '-'),
    ('+', '='),
    ('{', '['),
    ('}', ']'),
    ('|', '\\'),
    (':', ';'),
    ('"', '\''),
    ('<', ','),
    ('>', '.'),
    ('?', '/'),
    ('~', '`'),
];

impl KeyPress {
    #[cfg(any(target_os = "macos", test))]
    /// The press of the key the system names: as the Mac app's `KeyNames` names it.
    pub fn of_key(key: TypedKey, command: bool, control: bool, option: bool, shift: bool) -> Option<Self> {
        let names = key.names();
        if names.is_empty() {
            return None;
        }
        Some(Self { names, command, control, option, shift })
    }

    /// The press as GPUI names it, for a key without its code, as the dev driver sends. Nothing
    /// for a modifier pressed alone.
    pub fn of(keystroke: &Keystroke) -> Option<Self> {
        let modifiers = &keystroke.modifiers;
        let mut shift = modifiers.shift;
        let key = match keystroke.key.as_str() {
            "up" => "arrowup".to_string(),
            "down" => "arrowdown".to_string(),
            "left" => "arrowleft".to_string(),
            "right" => "arrowright".to_string(),
            "enter" | "tab" | "space" | "backspace" | "delete" | "escape" | "home" | "end" | "pageup" | "pagedown" => {
                keystroke.key.clone()
            }
            key if key.len() <= 3 && key.starts_with('f') && key[1..].parse::<u8>().is_ok() => key.to_string(),
            key => {
                let mut chars = key.chars();
                let (Some(typed), None) = (chars.next(), chars.next()) else { return None };
                if let Some((_, base)) = SHIFTED.iter().find(|(sign, _)| *sign == typed) {
                    shift = true;
                    base.to_string()
                } else if typed.is_uppercase() {
                    shift = true;
                    typed.to_lowercase().to_string()
                } else {
                    typed.to_string()
                }
            }
        };
        Some(Self {
            names: vec![key],
            command: modifiers.platform,
            control: modifiers.control,
            option: modifiers.alt,
            shift,
        })
    }

    /// The press as `keybindings.json` writes it, as in `mod+shift+k`: `mod` is ⌘ on the Mac and
    /// Ctrl elsewhere.
    pub fn rule(&self) -> String {
        let (primary, other) = if cfg!(target_os = "macos") {
            ((self.command, "mod"), (self.control, "ctrl"))
        } else {
            ((self.control, "mod"), (self.command, "cmd"))
        };
        let held = [primary, other, (self.option, "alt"), (self.shift, "shift")];
        let mut parts: Vec<&str> = held.into_iter().filter(|(down, _)| *down).map(|(_, name)| name).collect();
        parts.push(&self.names[0]);
        parts.join("+")
    }

    pub fn is_escape(&self) -> bool {
        self.names[0] == "escape" && !(self.command || self.control || self.option || self.shift)
    }

    fn is(&self, shortcut: &Shortcut) -> bool {
        self.names.contains(&shortcut.key)
            && shortcut.command == self.command
            && shortcut.control == self.control
            && shortcut.option == self.option
            && shortcut.shift == self.shift
    }
}

impl Shortcuts {
    /// The command the press runs where `context` holds: the last rule that matches.
    pub fn command_for(&self, press: &KeyPress, context: &[&str]) -> Option<&str> {
        self.rules
            .iter()
            .rev()
            .find(|rule| press.is(&rule.shortcut) && applies(rule, context))
            .map(|rule| rule.command.as_str())
    }

    /// The rules of the command that run where `context` holds, each not taken by a later rule
    /// on its keys, in the order they were given.
    pub fn effective(&self, command: &str, context: &[&str]) -> Vec<&ActiveRule> {
        let mut taken: Vec<&Shortcut> = Vec::new();
        let mut found = Vec::new();
        for rule in self.rules.iter().rev() {
            if !applies(rule, context) || taken.contains(&&rule.shortcut) {
                continue;
            }
            taken.push(&rule.shortcut);
            if rule.command == command {
                found.insert(0, rule);
            }
        }
        found
    }

    /// The keys of the command, as a tooltip says them: "⌘B" on the Mac, "Ctrl+B" elsewhere.
    pub fn label(&self, command: &str) -> Option<String> {
        self.label_in(command, &[])
    }

    /// The keys of the command where `context` holds.
    pub fn label_in(&self, command: &str, context: &[&str]) -> Option<String> {
        self.effective(command, context).first().map(|rule| join_caps(&rule.caps, self.platform))
    }

    /// The keys of the command, one cap each.
    pub fn caps(&self, command: &str) -> Option<Vec<String>> {
        self.effective(command, &[]).first().map(|rule| rule.caps.clone())
    }

    /// The text with the command's keys after it in parentheses: "Hide the sidebar (⌘B)".
    pub fn help(&self, text: &str, command: &str) -> String {
        match self.label(command) {
            Some(label) => format!("{text} ({label})"),
            None => text.to_string(),
        }
    }

    /// The caps of keys no command has, as `keybindings.json` writes them: `mod+r` is ⌘ and R
    /// on the Mac, Ctrl and R elsewhere.
    pub fn caps_of(&self, keys: &str) -> Vec<String> {
        motile_core::keybindings::caps_of(keys, self.platform)
    }

    /// Keys no command has, as a tooltip says them: `mod+r` is "⌘R" on the Mac, "Ctrl+R"
    /// elsewhere.
    pub fn keys_label(&self, keys: &str) -> String {
        join_caps(&self.caps_of(keys), self.platform)
    }

    /// A click with Shift held, as a tooltip says it.
    pub fn shift_click(&self) -> &'static str {
        if is_apple(self.platform) { "⇧-click" } else { "Shift+click" }
    }

    /// The keys a menu item shows, as GPUI writes them. Only a rule without a condition can be
    /// shown, since the menu would run it anywhere.
    pub fn menu_keystroke(&self, command: &str) -> Option<String> {
        let rule = self.effective(command, &[]).into_iter().find(|rule| rule.when.is_none())?;
        keystroke_text(&rule.shortcut)
    }

    /// The modifiers of the command's first rule, to tell when they alone are held.
    pub fn modifiers(&self, command: &str) -> Option<Modifiers> {
        let rule = self.effective(command, &[]).into_iter().next()?;
        let shortcut = &rule.shortcut;
        Some(Modifiers {
            platform: shortcut.command,
            control: shortcut.control,
            alt: shortcut.option,
            shift: shortcut.shift,
            function: false,
        })
    }
}

/// The keys as GPUI parses them, as in `shift-secondary-[` on the Mac and `ctrl-shift-[`
/// elsewhere.
fn keystroke_text(shortcut: &Shortcut) -> Option<String> {
    let key = match shortcut.key.as_str() {
        "arrowup" => "up",
        "arrowdown" => "down",
        "arrowleft" => "left",
        "arrowright" => "right",
        key => key,
    };
    if key.is_empty() {
        return None;
    }
    let mut text = String::new();
    for (down, name) in [
        (shortcut.control, "ctrl-"),
        (shortcut.option, "alt-"),
        (shortcut.shift, "shift-"),
        (shortcut.command, if cfg!(target_os = "macos") { "secondary-" } else { "cmd-" }),
    ] {
        if down {
            text.push_str(name);
        }
    }
    text.push_str(key);
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(command: &str, key: &str, mods: &str, when: Option<Condition>, caps: &[&str]) -> ActiveRule {
        ActiveRule {
            command: command.into(),
            shortcut: Shortcut {
                key: key.into(),
                command: mods.contains("cmd"),
                control: mods.contains("ctrl"),
                option: mods.contains("alt"),
                shift: mods.contains("shift"),
            },
            when,
            caps: caps.iter().map(|cap| cap.to_string()).collect(),
        }
    }

    fn name(name: &str) -> Condition {
        Condition::Name { name: name.into() }
    }

    fn shortcuts(rules: Vec<ActiveRule>) -> Shortcuts {
        Shortcuts { rules, platform: "macos", ..Default::default() }
    }

    fn press(text: &str) -> KeyPress {
        KeyPress::of(&Keystroke::parse(text).unwrap()).unwrap()
    }

    fn key(code: u16, typed: &str, shift: bool) -> KeyPress {
        KeyPress::of_key(TypedKey { code, typed: Some(typed) }, true, false, false, shift).unwrap()
    }

    #[test]
    fn digits_and_signs_go_by_their_place_and_letters_by_the_layout() {
        let keys = shortcuts(vec![
            rule("thread.jump.7", "7", "cmd", None, &["⌘", "7"]),
            rule("thread.previous", "[", "cmd-shift", None, &["⇧", "⌘", "["]),
            rule("chat.new", "n", "cmd", None, &["⌘", "N"]),
            rule("sidebar.toggle", "z", "cmd", None, &["⌘", "Z"]),
        ]);
        // The 7 key on a Czech layout, AZERTY's, and German's [ key with Shift.
        assert_eq!(keys.command_for(&key(26, "ý", false), &[]), Some("thread.jump.7"));
        assert_eq!(keys.command_for(&key(26, "è", false), &[]), Some("thread.jump.7"));
        assert_eq!(keys.command_for(&key(33, "Ü", true), &[]), Some("thread.previous"));
        // A letter is the one the layout types: Dvorak's N is on the US L key.
        assert_eq!(keys.command_for(&key(37, "n", false), &[]), Some("chat.new"));
        // A layout with no letter on the key still has the US letter there.
        assert_eq!(keys.command_for(&key(6, "я", false), &[]), Some("sidebar.toggle"));
        assert_eq!(key(26, "ý", false).rule(), "mod+7");
        assert_eq!(key(33, "Ü", true).rule(), "mod+shift+[");
        assert_eq!(key(16, "z", false).rule(), "mod+z");
        assert_eq!(key(53, "\u{1b}", false).names[0], "escape");
    }

    #[test]
    fn the_last_rule_that_holds_wins() {
        let keys = shortcuts(vec![
            rule("thread.stop", ".", "cmd", None, &["⌘", "."]),
            rule("composer.sendAlternate", "enter", "cmd", Some(name("composerFocus")), &["⌘", "↩"]),
            rule("other", ".", "cmd", Some(name("turnRunning")), &["⌘", "."]),
        ]);
        assert_eq!(keys.command_for(&press("cmd-."), &[]), Some("thread.stop"));
        assert_eq!(keys.command_for(&press("cmd-."), &["turnRunning"]), Some("other"));
        assert_eq!(keys.command_for(&press("cmd-enter"), &[]), None);
        assert_eq!(keys.command_for(&press("cmd-enter"), &["composerFocus"]), Some("composer.sendAlternate"));
        assert_eq!(keys.command_for(&press("cmd-shift-."), &[]), None);
    }

    #[test]
    fn signs_match_by_their_key_with_or_without_shift() {
        let keys = shortcuts(vec![rule("thread.next", "]", "cmd-shift", None, &["⇧", "⌘", "]"])]);
        assert_eq!(keys.command_for(&press("cmd-shift-]"), &[]), Some("thread.next"));
        let typed = Keystroke { modifiers: Modifiers::command(), key: "}".into(), key_char: None };
        assert_eq!(keys.command_for(&KeyPress::of(&typed).unwrap(), &[]), Some("thread.next"));
        assert_eq!(press("alt-up").names, ["arrowup"]);
    }

    #[test]
    fn a_recorded_press_is_written_as_the_file_writes_it() {
        assert_eq!(press("cmd-shift-k").rule(), "mod+shift+k");
        assert_eq!(press("ctrl-alt-up").rule(), "ctrl+alt+arrowup");
        let typed = Keystroke { modifiers: Modifiers::command(), key: "{".into(), key_char: None };
        assert_eq!(KeyPress::of(&typed).unwrap().rule(), "mod+shift+[");
        assert_eq!(press("f5").rule(), "f5");
    }

    #[test]
    fn conditions_combine() {
        let when = Condition::And {
            left: Box::new(name("composerFocus")),
            right: Box::new(Condition::Not { of: Box::new(name("turnRunning")) }),
        };
        assert!(holds(&when, &["composerFocus"]));
        assert!(!holds(&when, &["composerFocus", "turnRunning"]));
        assert!(holds(&name("true"), &[]));
    }

    #[test]
    fn labels_and_menus_skip_rules_a_later_one_takes() {
        let keys = shortcuts(vec![
            rule("chat.new", "n", "cmd", None, &["⌘", "N"]),
            rule("chat.new", "o", "cmd-shift", None, &["⇧", "⌘", "O"]),
            rule("other", "n", "cmd", None, &["⌘", "N"]),
            rule("usage.cost", "c", "", Some(name("usagePageOpen")), &["C"]),
        ]);
        assert_eq!(keys.label("chat.new").as_deref(), Some("⇧⌘O"));
        assert_eq!(keys.help("New thread", "chat.new"), "New thread (⇧⌘O)");
        assert_eq!(keys.help("Nothing", "missing"), "Nothing");
        assert_eq!(keys.menu_keystroke("chat.new").as_deref(), Some("shift-secondary-o"));
        assert_eq!(keys.menu_keystroke("usage.cost"), None);
    }

    #[test]
    fn labels_off_the_mac_join_words_with_plus() {
        for platform in ["linux", "windows"] {
            let caps = motile_core::keybindings::caps_of("mod+shift+o", platform);
            let caps: Vec<&str> = caps.iter().map(String::as_str).collect();
            let keys = Shortcuts {
                rules: vec![rule("chat.new", "o", "ctrl-shift", None, &caps)],
                platform,
                ..Default::default()
            };
            assert_eq!(keys.label("chat.new").as_deref(), Some("Ctrl+Shift+O"));
            assert_eq!(keys.help("New thread", "chat.new"), "New thread (Ctrl+Shift+O)");
            assert_eq!(keys.menu_keystroke("chat.new").as_deref(), Some("ctrl-shift-o"));
            assert_eq!(keys.keys_label("mod+r"), "Ctrl+R");
            assert_eq!(keys.caps_of("mod+enter"), ["Ctrl", "Enter"]);
            assert_eq!(keys.shift_click(), "Shift+click");
        }
        let mac = shortcuts(Vec::new());
        assert_eq!(mac.keys_label("mod+r"), "⌘R");
        assert_eq!(mac.caps_of("mod+enter"), ["⌘", "↩"]);
        assert_eq!(mac.shift_click(), "⇧-click");
    }

    #[test]
    fn control_is_matched_as_the_key_it_is() {
        let keys = shortcuts(vec![rule("chat.new", "n", "ctrl", None, &["Ctrl", "N"])]);
        assert_eq!(keys.command_for(&press("ctrl-n"), &[]), Some("chat.new"));
        assert_eq!(keys.command_for(&press("cmd-n"), &[]), None);
    }
}
