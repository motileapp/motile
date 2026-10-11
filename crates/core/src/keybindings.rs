//! The keyboard shortcuts: rules that give a command a key, maybe on a condition such as
//! `composerFocus && turnRunning`; the last rule that matches wins. `keybindings.json` in the data
//! folder holds the commands the user changed, which lose their default keys.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

const FILE: &str = "keybindings.json";
const MAX_RULES: usize = 256;
const MAX_KEY: usize = 64;
const MAX_WHEN: usize = 256;
const MAX_DEPTH: usize = 64;

/// The sections of the settings page, in order.
const GROUPS: [&str; 5] = ["Navigation", "Threads", "Composer", "Side Panel", "Usage"];

/// What the conditions can ask about the client. Any other name is false.
pub const CONDITIONS: [&str; 7] =
    ["composerFocus", "editableFocus", "turnRunning", "threadOpen", "rightPanelOpen", "settingsOpen", "usagePageOpen"];

/// Every command, by group, in the order the settings list them.
const COMMANDS: &[(&str, &str, usize)] = &[
    ("commandPalette.toggle", "Commands", 0),
    ("threadPicker.toggle", "Go to Thread", 0),
    ("sidebar.toggle", "Show or Hide the Sidebar", 0),
    ("settings.open", "Settings", 0),
    ("usage.open", "Usage", 0),
    ("appearance.cycle", "Switch the Theme", 0),
    ("chat.new", "New Thread", 1),
    ("chat.newLocal", "New Thread in This Project", 1),
    ("chat.newWithoutProject", "New Thread Without a Project", 1),
    ("thread.previous", "Previous Thread", 1),
    ("thread.next", "Next Thread", 1),
    ("thread.jump.1", "Go to the 1st Thread", 1),
    ("thread.jump.2", "Go to the 2nd Thread", 1),
    ("thread.jump.3", "Go to the 3rd Thread", 1),
    ("thread.jump.4", "Go to the 4th Thread", 1),
    ("thread.jump.5", "Go to the 5th Thread", 1),
    ("thread.jump.6", "Go to the 6th Thread", 1),
    ("thread.jump.7", "Go to the 7th Thread", 1),
    ("thread.jump.8", "Go to the 8th Thread", 1),
    ("thread.jump.9", "Go to the 9th Thread", 1),
    ("thread.done", "Mark Done or Undone", 1),
    ("thread.undo", "Undo Mark Done", 1),
    ("thread.stop", "Stop the Agent", 1),
    ("thread.steerQueuedMessage", "Steer With the First Queued Message", 1),
    ("thread.editQueuedMessage", "Edit the Last Queued Message", 1),
    ("pullRequest.copyLink", "Copy the Pull Request's Link", 1),
    ("pullRequest.copyNumber", "Copy the Pull Request's Number", 1),
    ("composer.sendAlternate", "Send the Other Way: Queue or Steer", 2),
    ("modelPicker.toggle", "Choose the Model", 2),
    ("composer.effort", "Choose the Effort", 2),
    ("composer.access", "Choose the Access", 2),
    ("composer.project", "Choose the Project", 2),
    ("composer.workspace", "Choose the Workspace", 2),
    ("composer.branch", "Choose the Branch", 2),
    ("rightPanel.toggle", "Show or Hide the Side Panel", 3),
    ("rightPanel.toggleMaximized", "Maximize or Restore the Side Panel", 3),
    ("rightPanel.new", "New Tab", 3),
    ("rightPanel.close", "Close", 3),
    ("rightPanel.nextTab", "Next Tab", 3),
    ("rightPanel.previousTab", "Previous Tab", 3),
    ("rightPanel.diff", "Show Changes", 3),
    ("rightPanel.files", "Show Files", 3),
    ("rightPanel.agents", "Show Agents", 3),
    ("rightPanel.pullRequest", "Show Pull Request", 3),
    ("rightPanel.pullRequests", "Show All Pull Requests", 3),
    ("rightPanel.linear", "Show Linear", 3),
    ("usage.cost", "Show Cost", 4),
    ("usage.tokens", "Show Tokens", 4),
    ("usage.limits", "Show Limits", 4),
    ("usage.period.day", "Show the Last 24 Hours", 4),
    ("usage.period.week", "Show the Last 7 Days", 4),
    ("usage.period.month", "Show the Last 30 Days", 4),
    ("usage.period.quarter", "Show the Last 90 Days", 4),
];

