//! The keyboard shortcuts' page of the settings: every command under its group with its keys, to
//! record new ones, put a condition on, add more of, or give back their defaults.

use std::collections::HashSet;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::keybindings::{Group, Row};

use super::layout::{
    END_ROOM, GROUP_GAP, INSET, PAGE_PADDING, ROW_HEIGHT, TITLE_GAP, column, group, group_title, large_outset, note,
    row,
};
use crate::store::Store;
use crate::store::shortcuts::ShortcutCheck;
use crate::theme::{ControlSize, Opacity, ShadowSize, Surface, colors};
use crate::ui::control::Look;
use crate::ui::menu::{Anchor, Menu};
use crate::ui::motion;
use crate::ui::{
    ActionButton, ActionMenu, Chip, InputField, KeyCaps, Variant, card_default, card_slice, edges, icons, tooltip,
};

/// The field the new shortcut records its keys in.
const NEW_FIELD: &str = "new-shortcut";
/// The deferred priority of the condition's editor: over the page, under the menus.
const POPOVER_PRIORITY: usize = 50;
const POPOVER_WIDTH: f32 = 420.;
/// Room before a command's name and its condition in the new shortcut's row.
const COMMAND_PADDING: f32 = 10.;

/// Half the room between a row's buttons, which is as wide as the room between them and the row's
/// edge. Each button takes its half, so that a click between them still lands on one.
fn half_gap(leading: bool, trailing: bool) -> Edges<f32> {
    let half = (ROW_HEIGHT - ControlSize::Large.height()) / 4.;
    edges(0., if leading { half } else { 0. }, 0., if trailing { half } else { 0. })
}

/// A shortcut being added: its command, its keys and its condition, kept once all are right.
#[derive(Default)]
struct NewShortcut {
    command: Option<String>,
    key: String,
    when: String,
    check: ShortcutCheck,
}

/// Which shortcut's condition is being edited: a row's, or the new one's.
#[derive(Clone, PartialEq)]
enum Editing {
    Row(String),
    New,
}

/// A part of the page, which the list lays out only while it is on screen.
enum Item {
    Header,
    Issues(Vec<String>),
    New,
    Title(String),
    Row { row: Row, first: bool, last: bool },
    Empty,
}

impl Item {
    /// What the part is: a group's title, or a row, which is its command for the command's first
    /// row, as the settings' search reveals them.
    fn name(&self, first_of_command: bool) -> String {
        match self {
            Item::Header => "header".into(),
            Item::Issues(_) => "issues".into(),
            Item::New => "new".into(),
            Item::Title(title) => format!("shortcuts-{title}"),
            Item::Row { row, .. } if first_of_command => format!("shortcut-{}", row.command),
            Item::Row { row, .. } => row.id.clone(),
            Item::Empty => "empty".into(),
        }
    }

    /// What the part is and how it looks, which a list laid out again keeps its place by.
    fn key(&self, name: &str) -> String {
        match self {
            Item::Issues(issues) => format!("{name}\n{}", issues.join("\n")),
            Item::Row { first, last, .. } => format!("{name}\n{first}\n{last}"),
            _ => name.to_string(),
        }
    }
}

