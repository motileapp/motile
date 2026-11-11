//! An image being marked up in the viewer, as the Mac's `MarkupSession` does it, through the core
//! (`markup::markup_json`): the pointer goes in, in the image's pixels, and the scene of its marks
//! comes back to paint over it.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::*;
use motile_core::markup::markup_json;
use motile_core::render::openui::canvas::Scene;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::models::SavedMarkup;
use crate::theme::Colors;
use crate::transcript::canvas::Prepared;

/// What a mark is drawn with.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MarkupTool {
    Pen,
    Arrow,
    Box,
    Pin,
}

impl MarkupTool {
    pub const ALL: [MarkupTool; 4] = [MarkupTool::Pen, MarkupTool::Arrow, MarkupTool::Box, MarkupTool::Pin];

    fn name(self) -> &'static str {
        match self {
            MarkupTool::Pen => "pen",
            MarkupTool::Arrow => "arrow",
            MarkupTool::Box => "box",
            MarkupTool::Pin => "pin",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            MarkupTool::Pen => "pencil",
            MarkupTool::Arrow => "move-up-right",
            MarkupTool::Box => "square",
            MarkupTool::Pin => "map-pin",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            MarkupTool::Pen => "Pen",
            MarkupTool::Arrow => "Arrow",
            MarkupTool::Box => "Box",
            MarkupTool::Pin => "Pin a Note",
        }
    }

    /// The key that picks it on a keyboard.
    pub fn key(self) -> &'static str {
        match self {
            MarkupTool::Pen => "P",
            MarkupTool::Arrow => "A",
            MarkupTool::Box => "B",
            MarkupTool::Pin => "N",
        }
    }
}

/// An image marked up and written out for the composer, with what to go on from.
pub struct MarkupResult {
    pub file: PathBuf,
    /// The pins' notes, which go in the message.
    pub notes: Option<String>,
    pub saved: SavedMarkup,
}

#[derive(Deserialize, Clone, PartialEq, Debug)]
pub struct Pin {
    pub mark: usize,
    pub number: usize,
    pub note: String,
    pub ink: String,
}

#[derive(Deserialize)]
struct State {
    id: u64,
    width: f64,
    height: f64,
    scene: Scene,
    pins: Vec<Pin>,
    selected: Option<usize>,
    can_undo: bool,
    can_redo: bool,
    inks: Vec<String>,
}

thread_local! {
    static LAST_TOOL: Cell<MarkupTool> = const { Cell::new(MarkupTool::Pen) };
    static LAST_INK: RefCell<String> = RefCell::new("red".into());
}

pub struct MarkupSession {
    pub source: PathBuf,
    pub name: String,
    /// The composer's attachment it replaces, when it is one.
    pub attachment: Option<String>,
    id: u64,
    width: f64,
    height: f64,
    pub scene: Option<Arc<Prepared>>,
    pub pins: Vec<Pin>,
    pub selected: Option<usize>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub inks: Vec<String>,
    /// Whether anything was drawn since it opened, which cancelling throws away.
    pub changed: bool,
    pub finishing: bool,
    pub failed: bool,
    pub tool: MarkupTool,
    pub ink: String,
    /// The selected pin's note as it is being written.
    pub note: String,
}

impl MarkupSession {
    /// `width` and `height` are the pixels of the picture shown; `saved` goes on from an earlier markup.
    pub fn open(
        source: PathBuf,
        name: String,
        attachment: Option<String>,
        (width, height): (u32, u32),
        saved: Option<&SavedMarkup>,
    ) -> Option<Self> {
        let mut session = Self {
            source,
            name,
            attachment,
            id: 0,
            width: 1.,
            height: 1.,
            scene: None,
            pins: Vec::new(),
            selected: None,
            can_undo: false,
            can_redo: false,
            inks: Vec::new(),
            changed: false,
            finishing: false,
            failed: false,
            tool: LAST_TOOL.get(),
            ink: LAST_INK.with_borrow(Clone::clone),
            note: String::new(),
        };
        let mut open = json!({ "op": "open", "width": width, "height": height });
        if let Some(saved) = saved {
            open["marks"] = json!(saved.marks);
        }
        session.apply(&markup_json(&open.to_string())).then_some(session)
    }

    pub fn selected_pin(&self) -> Option<&Pin> {
        self.pins.iter().find(|pin| Some(pin.mark) == self.selected)
    }

    /// The image's pixels across the picture shown `shown_width` wide.
    pub fn pixels_across(&self, shown_width: f32) -> f64 {
        if shown_width <= 0. { 1. } else { self.width / shown_width as f64 }
    }