/// The keys every command has until the user changes them, in order of precedence.
const DEFAULTS: &[(&str, &str, Option<&str>)] = &[
    ("mod+k", "commandPalette.toggle", None),
    ("mod+p", "threadPicker.toggle", None),
    ("mod+b", "sidebar.toggle", None),
    ("mod+,", "settings.open", None),
    ("mod+u", "usage.open", None),
    ("mod+alt+shift+a", "appearance.cycle", None),
    ("mod+n", "chat.new", None),
    ("mod+shift+o", "chat.new", None),
    ("mod+shift+n", "chat.newLocal", None),
    ("mod+alt+n", "chat.newWithoutProject", None),
    ("mod+shift+[", "thread.previous", None),
    ("mod+shift+]", "thread.next", None),
    ("mod+1", "thread.jump.1", None),
    ("mod+2", "thread.jump.2", None),
    ("mod+3", "thread.jump.3", None),
    ("mod+4", "thread.jump.4", None),
    ("mod+5", "thread.jump.5", None),
    ("mod+6", "thread.jump.6", None),
    ("mod+7", "thread.jump.7", None),
    ("mod+8", "thread.jump.8", None),
    ("mod+9", "thread.jump.9", None),
    ("mod+shift+s", "thread.done", None),
    ("mod+z", "thread.undo", Some("!editableFocus")),
    ("mod+.", "thread.stop", None),
    ("mod+shift+enter", "thread.steerQueuedMessage", None),
    ("alt+arrowup", "thread.editQueuedMessage", Some("composerFocus")),
    ("mod+shift+c", "pullRequest.copyLink", None),
    ("mod+shift+k", "pullRequest.copyNumber", None),
    ("mod+enter", "composer.sendAlternate", Some("composerFocus && turnRunning")),
    ("mod+shift+m", "modelPicker.toggle", None),
    ("mod+shift+e", "composer.effort", None),
    ("mod+shift+a", "composer.access", None),
    ("mod+shift+h", "composer.project", None),
    ("mod+shift+x", "composer.workspace", None),
    ("mod+shift+g", "composer.branch", None),
    ("mod+alt+b", "rightPanel.toggle", None),
    ("mod+alt+shift+b", "rightPanel.toggleMaximized", None),
    ("mod+t", "rightPanel.new", None),
    ("mod+w", "rightPanel.close", None),
    ("ctrl+tab", "rightPanel.nextTab", None),
    ("ctrl+shift+tab", "rightPanel.previousTab", None),
    ("mod+d", "rightPanel.diff", None),
    ("mod+alt+e", "rightPanel.files", None),
    ("mod+alt+a", "rightPanel.agents", None),
    ("mod+shift+r", "rightPanel.pullRequest", None),
    ("mod+alt+shift+r", "rightPanel.pullRequests", None),
    ("c", "usage.cost", Some("usagePageOpen")),
    ("t", "usage.tokens", Some("usagePageOpen")),
    ("l", "usage.limits", Some("usagePageOpen")),
    ("mod+shift+1", "usage.period.day", Some("usagePageOpen")),
    ("mod+shift+2", "usage.period.week", Some("usagePageOpen")),
    ("mod+shift+3", "usage.period.month", Some("usagePageOpen")),
    ("mod+shift+4", "usage.period.quarter", Some("usagePageOpen")),
];

/// A rule as the file has it. A `command` that starts with `-` takes the command's keys away.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Rule {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub key: String,
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
}

impl Rule {
    fn new(key: &str, command: &str, when: Option<&str>) -> Rule {
        Rule { key: key.into(), command: command.into(), when: when.map(String::from) }
    }

    fn removes(&self) -> Option<&str> {
        self.command.strip_prefix('-')
    }

    fn named(&self) -> &str {
        self.removes().unwrap_or(&self.command)
    }
}

/// A key and the modifiers held with it. `primary` is ⌘ on Apple's systems and Ctrl elsewhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keys {
    key: String,
    primary: bool,
    command: bool,
    control: bool,
    option: bool,
    shift: bool,
}

/// What a client matches a key press against: the modifiers as the system has them.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub key: String,
    pub command: bool,
    pub control: bool,
    pub option: bool,
    pub shift: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Condition {
    Name { name: String },
    Not { of: Box<Condition> },
    And { left: Box<Condition>, right: Box<Condition> },
    Or { left: Box<Condition>, right: Box<Condition> },
}

impl Condition {
    fn names(&self, into: &mut Vec<String>) {
        match self {
            Condition::Name { name } => {
                if !into.contains(name) {
                    into.push(name.clone());
                }
            }
            Condition::Not { of } => of.names(into),
            Condition::And { left, right } | Condition::Or { left, right } => {
                left.names(into);
                right.names(into);
            }
        }
    }

    fn text(&self) -> String {
        match self {
            Condition::Name { name } => name.clone(),
            Condition::Not { of } => format!("!{}", of.wrapped()),
            Condition::And { left, right } => format!("{} && {}", left.wrapped(), right.wrapped()),
            Condition::Or { left, right } => format!("{} || {}", left.wrapped(), right.wrapped()),
        }
    }

    fn wrapped(&self) -> String {
        match self {
            Condition::Name { .. } | Condition::Not { .. } => self.text(),
            _ => format!("({})", self.text()),
        }
    }
}

/// A rule that parsed: its key and condition as written back, and what they mean.
#[derive(Clone, Debug)]
struct Compiled {
    command: String,
    key: String,
    keys: Keys,
    when: Option<Condition>,
}

impl Compiled {
    fn when_text(&self) -> String {
        self.when.as_ref().map(Condition::text).unwrap_or_default()
    }