pub struct KeybindingsPage {
    store: Entity<Store>,
    query: Entity<InputState>,
    adding: Option<NewShortcut>,
    editing: Option<(Editing, Entity<ConditionEditor>)>,
    /// Where the label of the shortcut whose condition is edited was drawn.
    anchor: Anchor,
    /// The parts of the page, of which only those on screen are laid out.
    list: ListState,
    items: Vec<Item>,
    listed: Vec<String>,
    /// What a search picked, to reveal once the page is drawn.
    revealing: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl KeybindingsPage {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Search shortcuts"));
        let subscriptions = vec![
            cx.subscribe(&query, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.observe(&store, |this, _, cx| this.take_recorded(cx)),
            cx.on_release(|_, cx| crate::ui::popover_shown("condition-editor", false, cx)),
        ];
        Self {
            store,
            query,
            adding: None,
            editing: None,
            anchor: Anchor::default(),
            list: ListState::new(0, ListAlignment::Top, px(ROW_HEIGHT * 4.)),
            items: Vec::new(),
            listed: Vec::new(),
            revealing: None,
            _subscriptions: subscriptions,
        }
    }

    /// Shows what a search picked in the middle of the page: a group or a command's first row.
    pub fn reveal(&mut self, target: String) {
        self.revealing = Some(target);
    }

    /// Scrolls to the part, roughly at once, as the rows before it aren't laid out, and right to
    /// the middle on the next frame, once it is.
    fn scroll_to_item(&self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let rows_above = (self.list.viewport_bounds().size.height / 2. / px(ROW_HEIGHT)) as usize;
        self.list.scroll_to(ListOffset { item_ix: index.saturating_sub(rows_above), offset_in_item: px(0.) });
        cx.on_next_frame(window, move |page, _, cx| {
            let Some(bounds) = page.list.bounds_for_item(index) else { return };
            page.list.scroll_by(bounds.center().y - page.list.viewport_bounds().center().y);
            cx.notify();
        });
    }

    /// The keys a field of the page recorded.
    fn take_recorded(&mut self, cx: &mut Context<Self>) {
        let Some((field, key)) = self.store.read(cx).recorded_shortcut.clone() else { return };
        let row = row_of_field(&field, &self.store.read(cx).shortcuts.groups);
        if field.as_ref() != NEW_FIELD && row.is_none() {
            return;
        }
        self.store.update(cx, |store, _| store.recorded_shortcut = None);
        let Some(key) = key else { return cx.notify() };
        match row {
            Some(row) => self.store.update(cx, |store, cx| {
                store.set_shortcut(&row.command, &key, &row.when, Some(&row));
                cx.notify();
            }),
            None => {
                if let Some(adding) = &mut self.adding {
                    adding.key = key;
                }
                self.check_new(cx);
            }
        }
        cx.notify();
    }

    /// Reads the new shortcut's keys and condition again.
    fn check_new(&mut self, cx: &mut Context<Self>) {
        let Some(adding) = &self.adding else { return };
        if adding.key.is_empty() {
            return;
        }
        let (key, when, page) = (adding.key.clone(), adding.when.clone(), cx.weak_entity());
        let (checked_key, checked_when) = (key.clone(), when.clone());
        self.store.update(cx, |store, _| {
            store.check_shortcut(&checked_key, &checked_when, None, move |check, cx| {
                let _ = page.update(cx, |page, cx| {
                    let Some(adding) = &mut page.adding else { return };
                    if adding.key == key && adding.when == when {
                        adding.check = check;
                        cx.notify();
                    }
                });
            })
        });
    }

    fn start_adding(&mut self, command: Option<String>, cx: &mut Context<Self>) {
        self.adding = Some(NewShortcut { command, ..Default::default() });
        cx.notify();
    }

    fn edit_condition(&mut self, editing: Editing, window: &mut Window, cx: &mut Context<Self>) {
        let groups = &self.store.read(cx).shortcuts.groups;
        let (key, when, row) = match &editing {
            Editing::Row(id) => {
                let Some(row) = find_row(groups, id) else { return };
                (row.key.clone(), row.when.clone(), Some(row))
            }
            Editing::New => {
                let Some(adding) = &self.adding else { return };
                (adding.key.clone(), adding.when.clone(), None)
            }
        };
        let (store, page) = (self.store.clone(), cx.weak_entity());
        let editor = cx.new(|cx| ConditionEditor::new(store, page, key, when, row, window, cx));
        crate::ui::popover_shown("condition-editor", true, cx);
        self.editing = Some((editing, editor));
        cx.notify();
    }

    /// The condition's editor is done: with the condition to keep, or without when it was let go.
    pub fn finish_condition(&mut self, saved: Option<String>, cx: &mut Context<Self>) {
        let Some((editing, _)) = self.editing.take() else { return };
        crate::ui::popover_shown("condition-editor", false, cx);
        cx.notify();
        let Some(when) = saved else { return };
        match editing {
            Editing::New => {
                if let Some(adding) = &mut self.adding {
                    adding.when = when;
                }
                self.check_new(cx);
            }
            Editing::Row(id) => {
                let Some(row) = find_row(&self.store.read(cx).shortcuts.groups, &id) else { return };
                self.store.update(cx, |store, cx| {
                    store.set_shortcut(&row.command, &row.key, &when, Some(&row));
                    cx.notify();
                });
            }
        }
    }

    fn header(&self, adding: bool, cx: &mut Context<Self>) -> AnyElement {
        let store = self.store.clone();
        let controls = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(InputField::new(&self.query).icon("search").size(ControlSize::Large).clearable(true)),
            )
            .child(
                ActionButton::new("add-shortcut", "Add Shortcut")
                    .symbol("plus")
                    .outline()
                    .large()
                    .disabled(adding)
                    .on_click(cx.listener(|this, _, _, cx| this.start_adding(None, cx))),
            )
            .child(
                ActionButton::icon("open-keybindings", "file-braces", "Open keybindings.json")
                    .outline()
                    .large()
                    .on_click(move |_, _, cx| store.update(cx, |store, _| store.open_shortcuts_file())),
            )
            .into_any_element();
        let caption = "Click a shortcut to record new keys. Where two take the same keys, the one added last wins.";
        group("shortcuts", "Keybindings", Some(caption), false, vec![controls], cx)
    }