    pub fn set_tool(&mut self, tool: MarkupTool) {
        self.tool = tool;
        LAST_TOOL.set(tool);
    }

    pub fn set_ink(&mut self, ink: &str) {
        self.ink = ink.to_string();
        LAST_INK.with_borrow_mut(|last| *last = ink.to_string());
    }

    pub fn down(&mut self, [x, y]: [f64; 2], slop: f64) {
        self.commit_note();
        self.send(json!({ "op": "down", "tool": self.tool.name(), "ink": self.ink, "x": x, "y": y, "slop": slop }));
        self.changed = true;
    }

    pub fn drag(&mut self, [x, y]: [f64; 2]) {
        self.send(json!({ "op": "move", "x": x, "y": y }));
    }

    pub fn up(&mut self, [x, y]: [f64; 2]) {
        self.send(json!({ "op": "up", "x": x, "y": y }));
    }

    /// Keeps the selected pin's note and puts the pin down.
    pub fn deselect(&mut self) {
        self.commit_note();
        self.send(json!({ "op": "select" }));
    }

    pub fn remove_selected(&mut self) {
        let Some(selected) = self.selected else { return };
        self.send(json!({ "op": "remove", "mark": selected }));
    }

    pub fn undo(&mut self) {
        self.commit_note();
        self.send(json!({ "op": "undo" }));
    }

    pub fn redo(&mut self) {
        self.commit_note();
        self.send(json!({ "op": "redo" }));
    }

    /// What writes the image out with its marks into `folder`, which takes a while: run it off the
    /// main thread.
    pub fn finish(&mut self, folder: PathBuf) -> impl FnOnce() -> Option<MarkupResult> + Send + use<> {
        self.commit_note();
        self.finishing = true;
        self.failed = false;
        let (id, source, name) = (self.id, self.source.clone(), self.name.clone());
        move || {
            let ask = json!({ "op": "flatten", "id": id, "source": source, "folder": folder, "name": name });
            let answer: Value = serde_json::from_str(&markup_json(&ask.to_string())).ok()?;
            result(&answer, &source, name)
        }
    }

    fn commit_note(&mut self) {
        let Some(pin) = self.selected_pin() else { return };
        if pin.note == self.note.trim() {
            return;
        }
        let mark = pin.mark;
        self.send(json!({ "op": "note", "mark": mark, "text": self.note }));
    }

    fn send(&mut self, mut input: Value) {
        input["id"] = json!(self.id);
        self.apply(&markup_json(&input.to_string()));
    }

    fn apply(&mut self, answer: &str) -> bool {
        let Ok(state) = serde_json::from_str::<State>(answer) else { return false };
        let was_selected = self.selected;
        (self.id, self.width, self.height) = (state.id, state.width, state.height);
        self.scene = Some(Arc::new(Prepared::new(state.scene)));
        (self.pins, self.selected, self.inks) = (state.pins, state.selected, state.inks);
        (self.can_undo, self.can_redo) = (state.can_undo, state.can_redo);
        if self.selected != was_selected {
            self.note = self.selected_pin().map(|pin| pin.note.clone()).unwrap_or_default();
        }
        true
    }
}

impl Drop for MarkupSession {
    fn drop(&mut self) {
        markup_json(&json!({ "op": "close", "id": self.id }).to_string());
    }
}

/// Keeps the image as it was beside the marked one, to be marked up again.
fn result(answer: &Value, source: &Path, name: String) -> Option<MarkupResult> {
    let file = PathBuf::from(answer["file"].as_str()?);
    let marks = answer["marks"].as_str()?.to_string();
    let original = file.parent()?.join("original").join(source.file_name()?);
    std::fs::create_dir_all(original.parent()?).ok()?;
    std::fs::copy(source, &original).ok()?;
    let notes = answer["notes"].as_str().map(String::from);
    Some(MarkupResult { file, notes: notes.clone(), saved: SavedMarkup { original, name, marks, notes } })
}

/// The colour of a mark's ink.
pub fn ink_color(ink: &str, c: &Colors) -> Hsla {
    c.named(&format!("markup-{ink}")).unwrap_or(c.markup_red)
}

/// What is written on a pin of that ink.
pub fn ink_contrast(ink: &str, c: &Colors) -> Hsla {
    if ink == "yellow" || ink == "white" { c.markup_ink } else { c.markup_white }
}

/// Where the marked-up images are written.
pub fn folder() -> PathBuf {
    std::env::temp_dir().join("motile-attachments").join("markup")
}
