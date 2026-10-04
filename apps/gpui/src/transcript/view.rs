//! The transcript: a column of rows of which only the ones on screen are laid out. It follows
//! the end as the thread grows, asks for earlier turns as it nears the first row, and lets go of
//! the turns far above while it rests on the end.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::api::Command;

use super::model::{Item, Transcript};
use super::rows::{self, RowContext};
use crate::store::Store;
use crate::theme::{self, colors};
use crate::ui::icons;

/// Room above the first row, which the transcript fades out in.
pub const TOP_PADDING: f32 = 20.;
/// How many rows above the first one on screen bring the earlier turns.
const EARLIER_WITHIN: usize = 30;
/// How many rows above the end there may be before the ones further up are let go.
const KEEP_ROWS: usize = 400;

pub struct TranscriptView {
    store: Entity<Store>,
    model: Entity<Transcript>,
    /// This transcript is the one of an agent the thread started, in the side panel.
    agent: bool,
    /// The tool calls opened here, to show their input and output.
    expanded: HashSet<String>,
    requested_highlight: HashSet<String>,
    /// The rows that came into view needing highlighting, to ask for after the frame.
    needs_highlight: Rc<RefCell<Vec<String>>>,
    copied: Option<String>,
    copied_task: Option<Task<()>>,
    loading_earlier: Rc<Cell<bool>>,
    trimming: Rc<Cell<bool>>,
    /// Room left under the last row for what floats over the transcript's end.
    pub bottom_inset: f32,
    /// Whether the user has scrolled up from the end, as far as the jump button goes.
    away_from_end: Rc<Cell<bool>>,
    /// The videos played in their rows, by row.
    #[cfg(target_os = "macos")]
    players: std::collections::HashMap<String, Entity<crate::media::video::VideoPlayer>>,
    /// The rows whose video is on its way to this device.
    fetching: HashSet<String>,
    _ticks: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl TranscriptView {
    pub fn new(store: Entity<Store>, model: Entity<Transcript>, agent: bool, cx: &mut Context<Self>) -> Self {
        let subscriptions = vec![cx.observe(&model, |this, model, cx| {
            if model.read(cx).rows.is_empty() {
                #[cfg(target_os = "macos")]
                this.players.clear();
                this.fetching.clear();
                this.expanded.clear();
                this.requested_highlight.clear();
                this.loading_earlier.set(false);
                this.trimming.set(false);
            }
            cx.notify();
        })];
        // The times of what runs go on, once a second.
        let ticks = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let Ok(running) = this.read_with(cx, |this, cx| this.model.read(cx).activity.running) else { break };
                if running && this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        let view = Self {
            store,
            model,
            agent,
            expanded: HashSet::new(),
            requested_highlight: HashSet::new(),
            needs_highlight: Rc::default(),
            copied: None,
            copied_task: None,
            loading_earlier: Rc::default(),
            trimming: Rc::default(),
            bottom_inset: 0.,
            away_from_end: Rc::default(),
            #[cfg(target_os = "macos")]
            players: Default::default(),
            fetching: HashSet::new(),
            _ticks: ticks,
            _subscriptions: subscriptions,
        };
        view.watch_scroll(cx);
        view
    }

    /// Asks for the earlier turns as the first row nears the viewport, and lets go of the ones
    /// far above it while the view follows the end.
    fn watch_scroll(&self, cx: &mut Context<Self>) {
        let list = self.model.read(cx).list.clone();
        let (model, store) = (self.model.downgrade(), self.store.downgrade());
        let (loading, trimming, away) =
            (self.loading_earlier.clone(), self.trimming.clone(), self.away_from_end.clone());
        let agent = self.agent;
        let view = cx.entity().downgrade();
        list.set_scroll_handler(move |event, _, cx| {
            if away.replace(!event.is_following_tail) != !event.is_following_tail {
                let view = view.clone();
                cx.defer(move |cx| {
                    let _ = view.update(cx, |_, cx| cx.notify());
                });
            }
            if agent {
                return;
            }
            let (Some(model), Some(store)) = (model.upgrade(), store.upgrade()) else { return };
            let transcript = model.read(cx);
            let Some(thread_id) = transcript.thread_id.clone() else { return };
            let (earlier, item_count) = (transcript.earlier, transcript.items.len());
            if earlier && !loading.get() && event.visible_range.start < EARLIER_WITHIN {
                loading.set(true);
                store.update(cx, |store, _| store.send(Command::LoadEarlier { thread_id: thread_id.clone() }));
            }
            if event.is_following_tail && !trimming.get() && event.visible_range.start > KEEP_ROWS {
                trimming.set(true);
                let keep_rows = item_count.saturating_sub(event.visible_range.start) + KEEP_ROWS / 2;
                store.update(cx, |store, _| store.send(Command::TrimEarlier { thread_id, keep_rows }));
            }
        });
    }

    #[cfg(target_os = "macos")]
    pub fn player(&self, row_id: &str) -> Option<AnyView> {
        self.players.get(row_id).map(|player| player.clone().into())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn player(&self, _row_id: &str) -> Option<AnyView> {
        None
    }

    pub fn fetching(&self, row_id: &str) -> bool {
        self.fetching.contains(row_id)
    }

    /// Plays the video of the row in its place, once it is on this device. One the system's
    /// player can't play opens in the app the system has for it.
    pub fn play(&mut self, row_id: String, media_id: String, cx: &mut Context<Self>) {
        if !self.fetching.insert(row_id.clone()) {
            return;
        }
        let view = cx.entity().downgrade();
        let row = row_id.clone();
        let path = crate::media::MediaFiles::fetch_then(&self.store, &media_id, cx, move |path, cx| {
            let _ = view.update(cx, |view, cx| view.start_playing(row, path, cx));
        });
        if path.is_some() {
            self.start_playing(row_id, path, cx);
        }
        cx.notify();
    }

    fn start_playing(&mut self, row_id: String, path: Option<std::path::PathBuf>, cx: &mut Context<Self>) {
        self.fetching.remove(&row_id);
        cx.notify();
        let Some(path) = path else { return };
        if !crate::media::plays_here(&path) {
            return cx.open_with_system(&path);
        }
        #[cfg(target_os = "macos")]
        {
            let player = cx.new(|cx| crate::media::video::VideoPlayer::new(&path, cx));
            self.players.insert(row_id.clone(), player);
            let model = self.model.read(cx);
            if let Some(index) = model.row_index(&row_id)
                && let Some(item) = model.items.iter().position(|item| *item == Item::Row(index))
            {
                model.list.splice(item..item + 1, 1);
            }
        }
        #[cfg(not(target_os = "macos"))]
        cx.open_with_system(&path);
    }

    pub fn show_copied(&mut self, key: String, cx: &mut Context<Self>) {
        self.copied = Some(key);
        self.copied_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1200)).await;
            let _ = this.update(cx, |this, cx| {
                this.copied = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Opens or closes a tool call's detail, keeping the row where it is.
    pub fn toggle_expanded(&mut self, row_id: &str, cx: &mut Context<Self>) {
        if !self.expanded.remove(row_id) {
            self.expanded.insert(row_id.to_string());
        }
        let model = self.model.read(cx);
        model.list.pause_following_tail();
        if let Some(index) = model.row_index(row_id) {
            let item = model.items.iter().position(|item| *item == Item::Row(index));
            if let Some(item) = item {
                model.list.splice(item..item + 1, 1);
            }
        }
        cx.notify();
    }

    /// Opens or closes a group or a fold, whose rows the core adds and removes.
    pub fn toggle_in_core(&mut self, row_id: &str, cx: &mut Context<Self>) {
        let Some(thread_id) = self.model.read(cx).thread_id.clone() else { return };
        self.model.read(cx).list.pause_following_tail();
        let row_id = row_id.to_string();
        self.store.update(cx, |store, _| store.send(Command::ToggleRow { thread_id, row_id }));
    }

    /// Opens or closes a folder among a turn's changed files.
    pub fn toggle_folder(&mut self, entry_id: &str, _row_id: &str, cx: &mut Context<Self>) {
        let Some(thread_id) = self.model.read(cx).thread_id.clone() else { return };
        self.model.read(cx).list.pause_following_tail();
        let row_id = entry_id.to_string();
        self.store.update(cx, |store, _| store.send(Command::ToggleRow { thread_id, row_id }));
    }

    pub fn scroll_to_end(&mut self, cx: &mut Context<Self>) {
        let list = self.model.read(cx).list.clone();
        list.set_follow_mode(FollowMode::Tail);
        list.scroll_to_end();
        self.away_from_end.set(false);
        cx.notify();
    }

    /// Asks the core for the highlighting of the rows that came into view without it.
    fn request_highlights(&mut self, cx: &mut Context<Self>) {
        let waiting: Vec<String> = self.needs_highlight.borrow_mut().drain(..).collect();
        let fresh: Vec<String> = waiting.into_iter().filter(|id| self.requested_highlight.insert(id.clone())).collect();
        if fresh.is_empty() {
            return;
        }
        let Some(thread_id) = self.model.read(cx).thread_id.clone() else { return };
        self.store.update(cx, |store, _| store.send(Command::Highlight { thread_id, row_ids: fresh }));
    }
}

impl Render for TranscriptView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        // What the last frame found without colours is asked for now.
        if !self.needs_highlight.borrow().is_empty() {
            self.request_highlights(cx);
        }
        let model = self.model.read(cx);
        if !model.earlier {
            self.loading_earlier.set(false);
        }
        if model.rows.len() <= KEEP_ROWS {
            self.trimming.set(false);
        }
        let list_state = model.list.clone();
        let items = Rc::new(model.items.clone());
        let rows = Rc::new(model.rows.clone());
        let pending = model.pending.clone();
        let activity = model.activity.clone();
        let ctx = RowContext {
            view: cx.entity().downgrade(),
            store: self.store.clone(),
            expanded: Rc::new(self.expanded.clone()),
            copied: self.copied.clone(),
            rows: rows.clone(),
        };
        let needs_highlight = self.needs_highlight.clone();
        let requested = self.requested_highlight.clone();
        let entity = cx.entity().downgrade();
        let bottom = self.bottom_inset;
        // Far enough from the end that it can't be seen, and not on its way there.
        let show_jump = self.away_from_end.get() && !rows.is_empty() && model.list.is_scrolled_to_end() == Some(false);
        let top_padding = if self.agent { 12. } else { TOP_PADDING };

        let list = list(list_state, move |index, _, cx| {
            let element = match items.get(index) {
                Some(Item::Row(row)) => match rows.get(*row) {
                    Some(model) => {
                        if model.needs_highlight() && !requested.contains(&model.row.id) {
                            needs_highlight.borrow_mut().push(model.row.id.clone());
                            let entity = entity.clone();
                            cx.defer(move |cx| {
                                let _ = entity.update(cx, |_, cx| cx.notify());
                            });
                        }
                        rows::render_row(model, &ctx, cx)
                    }
                    None => div().into_any_element(),
                },
                Some(Item::Working) => rows::working_line(&activity, cx),
                Some(Item::Pending) => match &pending {
                    Some(model) => rows::render_row(model, &ctx, cx),
                    None => div().into_any_element(),
                },
                None => div().into_any_element(),
            };
            div()
                .w_full()
                .px(px(theme::CONTENT_PADDING))
                .flex()
                .justify_center()
                .child(div().w_full().max_w(px(theme::CONTENT_WIDTH)).child(element))
                .into_any_element()
        })
        .size_full()
        .pt(px(top_padding))
        .pb(px(bottom + 16.));

        let fade_top = div().absolute().top_0().left_0().right_0().h(px(top_padding)).bg(linear_gradient(
            180.,
            linear_color_stop(c.background, 0.),
            linear_color_stop(c.background.opacity(0.), 1.),
        ));
        let fade_bottom = div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(px((bottom - crate::thread::COMPOSER_GAP).max(0.)))
            .h(px(crate::thread::COMPOSER_GAP))
            .bg(linear_gradient(
                0.,
                linear_color_stop(c.background, 0.),
                linear_color_stop(c.background.opacity(0.), 1.),
            ));
        let jump_bottom = (bottom - crate::thread::COMPOSER_GAP).max(0.) + crate::thread::COMPOSER_GAP + 12.;

        div()
            .size_full()
            .relative()
            .child(list)
            .child(fade_top)
            .when(bottom > 0., |transcript| transcript.child(fade_bottom))
            .when(show_jump, |transcript| {
                transcript.child(
                    div().absolute().left_0().right_0().bottom(px(jump_bottom)).flex().justify_center().child(
                        div()
                            .id("jump-to-end")
                            .size(px(32.))
                            .rounded_full()
                            .bg(c.raised)
                            .border_1()
                            .border_color(c.strong_border)
                            .shadow(crate::ui::shadow(hsla(0., 0., 0., 0.18), 2., 8.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icons::symbol("arrow.down", 12.).text_color(c.secondary))
                            .tooltip(crate::ui::tooltip("Scroll to end"))
                            .on_click(cx.listener(|this, _, _, cx| this.scroll_to_end(cx))),
                    ),
                )
            })
    }
}