    fn issues(&self, issues: &[String], cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.clone();
        card_default(div().px(px(INSET)).py(px(12.)).flex().flex_col().gap(px(8.)), cx)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(icons::symbol("triangle-alert", 13.).text_color(c.warning))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .child("Some of keybindings.json is skipped"),
                    )
                    .child(
                        div().ml(px(12.)).mr(px(-large_outset())).child(
                            ActionButton::new("open-issues", "Open")
                                .outline()
                                .large()
                                .surface(Surface::Card)
                                .on_click(move |_, _, cx| store.update(cx, |store, _| store.open_shortcuts_file())),
                        ),
                    ),
            )
            .children(issues.iter().map(|issue| {
                div().text_size(px(11.5)).text_color(c.muted_foreground).child(issue.clone()).into_any_element()
            }))
            .into_any_element()
    }

    fn new_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let adding = self.adding.as_ref()?;
        let c = colors(cx);
        let shortcuts = &self.store.read(cx).shortcuts;
        let label = adding
            .command
            .as_ref()
            .and_then(|command| shortcuts.commands.iter().find(|info| &info.id == command))
            .map(|info| info.label.clone());
        let groups = shortcuts.groups.clone();
        let page = cx.weak_entity();
        let choose =
            ActionMenu::new("choose-command", label.clone().unwrap_or("Choose a Command".into()), move |_, _| {
                groups.iter().fold(Menu::new(), |menu, group| {
                    commands_of(group).into_iter().fold(menu.note(group.title.clone()), |menu, (id, label)| {
                        let page = page.clone();
                        menu.item(label, move |_, cx| {
                            let id = id.clone();
                            let _ = page.update(cx, |page, cx| {
                                if let Some(adding) = &mut page.adding {
                                    adding.command = Some(id);
                                }
                                cx.notify();
                            });
                        })
                    })
                })
            })
            .button(|button| {
                button
                    .size(ControlSize::Large)
                    .surface(Surface::Card)
                    .padding(COMMAND_PADDING)
                    .tint(label.as_ref().map(|_| c.foreground))
            });
        let editing = self.editing.as_ref().is_some_and(|(editing, _)| *editing == Editing::New);
        let condition = condition_button("new-condition", &adding.when, Some(COMMAND_PADDING))
            .on_click(cx.listener(|this, _, window, cx| this.edit_condition(Editing::New, window, cx)));
        let leading = div()
            .ml(px(8. - INSET))
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(choose)
            .child(div().relative().when(editing, |cell| cell.child(self.anchor.track())).child(condition))
            .into_any_element();
        let can_add = adding.command.is_some() && !adding.key.is_empty() && adding.check.when_error.is_none();
        let buttons = div()
            .flex()
            .items_center()
            .child(
                ShortcutField::new(NEW_FIELD, adding.check.caps.clone(), &self.store)
                    .placeholder("Record Keys")
                    .margin(half_gap(false, true)),
            )
            .child(
                ActionButton::new("add-new-shortcut", "Add")
                    .primary()
                    .large()
                    .margin(half_gap(true, true))
                    .disabled(!can_add)
                    .on_click(cx.listener(|this, _, _, cx| {
                        let Some(adding) = this.adding.take() else { return };
                        let Some(command) = adding.command else { return };
                        this.store.update(cx, |store, cx| {
                            store.set_shortcut(&command, &adding.key, &adding.when, None);
                            cx.notify();
                        });
                        cx.notify();
                    })),
            )
            .child(
                ActionButton::icon("cancel-new-shortcut", "x", "Cancel")
                    .large()
                    .surface(Surface::Card)
                    .margin(half_gap(true, false))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.adding = None;
                        cx.notify();
                    })),
            )
            .into_any_element();
        let mut trailing = conflict_mark(&adding.check.conflicts, cx).into_iter().collect::<Vec<_>>();
        trailing.push(buttons);
        let rows = vec![row(vec![leading], trailing).into_any_element()];
        Some(group("new-shortcut", "New shortcut", None, true, rows, cx))
    }

    fn shortcut_row(&self, shortcut: &Row, cx: &mut Context<Self>) -> AnyElement {
        let id = shortcut.id.clone();
        let editing = self.editing.as_ref().is_some_and(|(editing, _)| *editing == Editing::Row(id.clone()));
        let title = div()
            .id(SharedString::from(format!("label-{id}")))
            .text_size(px(13.))
            .min_w_0()
            .font_weight(FontWeight::MEDIUM)
            .child(shortcut.label.clone())
            .tooltip(tooltip(shortcut.command.clone()));
        let leading = div()
            .relative()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(2.))
            .when(editing, |cell| cell.child(self.anchor.track()))
            .child(div().flex().items_center().gap(px(6.)).min_w_0().child(title).when(shortcut.custom, |name| {
                name.child(div().flex_shrink_0().child(Chip::new("Custom").surface(Surface::Card)))
            }))
            .when(!shortcut.when.is_empty(), |cell| {
                let editing = Editing::Row(id.clone());
                cell.child(div().ml(px(-ControlSize::Small.padding())).child(
                    condition_button(SharedString::from(format!("condition-{id}")), &shortcut.when, None).on_click(
                        cx.listener(move |this, _, window, cx| this.edit_condition(editing.clone(), window, cx)),
                    ),
                ))
            })
            .into_any_element();
        let menu = self.row_menu(shortcut, cx);
        let buttons = div()
            .flex()
            .items_center()
            .child(
                ShortcutField::new(format!("field-{id}"), shortcut.caps.clone(), &self.store)
                    .margin(half_gap(false, true)),
            )
            .child(menu)
            .into_any_element();
        let mut trailing = conflict_mark(&shortcut.conflicts, cx).into_iter().collect::<Vec<_>>();
        trailing.push(buttons);
        row(vec![leading], trailing).into_any_element()
    }

    fn row_menu(&self, shortcut: &Row, cx: &mut Context<Self>) -> AnyElement {
        let has_key = !shortcut.key.is_empty();
        let (page, store, shortcut_row) = (cx.weak_entity(), self.store.clone(), shortcut.clone());
        let condition_title = if shortcut.when.is_empty() { "Add a Condition" } else { "Edit the Condition" };
        ActionMenu::icon(
            SharedString::from(format!("menu-{}", shortcut.id)),
            "ellipsis",
            "What to do with the shortcut",
            move |_, _| {
                let row = shortcut_row.clone();
                let (edit_page, add_page, store_reset, store_remove) =
                    (page.clone(), page.clone(), store.clone(), store.clone());
                let (edit_id, add_command, reset_command, removed) =
                    (row.id.clone(), row.command.clone(), row.command.clone(), row.clone());
                Menu::new()
                    .when(has_key, |menu| {
                        menu.symbol_item(condition_title, "pencil", move |window, cx| {
                            let id = edit_id.clone();
                            let _ = edit_page.update(cx, |page, cx| page.edit_condition(Editing::Row(id), window, cx));
                        })
                        .symbol_item("Add Another Shortcut", "plus", move |_, cx| {
                            let command = add_command.clone();
                            let _ = add_page.update(cx, |page, cx| page.start_adding(Some(command), cx));
                        })
                    })
                    .when(row.resettable, |menu| {
                        menu.symbol_item("Reset to Default", "undo-2", move |_, cx| {
                            store_reset.update(cx, |store, _| store.reset_shortcut(&reset_command))
                        })
                    })
                    .when(has_key, |menu| {
                        menu.separator().danger_symbol_item("Remove", "trash-2", move |_, cx| {
                            store_remove.update(cx, |store, _| store.remove_shortcut(&removed))
                        })
                    })
            },
        )
        .button(|button| {
            button
                .size(ControlSize::Large)
                .surface(Surface::Card)
                .margin(half_gap(true, false))
                .disabled(!has_key && !shortcut.resettable)
        })
        .into_any_element()
    }

    /// The rows every word of the search is found in: by name, command, keys or condition.
    fn found(&self, groups: &[Group], cx: &App) -> Vec<Group> {
        let query = self.query.read(cx).value().to_lowercase();
        let words: Vec<&str> = query.split_whitespace().collect();
        if words.is_empty() {
            return groups.to_vec();
        }
        groups
            .iter()
            .filter_map(|group| {
                let rows: Vec<Row> = group
                    .rows
                    .iter()
                    .filter(|row| {
                        let text = format!(
                            "{} {} {} {} {} {}",
                            row.label,
                            row.command,
                            crate::store::shortcuts::join_caps(&row.caps, std::env::consts::OS),
                            row.key,
                            row.when,
                            group.title
                        )
                        .to_lowercase();
                        words.iter().all(|word| text.contains(word))
                    })
                    .cloned()
                    .collect();
                (!rows.is_empty()).then(|| Group { title: group.title.clone(), rows })
            })
            .collect()
    }

    /// The condition's editor, under the label of the shortcut it is for. It fades in and out as
    /// a popover does.
    fn popover(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editor = self.editing.as_ref().map(|(_, editor)| editor.clone());
        let (editor, leaving) = motion::lingering("condition-popover", editor, motion::QUICK, window, cx)?;
        let shown =
            motion::eased_from("condition-popover-shown", 0., if leaving { 0. } else { 1. }, motion::QUICK, window, cx);
        let bounds = self.anchor.bounds();
        let viewport = crate::window_frame::content_size(window);
        let page = cx.weak_entity();
        let editor = anchored()
            .position(point(bounds.origin.x, bounds.bottom() + px(6.)))
            .snap_to_window_with_margin(px(8.))
            .child(div().opacity(shown).child(editor));
        let layer = div().id("condition-layer").w(viewport.width).h(viewport.height).when(!leaving, |layer| {
            layer.occlude().on_mouse_down(MouseButton::Left, move |_, _, cx| {
                cx.stop_propagation();
                let _ = page.update(cx, |page, cx| page.finish_condition(None, cx));
            })
        });
        let layer = layer.child(editor);
        Some(
            deferred(anchored().position(crate::window_frame::content_origin(window)).child(layer))
                .with_priority(POPOVER_PRIORITY)
                .into_any_element(),
        )
    }
}