    fn same(&self, other: &Compiled) -> bool {
        self.command == other.command && self.key == other.key && self.when_text() == other.when_text()
    }
}

/// A rule the client matches key presses against, in order: the last that matches wins.
#[derive(Serialize, Clone, Debug)]
pub struct ActiveRule {
    pub command: String,
    pub shortcut: Shortcut,
    pub when: Option<Condition>,
    /// The keys as they are drawn, one cap each.
    pub caps: Vec<String>,
}

/// One rule as the settings show it, or a command with no key yet.
#[derive(Serialize, Clone, Debug)]
pub struct Row {
    pub id: String,
    pub command: String,
    pub label: String,
    /// As the file writes it, as in `mod+shift+k`. Empty for a command without a key.
    pub key: String,
    pub caps: Vec<String>,
    pub when: String,
    /// The rule isn't one of the command's defaults.
    pub custom: bool,
    /// The command's keys were changed and it has defaults to go back to.
    pub resettable: bool,
    /// The commands another rule on the same key runs where both conditions can hold.
    pub conflicts: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Group {
    pub title: String,
    pub rows: Vec<Row>,
}

#[derive(Serialize, Clone, Debug)]
pub struct CommandInfo {
    pub id: String,
    pub label: String,
}

/// Everything a client needs: the rules to match, the settings' rows, and what is wrong with the file.
#[derive(Serialize, Clone, Debug)]
pub struct View {
    pub path: String,
    pub rules: Vec<ActiveRule>,
    pub groups: Vec<Group>,
    pub commands: Vec<CommandInfo>,
    pub conditions: Vec<String>,
    pub issues: Vec<String>,
}

pub struct Keybindings {
    path: PathBuf,
    apple: bool,
    custom: Vec<Rule>,
    issues: Vec<String>,
}

impl Keybindings {
    pub fn load(data_dir: &Path, platform: &str) -> Keybindings {
        let mut keybindings = Keybindings {
            path: data_dir.join(FILE),
            apple: matches!(platform, "macos" | "ios"),
            custom: Vec::new(),
            issues: Vec::new(),
        };
        keybindings.reload();
        keybindings
    }

    /// Reads the file again. Whether anything changed.
    pub fn reload(&mut self) -> bool {
        let (custom, issues) = read(&self.path);
        let changed = custom != self.custom || issues != self.issues;
        self.custom = custom;
        self.issues = issues;
        changed
    }

    /// The file, made with no rules if there is none yet.
    pub fn file(&self) -> Result<PathBuf, String> {
        if !self.path.exists() {
            std::fs::write(&self.path, "[]\n")
                .map_err(|error| format!("{} can't be written: {error}", self.path.display()))?;
        }
        Ok(self.path.clone())
    }

    /// Gives the command the key, in place of `replace` when it is one of the command's rules.
    pub fn set(&mut self, command: &str, key: &str, when: Option<&str>, replace: Option<&Rule>) -> Result<(), String> {
        let rule = self.compile_new(command, key, when)?;
        let mut rules = self.rules_of(command);
        if let Some(replaced) = replace.and_then(|replace| compile(replace).ok()) {
            rules.retain(|existing| !existing.same(&replaced));
        }
        rules.retain(|existing| !existing.same(&rule));
        rules.push(rule);
        self.write_command(command, rules)
    }

    /// Takes the rule away from its command.
    pub fn remove(&mut self, rule: &Rule) -> Result<(), String> {
        let removed = compile(rule)?;
        let mut rules = self.rules_of(&rule.command);
        rules.retain(|existing| !existing.same(&removed));
        self.write_command(&rule.command, rules)
    }

    /// Gives the command its default keys back.
    pub fn reset(&mut self, command: &str) -> Result<(), String> {
        self.write_command(command, defaults_of(command))
    }

    /// How a key the user is choosing reads, and what it would clash with. `row` is the rule it
    /// takes the place of.
    pub fn check(&self, key: &str, when: Option<&str>, row: Option<&Rule>) -> Value {
        let when_error = when.filter(|when| !when.trim().is_empty()).and_then(|when| {
            parse_when(when)
                .is_none()
                .then_some("That isn't a whole condition. Join names with !, &&, || and parentheses.")
        });
        let parsed = parse_when(when.unwrap_or_default());
        let mut unknown = Vec::new();
        if let Some(condition) = &parsed {
            condition.names(&mut unknown);
            unknown.retain(|name| !CONDITIONS.contains(&name.as_str()) && name != "true" && name != "false");
        }
        let Some(keys) = parse_keys(key) else {
            return serde_json::json!({ "caps": [], "conflicts": [], "unknown": unknown, "when_error": when_error, "valid": false });
        };
        let replaced = row.and_then(|row| compile(row).ok());
        let when_text = parsed.as_ref().map(Condition::text).unwrap_or_default();
        let rules = self.merged();
        let conflicts = self.conflicts(&rules, &keys, &when_text, |other| {
            replaced.as_ref().is_some_and(|replaced| replaced.same(other))
        });
        serde_json::json!({
            "caps": self.caps(&keys),
            "conflicts": conflicts,
            "unknown": unknown,
            "when_error": when_error,
            "valid": when_error.is_none(),
        })
    }