impl KeybindingsPage {
    /// The parts of the page: the search, then each group's title and rows.
    fn items(&self, cx: &App) -> Vec<Item> {
        let shortcuts = &self.store.read(cx).shortcuts;
        let found = self.found(&shortcuts.groups, cx);
        let mut items = vec![Item::Header];
        if !shortcuts.issues.is_empty() {
            items.push(Item::Issues(shortcuts.issues.clone()));
        }
        if self.adding.is_some() {
            items.push(Item::New);
        }
        for shortcut_group in &found {
            items.push(Item::Title(shortcut_group.title.clone()));
            let last = shortcut_group.rows.len().saturating_sub(1);
            items.extend(shortcut_group.rows.iter().enumerate().map(|(place, row)| Item::Row {
                row: row.clone(),
                first: place == 0,
                last: place == last,
            }));
        }
        if found.is_empty() {
            items.push(Item::Empty);
        }
        items
    }

    fn item(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(item) = self.items.get(index) else { return div().into_any_element() };
        let part = match item {
            Item::Header => self.header(self.adding.is_some(), cx),
            Item::Issues(issues) => self.issues(issues, cx),
            Item::New => self.new_row(cx).unwrap_or_else(|| div().into_any_element()),
            Item::Title(title) => group_title(title.clone(), None, cx).pb(px(TITLE_GAP)).into_any_element(),
            Item::Row { row, first, last } => card_slice(div(), *first, *last, cx)
                .child(self.shortcut_row(row, cx))
                .when(!last, |slice| slice.child(crate::ui::divider_on(Surface::Card, cx)))
                .into_any_element(),
            Item::Empty => note("No shortcuts found", cx),
        };
        let spaced = !matches!(item, Item::Header | Item::Row { .. });
        column(part).when(spaced, |part| part.pt(px(GROUP_GAP))).into_any_element()
    }
}

impl Render for KeybindingsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.items = self.items(cx);
        let mut commands = HashSet::new();
        let names: Vec<String> = self
            .items
            .iter()
            .map(|item| item.name(matches!(item, Item::Row { row, .. } if commands.insert(row.command.clone()))))
            .collect();
        let keys = self.items.iter().zip(&names).map(|(item, name)| item.key(name)).collect();
        crate::ui::relist(&self.list, &mut self.listed, keys);
        if let Some(target) = self.revealing.take()
            && let Some(index) = names.iter().position(|name| *name == target)
        {
            self.scroll_to_item(index, window, cx);
        }
        div()
            .size_full()
            .child(
                list(self.list.clone(), cx.processor(|this, index, _, cx| this.item(index, cx)))
                    .size_full()
                    .pt(px(PAGE_PADDING))
                    .pb(px(END_ROOM)),
            )
            .children(self.popover(window, cx))
    }
}

/// The row a field records keys for.
fn row_of_field(field: &str, groups: &[Group]) -> Option<Row> {
    let id = field.strip_prefix("field-")?;
    find_row(groups, id)
}

fn find_row(groups: &[Group], id: &str) -> Option<Row> {
    groups.iter().flat_map(|group| &group.rows).find(|row| row.id == id).cloned()
}

/// Each command of the group once, with its name.
fn commands_of(group: &Group) -> Vec<(String, String)> {
    let mut seen = HashSet::new();
    group
        .rows
        .iter()
        .filter(|row| seen.insert(row.command.clone()))
        .map(|row| (row.command.clone(), row.label.clone()))
        .collect()
}