    pub fn view(&self) -> View {
        let rules = self.merged();
        let active = rules
            .iter()
            .map(|rule| ActiveRule {
                command: rule.command.clone(),
                shortcut: self.shortcut(&rule.keys),
                when: rule.when.clone(),
                caps: self.caps(&rule.keys),
            })
            .collect();
        let changed: Vec<&str> = self.custom.iter().map(Rule::named).collect();
        let mut groups: Vec<Group> =
            GROUPS.iter().map(|title| Group { title: title.to_string(), rows: Vec::new() }).collect();
        for &(command, label, group) in COMMANDS {
            let defaults = defaults_of(command);
            let resettable = changed.contains(&command) && !defaults.is_empty();
            let own: Vec<&Compiled> = rules.iter().filter(|rule| rule.command == command).collect();
            if own.is_empty() {
                groups[group].rows.push(Row {
                    id: format!("{command}\u{0}"),
                    command: command.into(),
                    label: label.into(),
                    key: String::new(),
                    caps: Vec::new(),
                    when: String::new(),
                    custom: false,
                    resettable,
                    conflicts: Vec::new(),
                });
                continue;
            }
            for rule in own {
                let when = rule.when_text();
                groups[group].rows.push(Row {
                    id: format!("{command}\u{0}{}\u{0}{when}", rule.key),
                    command: command.into(),
                    label: label.into(),
                    key: rule.key.clone(),
                    caps: self.caps(&rule.keys),
                    conflicts: self.conflicts(&rules, &rule.keys, &when, |other| other.same(rule)),
                    when,
                    custom: !defaults.iter().any(|default| default.same(rule)),
                    resettable,
                });
            }
        }
        View {
            path: self.path.display().to_string(),
            rules: active,
            groups,
            commands: COMMANDS
                .iter()
                .map(|&(id, label, _)| CommandInfo { id: id.into(), label: label.into() })
                .collect(),
            conditions: CONDITIONS.iter().map(|name| name.to_string()).collect(),
            issues: self.issues.clone(),
        }
    }

    fn compile_new(&self, command: &str, key: &str, when: Option<&str>) -> Result<Compiled, String> {
        if label_of(command).is_none() {
            return Err(format!("There is no command {command}."));
        }
        let when = when.map(str::trim).filter(|when| !when.is_empty());
        compile(&Rule::new(key, command, when))
    }

    /// The defaults the file leaves alone, then the file's rules.
    fn merged(&self) -> Vec<Compiled> {
        let changed: Vec<&str> = self.custom.iter().map(Rule::named).collect();
        let mut rules: Vec<Compiled> = DEFAULTS
            .iter()
            .filter(|(_, command, _)| !changed.contains(command))
            .filter_map(|&(key, command, when)| compile(&Rule::new(key, command, when)).ok())
            .collect();
        rules.extend(self.custom.iter().filter(|rule| rule.removes().is_none()).filter_map(|rule| compile(rule).ok()));
        let excess = rules.len().saturating_sub(MAX_RULES);
        rules.drain(..excess);
        rules
    }

    fn rules_of(&self, command: &str) -> Vec<Compiled> {
        self.merged().into_iter().filter(|rule| rule.command == command).collect()
    }

    /// Makes the command's rules these, keeping the file to what differs from the defaults.
    fn write_command(&mut self, command: &str, rules: Vec<Compiled>) -> Result<(), String> {
        let defaults = defaults_of(command);
        let mut custom: Vec<Rule> = self.custom.iter().filter(|rule| rule.named() != command).cloned().collect();
        let unchanged =
            rules.len() == defaults.len() && rules.iter().zip(&defaults).all(|(rule, default)| rule.same(default));
        if !unchanged {
            if rules.is_empty() {
                custom.push(Rule { key: String::new(), command: format!("-{command}"), when: None });
            }
            custom.extend(rules.iter().map(|rule| Rule {
                key: rule.key.clone(),
                command: rule.command.clone(),
                when: rule.when.as_ref().map(Condition::text),
            }));
        }
        let excess = custom.len().saturating_sub(MAX_RULES);
        custom.drain(..excess);
        let json = serde_json::to_string_pretty(&custom).map_err(|error| error.to_string())?;
        let temporary = self.path.with_extension("json.tmp");
        std::fs::write(&temporary, format!("{json}\n"))
            .and_then(|()| std::fs::rename(&temporary, &self.path))
            .map_err(|error| format!("{} can't be written: {error}", self.path.display()))?;
        self.custom = custom;
        self.issues.clear();
        Ok(())
    }