/// The condition under a shortcut's name, which opens it to edit.
fn condition_button(id: impl Into<ElementId>, when: &str, padding: Option<f32>) -> ActionButton {
    let (title, help) = if when.is_empty() {
        ("Always".to_string(), "Runs wherever the keys are pressed. Click to add a condition.")
    } else {
        (format!("When {when}"), "Runs only while this holds. Click to edit it.")
    };
    let button =
        ActionButton::new(id, title).ghost().small().surface(Surface::Card).opens(true).truncates(true).help(help);
    match padding {
        Some(padding) => button.padding(padding),
        None => button,
    }
}

/// Says which other commands take the same keys.
fn conflict_mark(conflicts: &[String], cx: &App) -> Option<AnyElement> {
    if conflicts.is_empty() {
        return None;
    }
    let help = format!("Also on {}. Where both can run, the one added last wins.", conflicts.join(", "));
    Some(
        div()
            .id(SharedString::from(format!("conflict-{}", conflicts.join(","))))
            .text_color(colors(cx).warning)
            .child(icons::symbol("triangle-alert", 13.))
            .tooltip(tooltip(help))
            .into_any_element(),
    )
}

/// The keys of a shortcut, on a button that records new ones: pressed, it takes the next keys
/// pressed with their modifiers. Esc or a click elsewhere stops it without a change.
#[derive(IntoElement)]
struct ShortcutField {
    id: SharedString,
    caps: Vec<String>,
    placeholder: SharedString,
    margin: Edges<f32>,
    store: Entity<Store>,
}

impl ShortcutField {
    fn new(id: impl Into<SharedString>, caps: Vec<String>, store: &Entity<Store>) -> Self {
        Self {
            id: id.into(),
            caps,
            placeholder: "Record Shortcut".into(),
            margin: Edges::default(),
            store: store.clone(),
        }
    }

    fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    fn margin(mut self, margin: Edges<f32>) -> Self {
        self.margin = margin;
        self
    }
}

impl RenderOnce for ShortcutField {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let size = ControlSize::Large;
        let recording = self.store.read(cx).recording_shortcut.as_ref() == Some(&self.id);
        let mut look = Look::new(Variant::Outline, size);
        look.selected = recording;
        look.surface = Surface::Card;
        let (fill, lit) = (look.fill(false, c), look.fill(true, c));
        let padding =
            if recording || self.caps.is_empty() { size.padding() } else { (size.height() - KeyCaps::HEIGHT) / 2. };
        let label: AnyElement = if recording {
            div()
                .text_size(px(size.text_size()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.primary)
                .child("Press keys")
                .into_any_element()
        } else if self.caps.is_empty() {
            div()
                .text_size(px(size.text_size()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.muted_foreground)
                .child(self.placeholder)
                .into_any_element()
        } else {
            div().text_color(c.foreground).child(KeyCaps::new(self.caps).surface(Surface::Card)).into_any_element()
        };
        let group = SharedString::from(format!("shortcut-field-{}", self.id));
        let (start, stop, id) = (self.store.clone(), self.store.clone(), self.id.clone());
        let stopped = self.id.clone();
        div()
            .id(ElementId::Name(group.clone()))
            .group(group.clone())
            .flex_shrink_0()
            .pl(px(self.margin.left))
            .pr(px(self.margin.right))
            .child(
                div()
                    .h(px(size.height()))
                    .min_w(px(size.height()))
                    .px(px(padding))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(look.radius()))
                    .bg(fill)
                    .when_some(look.border(c), |face, border| face.border_1().border_color(border))
                    .group_hover(group.clone(), move |face| face.bg(lit))
                    .child(label),
            )
            .tooltip(tooltip(if recording { "Press the keys, or Esc to stop" } else { "Record new keys" }))
            .on_click(move |_, _, cx| {
                let id = id.clone();
                start.update(cx, |store, cx| {
                    store.recording_shortcut = Some(id);
                    cx.notify();
                });
            })
            .when(recording, |field| {
                field.on_mouse_down_out(move |_, _, cx| {
                    stop.update(cx, |store, cx| {
                        if store.recording_shortcut.as_ref() == Some(&stopped) {
                            store.recording_shortcut = None;
                            cx.notify();
                        }
                    })
                })
            })
    }
}

/// Where a shortcut runs: names joined by ! for not, && for and, || for or, and parentheses.
pub struct ConditionEditor {
    store: Entity<Store>,
    page: WeakEntity<KeybindingsPage>,
    /// The keys it is checked with, for the commands they would clash with there.
    key: String,
    row: Option<Row>,
    when: Entity<InputState>,
    check: ShortcutCheck,
    _subscription: Subscription,
}