    /// The labels of the commands other rules on the same keys run where `when` can also hold.
    fn conflicts(
        &self,
        rules: &[Compiled],
        keys: &Keys,
        when: &str,
        itself: impl Fn(&Compiled) -> bool,
    ) -> Vec<String> {
        let shortcut = self.shortcut(keys);
        let mut labels: Vec<String> = rules
            .iter()
            .filter(|other| !itself(other) && self.shortcut(&other.keys) == shortcut)
            .filter(|other| {
                let other_when = other.when_text();
                when.is_empty() || other_when.is_empty() || other_when == when
            })
            .filter_map(|other| label_of(&other.command).map(String::from))
            .collect();
        labels.sort();
        labels.dedup();
        labels
    }

    fn shortcut(&self, keys: &Keys) -> Shortcut {
        Shortcut {
            key: keys.key.clone(),
            command: keys.command || (keys.primary && self.apple),
            control: keys.control || (keys.primary && !self.apple),
            option: keys.option,
            shift: keys.shift,
        }
    }

    /// The keys as the system draws them: ⌃⌥⇧⌘ before the key on Apple's systems, Ctrl+Alt+Shift elsewhere.
    fn caps(&self, keys: &Keys) -> Vec<String> {
        let shortcut = self.shortcut(keys);
        let mut caps = Vec::new();
        let held = [
            (shortcut.control, "⌃", "Ctrl"),
            (shortcut.option, "⌥", "Alt"),
            (shortcut.shift, "⇧", "Shift"),
            (shortcut.command, "⌘", "Meta"),
        ];
        for (down, apple, other) in held {
            if down {
                caps.push(if self.apple { apple } else { other }.to_string());
            }
        }
        caps.push(key_cap(&shortcut.key, self.apple));
        caps
    }
}

fn label_of(command: &str) -> Option<&'static str> {
    COMMANDS.iter().find(|(id, _, _)| *id == command).map(|(_, label, _)| *label)
}

fn defaults_of(command: &str) -> Vec<Compiled> {
    DEFAULTS
        .iter()
        .filter(|(_, default, _)| *default == command)
        .filter_map(|&(key, command, when)| compile(&Rule::new(key, command, when)).ok())
        .collect()
}

fn read(path: &Path) -> (Vec<Rule>, Vec<String>) {
    let Ok(text) = std::fs::read_to_string(path) else { return (Vec::new(), Vec::new()) };
    if text.trim().is_empty() {
        return (Vec::new(), Vec::new());
    }
    let entries: Vec<Value> = match serde_json::from_str(&text) {
        Ok(Value::Array(entries)) => entries,
        Ok(_) => return (Vec::new(), vec!["The file isn't a list of rules, so the default keys are used.".into()]),
        Err(error) => {
            return (Vec::new(), vec![format!("The file can't be read, so the default keys are used: {error}")]);
        }
    };
    let mut rules = Vec::new();
    let mut issues = Vec::new();
    for (index, entry) in entries.into_iter().enumerate() {
        let place = format!("Rule {}", index + 1);
        let rule: Rule = match serde_json::from_value(entry) {
            Ok(rule) => rule,
            Err(error) => {
                issues.push(format!("{place} is skipped: {error}"));
                continue;
            }
        };
        if label_of(rule.named()).is_none() {
            issues.push(format!("{place} is skipped: there is no command {}.", rule.named()));
            continue;
        }
        if rule.removes().is_none()
            && let Err(error) = compile(&rule)
        {
            issues.push(format!("{place} is skipped: {error}"));
            continue;
        }
        rules.push(rule);
    }
    let excess = rules.len().saturating_sub(MAX_RULES);
    rules.drain(..excess);
    (rules, issues)
}

fn compile(rule: &Rule) -> Result<Compiled, String> {
    if rule.key.len() > MAX_KEY {
        return Err("its key is too long.".into());
    }
    let keys = parse_keys(&rule.key).ok_or_else(|| format!("{:?} isn't a key.", rule.key))?;
    let when = match rule.when.as_deref().map(str::trim).filter(|when| !when.is_empty()) {
        None => None,
        Some(when) if when.len() > MAX_WHEN => return Err("its condition is too long.".into()),
        Some(when) => Some(parse_when(when).ok_or_else(|| format!("{when:?} isn't a condition."))?),
    };
    Ok(Compiled { command: rule.command.clone(), key: key_text(&keys), keys, when })
}

/// Reads `mod+shift+k`: modifiers and one key, joined by `+`.
fn parse_keys(text: &str) -> Option<Keys> {
    let lowered = text.trim().to_lowercase();
    if lowered.is_empty() {
        return None;
    }
    let mut parts: Vec<&str> = lowered.split('+').map(str::trim).collect();
    // A trailing `+` is the plus key: `mod++`.
    if lowered.ends_with('+') {
        while parts.last() == Some(&"") {
            parts.pop();
        }
        parts.push("+");
    }
    let mut keys =
        Keys { key: String::new(), primary: false, command: false, control: false, option: false, shift: false };
    for part in parts {
        match part {
            "" => return None,
            "mod" => keys.primary = true,
            "cmd" | "command" | "meta" => keys.command = true,
            "ctrl" | "control" => keys.control = true,
            "alt" | "option" | "opt" => keys.option = true,
            "shift" => keys.shift = true,
            key => {
                if !keys.key.is_empty() {
                    return None;
                }
                keys.key = key_name(key)?;
            }
        }
    }
    (!keys.key.is_empty()).then_some(keys)
}