impl ConditionEditor {
    fn new(
        store: Entity<Store>,
        page: WeakEntity<KeybindingsPage>,
        key: String,
        when: String,
        row: Option<Row>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let field = cx.new(|cx| InputState::new(window, cx).placeholder("Always").default_value(when));
        field.update(cx, |field, cx| field.focus(window, cx));
        let subscription = cx.subscribe(&field, |this, _, event: &InputEvent, cx| match event {
            InputEvent::Change => this.check(cx),
            InputEvent::PressEnter { .. } => this.save(cx),
            _ => {}
        });
        let mut editor =
            Self { store, page, key, row, when: field, check: ShortcutCheck::default(), _subscription: subscription };
        editor.check(cx);
        editor
    }

    fn check(&mut self, cx: &mut Context<Self>) {
        let when = self.when.read(cx).value().to_string();
        let (key, row, editor) = (self.key.clone(), self.row.clone(), cx.weak_entity());
        self.store.update(cx, |store, _| {
            let checked = when.clone();
            store.check_shortcut(&key, &when, row.as_ref(), move |check, cx| {
                let _ = editor.update(cx, |editor, cx| {
                    if editor.when.read(cx).value().as_ref() == checked {
                        editor.check = check;
                        cx.notify();
                    }
                });
            })
        });
        cx.notify();
    }

    fn add(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let written = self.when.read(cx).value().trim().to_string();
        let when = if written.is_empty() { name.to_string() } else { format!("{written} && {name}") };
        self.when.update(cx, |field, cx| field.set_value(when, window, cx));
        self.check(cx);
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if self.check.when_error.is_some() {
            return;
        }
        let when = self.when.read(cx).value().trim().to_string();
        let _ = self.page.update(cx, |page, cx| page.finish_condition(Some(when), cx));
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        let _ = self.page.update(cx, |page, cx| page.finish_condition(None, cx));
    }
}

impl Render for ConditionEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let conditions = self.store.read(cx).shortcuts.conditions.clone();
        let issue = |text: String, symbol: &'static str, color: Hsla| {
            div()
                .flex()
                .items_start()
                .gap(px(6.))
                .text_size(px(11.5))
                .text_color(color)
                .child(div().pt(px(1.)).child(icons::symbol(symbol, 12.)))
                .child(div().flex_1().min_w_0().child(text))
        };
        let note = if let Some(error) = &self.check.when_error {
            Some(issue(error.clone(), "circle-x", c.destructive))
        } else if !self.check.unknown.is_empty() {
            let names = self.check.unknown.join(", ");
            Some(issue(format!("Motile doesn't know {names}, which never holds."), "triangle-alert", c.warning))
        } else {
            None
        };
        let editor = cx.weak_entity();
        let add = ActionMenu::new("add-condition", "Add", move |_, _| {
            let section = |menu: Menu, prefix: &'static str| {
                conditions.iter().fold(menu, |menu, name| {
                    let (editor, name) = (editor.clone(), format!("{prefix}{name}"));
                    menu.item(name.clone(), move |window, cx| {
                        let _ = editor.update(cx, |editor, cx| editor.add(&name, window, cx));
                    })
                })
            };
            section(section(Menu::new().note("While"), "").note("Unless"), "!")
        })
        .button(|button| button.symbol("plus").variant(Variant::Outline).surface(Surface::Popover));
        crate::ui::dropdown_card(div(), cx)
            .id("condition-editor")
            .w(px(POPOVER_WIDTH))
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .text_color(c.foreground)
            .shadow(c.shadow(ShadowSize::Lg, Opacity::ShadowStronger))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    this.cancel(cx);
                }
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child("When"))
                    .child(div().text_size(px(11.5)).text_color(c.muted_foreground).child(
                        "The shortcut runs only while this holds. Join names with ! for not, && for and, || for or.",
                    )),
            )
            .child(InputField::new(&self.when).size(ControlSize::Large).monospaced(true).surface(Surface::Popover))
            .children(note)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(add)
                    .child(div().flex_1())
                    .child(
                        ActionButton::new("condition-cancel", "Cancel")
                            .surface(Surface::Popover)
                            .on_click(cx.listener(|this, _, _, cx| this.cancel(cx))),
                    )
                    .child(
                        ActionButton::new("condition-save", "Save")
                            .primary()
                            .disabled(self.check.when_error.is_some())
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    ),
            )
    }
}