/// The name a key is kept by, from what may be written for it.
fn key_name(key: &str) -> Option<String> {
    let name = match key {
        "esc" | "escape" => "escape",
        "return" | "enter" => "enter",
        "space" | " " => "space",
        "up" | "arrowup" => "arrowup",
        "down" | "arrowdown" => "arrowdown",
        "left" | "arrowleft" => "arrowleft",
        "right" | "arrowright" => "arrowright",
        "backspace" => "backspace",
        "delete" | "del" => "delete",
        "tab" | "home" | "end" | "pageup" | "pagedown" => key,
        _ if key.chars().count() == 1 => key,
        _ if key.len() <= 3
            && key.starts_with('f')
            && key[1..].parse::<u8>().is_ok_and(|number| (1..=20).contains(&number)) =>
        {
            key
        }
        _ => return None,
    };
    Some(name.to_string())
}

fn key_text(keys: &Keys) -> String {
    let mut parts = Vec::new();
    for (down, name) in [
        (keys.primary, "mod"),
        (keys.command, "cmd"),
        (keys.control, "ctrl"),
        (keys.option, "alt"),
        (keys.shift, "shift"),
    ] {
        if down {
            parts.push(name);
        }
    }
    parts.push(&keys.key);
    parts.join("+")
}

fn key_cap(key: &str, apple: bool) -> String {
    let cap = match (key, apple) {
        ("enter", true) => "↩",
        ("tab", true) => "⇥",
        ("backspace", true) => "⌫",
        ("delete", true) => "⌦",
        ("escape", true) => "esc",
        ("arrowup", _) => "↑",
        ("arrowdown", _) => "↓",
        ("arrowleft", _) => "←",
        ("arrowright", _) => "→",
        ("pageup", true) => "⇞",
        ("pagedown", true) => "⇟",
        ("home", true) => "↖",
        ("end", true) => "↘",
        ("space", _) => "Space",
        ("escape", false) => "Esc",
        ("pageup", false) => "PgUp",
        ("pagedown", false) => "PgDn",
        _ => {
            let mut chars = key.chars();
            let first = chars.next().map(|first| first.to_uppercase().collect::<String>()).unwrap_or_default();
            return first + chars.as_str();
        }
    };
    cap.to_string()
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Name(String),
    Not,
    And,
    Or,
    Open,
    Close,
}

fn tokens(text: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut rest = text;
    while let Some(first) = rest.chars().next() {
        if first.is_whitespace() {
            rest = &rest[first.len_utf8()..];
            continue;
        }
        let (token, length) = if rest.starts_with("&&") {
            (Token::And, 2)
        } else if rest.starts_with("||") {
            (Token::Or, 2)
        } else if first == '!' {
            (Token::Not, 1)
        } else if first == '(' {
            (Token::Open, 1)
        } else if first == ')' {
            (Token::Close, 1)
        } else if first.is_ascii_alphabetic() || first == '_' {
            let length =
                rest.find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))).unwrap_or(rest.len());
            (Token::Name(rest[..length].to_string()), length)
        } else {
            return None;
        };
        tokens.push(token);
        rest = &rest[length..];
    }
    Some(tokens)
}

/// Reads a condition: names joined by `!`, `&&` and `||`, with `&&` before `||`, and parentheses.
fn parse_when(text: &str) -> Option<Condition> {
    let tokens = tokens(text)?;
    let mut parser = Parser { tokens, at: 0 };
    let condition = parser.or(0)?;
    (parser.at == parser.tokens.len()).then_some(condition)
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
}

impl Parser {
    fn next_is(&self, token: &Token) -> bool {
        self.tokens.get(self.at) == Some(token)
    }

    fn or(&mut self, depth: usize) -> Option<Condition> {
        let mut left = self.and(depth)?;
        while self.next_is(&Token::Or) {
            self.at += 1;
            left = Condition::Or { left: Box::new(left), right: Box::new(self.and(depth)?) };
        }
        Some(left)
    }

    fn and(&mut self, depth: usize) -> Option<Condition> {
        let mut left = self.not(depth)?;
        while self.next_is(&Token::And) {
            self.at += 1;
            left = Condition::And { left: Box::new(left), right: Box::new(self.not(depth)?) };
        }
        Some(left)
    }

    fn not(&mut self, depth: usize) -> Option<Condition> {
        let mut nots = 0;
        while self.next_is(&Token::Not) {
            self.at += 1;
            nots += 1;
            if nots > MAX_DEPTH {
                return None;
            }
        }
        let mut condition = self.primary(depth)?;
        for _ in 0..nots {
            condition = Condition::Not { of: Box::new(condition) };
        }
        Some(condition)
    }

    fn primary(&mut self, depth: usize) -> Option<Condition> {
        if depth > MAX_DEPTH {
            return None;
        }
        match self.tokens.get(self.at)?.clone() {
            Token::Name(name) => {
                self.at += 1;
                Some(Condition::Name { name })
            }
            Token::Open => {
                self.at += 1;
                let inner = self.or(depth + 1)?;
                if !self.next_is(&Token::Close) {
                    return None;
                }
                self.at += 1;
                Some(inner)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded(file: Option<&str>) -> (tempfile::TempDir, Keybindings) {
        let dir = tempfile::tempdir().unwrap();
        if let Some(file) = file {
            std::fs::write(dir.path().join(FILE), file).unwrap();
        }
        let keybindings = Keybindings::load(dir.path(), "macos");
        (dir, keybindings)
    }

    fn keys_of(view: &View, command: &str) -> Vec<String> {
        view.groups
            .iter()
            .flat_map(|group| &group.rows)
            .filter(|row| row.command == command)
            .map(|row| row.key.clone())
            .collect()
    }

    fn saved(dir: &tempfile::TempDir) -> Value {
        serde_json::from_str(&std::fs::read_to_string(dir.path().join(FILE)).unwrap()).unwrap()
    }

    #[test]
    fn every_default_compiles_and_names_a_command() {
        for &(key, command, when) in DEFAULTS {
            assert!(label_of(command).is_some(), "{command}");
            compile(&Rule::new(key, command, when)).unwrap_or_else(|error| panic!("{key} {command}: {error}"));
        }
    }

    #[test]
    fn keys_are_written_one_way() {
        let written = |text: &str| parse_keys(text).map(|keys| key_text(&keys));
        assert_eq!(written("Shift+Mod+K"), Some("mod+shift+k".into()));
        assert_eq!(written("cmd+option+return"), Some("cmd+alt+enter".into()));
        assert_eq!(written("mod++"), Some("mod++".into()));
        assert_eq!(written("ctrl+up"), Some("ctrl+arrowup".into()));
        assert_eq!(written("f12"), Some("f12".into()));
        assert_eq!(written("mod+k+j"), None);
        assert_eq!(written("mod+shift"), None);
        assert_eq!(written("mod+banana"), None);
        assert_eq!(written(""), None);
    }

    #[test]
    fn mod_is_command_on_apple_systems_and_control_elsewhere() {
        let keys = parse_keys("mod+shift+k").unwrap();
        let mac = Keybindings { path: PathBuf::new(), apple: true, custom: vec![], issues: vec![] };
        let other = Keybindings { path: PathBuf::new(), apple: false, custom: vec![], issues: vec![] };
        assert!(mac.shortcut(&keys).command && !mac.shortcut(&keys).control);
        assert!(other.shortcut(&keys).control && !other.shortcut(&keys).command);
        assert_eq!(mac.caps(&keys), vec!["⇧", "⌘", "K"]);
        assert_eq!(other.caps(&keys), vec!["Ctrl", "Shift", "K"]);
    }

    #[test]
    fn conditions_bind_and_before_or_and_are_written_back() {
        let text = |when: &str| parse_when(when).map(|condition| condition.text());
        assert_eq!(text("a || b && !c"), Some("a || (b && !c)".into()));
        assert_eq!(text("!(a || b)"), Some("!(a || b)".into()));
        assert_eq!(text("  composerFocus&&turnRunning "), Some("composerFocus && turnRunning".into()));
        assert_eq!(text("a &&"), None);
        assert_eq!(text("(a"), None);
        assert_eq!(text("a b"), None);
        assert_eq!(text("a == b"), None);
        assert_eq!(text(&"!".repeat(MAX_DEPTH + 1)), None);
    }

    #[test]
    fn without_a_file_every_command_has_its_defaults() {
        let (dir, keybindings) = loaded(None);
        let view = keybindings.view();
        assert_eq!(keys_of(&view, "chat.new"), vec!["mod+n", "mod+shift+o"]);
        assert_eq!(keys_of(&view, "rightPanel.linear"), vec![""]);
        assert_eq!(view.rules.len(), DEFAULTS.len());
        assert!(view.issues.is_empty());
        assert!(!dir.path().join(FILE).exists());
    }

    #[test]
    fn a_command_in_the_file_loses_its_defaults_and_wins_over_them() {
        let (_dir, keybindings) = loaded(Some(r#"[{"key": "mod+k", "command": "chat.new"}]"#));
        let view = keybindings.view();
        assert_eq!(keys_of(&view, "chat.new"), vec!["mod+k"]);
        let last = view.rules.iter().rev().find(|rule| rule.shortcut.key == "k" && rule.shortcut.command).unwrap();
        assert_eq!(last.command, "chat.new");
        let row = view.groups.iter().flat_map(|group| &group.rows).find(|row| row.command == "chat.new").unwrap();
        assert!(row.custom && row.resettable);
        assert_eq!(row.conflicts, vec!["Commands"]);
    }

    #[test]
    fn a_removed_command_has_no_keys() {
        let (_dir, keybindings) = loaded(Some(r#"[{"command": "-thread.stop"}]"#));
        let view = keybindings.view();
        assert_eq!(keys_of(&view, "thread.stop"), vec![""]);
        assert!(view.rules.iter().all(|rule| rule.command != "thread.stop"));
    }

    #[test]
    fn broken_rules_are_skipped_and_said() {
        let (_dir, keybindings) = loaded(Some(
            r#"[{"key": "mod+j", "command": "sidebar.toggle"}, {"key": "mod+j", "command": "terminal.toggle"},
                {"key": "mod+k+j", "command": "chat.new"}, {"key": "j", "command": "chat.new", "when": "a &&"}, 7]"#,
        ));
        let view = keybindings.view();
        assert_eq!(keys_of(&view, "sidebar.toggle"), vec!["mod+j"]);
        assert_eq!(keys_of(&view, "chat.new"), vec!["mod+n", "mod+shift+o"]);
        assert_eq!(view.issues.len(), 4);
    }

    #[test]
    fn a_file_that_isnt_a_list_leaves_the_defaults() {
        let (_dir, keybindings) = loaded(Some("{ nope"));
        let view = keybindings.view();
        assert_eq!(keys_of(&view, "sidebar.toggle"), vec!["mod+b"]);
        assert_eq!(view.issues.len(), 1);
    }

    #[test]
    fn changing_a_key_writes_only_what_differs_from_the_defaults() {
        let (dir, mut keybindings) = loaded(None);
        let old = Rule::new("mod+shift+o", "chat.new", None);
        keybindings.set("chat.new", "mod+alt+o", None, Some(&old)).unwrap();
        assert_eq!(keys_of(&keybindings.view(), "chat.new"), vec!["mod+n", "mod+alt+o"]);
        assert_eq!(
            saved(&dir),
            serde_json::json!([{"key": "mod+n", "command": "chat.new"}, {"key": "mod+alt+o", "command": "chat.new"}])
        );

        keybindings.set("chat.new", "mod+shift+o", None, Some(&Rule::new("mod+alt+o", "chat.new", None))).unwrap();
        assert_eq!(saved(&dir), serde_json::json!([]));
        assert!(
            !keybindings.view().groups.iter().flat_map(|group| &group.rows).any(|row| row.custom || row.resettable)
        );
    }

    #[test]
    fn removing_every_key_of_a_command_keeps_it_without_one() {
        let (dir, mut keybindings) = loaded(None);
        keybindings.remove(&Rule::new("mod+.", "thread.stop", None)).unwrap();
        assert_eq!(saved(&dir), serde_json::json!([{"command": "-thread.stop"}]));
        assert_eq!(keys_of(&keybindings.view(), "thread.stop"), vec![""]);

        keybindings.reset("thread.stop").unwrap();
        assert_eq!(saved(&dir), serde_json::json!([]));
        assert_eq!(keys_of(&keybindings.view(), "thread.stop"), vec!["mod+."]);
    }

    #[test]
    fn a_key_for_a_command_without_defaults_is_kept_with_its_condition() {
        let (dir, mut keybindings) = loaded(None);
        keybindings.set("rightPanel.linear", "mod+alt+l", Some(" rightPanelOpen&&!settingsOpen "), None).unwrap();
        assert_eq!(
            saved(&dir),
            serde_json::json!([{"key": "mod+alt+l", "command": "rightPanel.linear", "when": "rightPanelOpen && !settingsOpen"}])
        );
        assert!(keybindings.set("nothing.here", "mod+j", None, None).is_err());
        assert!(keybindings.set("chat.new", "mod+j", Some("a ||"), None).is_err());
    }

    #[test]
    fn checking_a_key_reads_it_and_finds_what_it_clashes_with() {
        let (_dir, keybindings) = loaded(None);
        let check = keybindings.check("mod+k", None, None);
        assert_eq!(check["caps"], serde_json::json!(["⌘", "K"]));
        assert_eq!(check["conflicts"], serde_json::json!(["Commands"]));
        let replacing = keybindings.check("mod+k", None, Some(&Rule::new("mod+k", "commandPalette.toggle", None)));
        assert_eq!(replacing["conflicts"], serde_json::json!([]));
        let elsewhere = keybindings.check("c", Some("composerFocus"), None);
        assert_eq!(elsewhere["conflicts"], serde_json::json!([]));
        let unknown = keybindings.check("mod+j", Some("terminalFocus || composerFocus"), None);
        assert_eq!(unknown["unknown"], serde_json::json!(["terminalFocus"]));
        assert_eq!(keybindings.check("mod+j", Some("a &&"), None)["valid"], false);
    }

    #[test]
    fn a_file_edited_by_hand_is_read_again() {
        let (dir, mut keybindings) = loaded(None);
        assert!(!keybindings.reload());
        std::fs::write(dir.path().join(FILE), r#"[{"key": "mod+j", "command": "sidebar.toggle"}]"#).unwrap();
        assert!(keybindings.reload());
        assert_eq!(keys_of(&keybindings.view(), "sidebar.toggle"), vec!["mod+j"]);
    }
}
