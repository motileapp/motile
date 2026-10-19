//! The transcript: a column of rows of which only the ones on screen are laid out. It follows
//! the end as the thread grows, gliding after a reply that streams, asks for earlier turns as it
//! nears the first row, and lets go of the turns far above while it rests on the end.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::base::{ScrollBounce, ScrollbarHandle};
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::api::Command;

use super::model::{FADE, Item, RowModel, Transcript};
use super::rows::{self, META_FADE, RowContext};
use crate::store::Store;
use crate::theme::{self, ControlSize, colors};
use crate::ui::icons;

/// Room above the first row, which the transcript fades out in.
pub const TOP_PADDING: f32 = 20.;
/// The room under the composer, which its box doesn't reach.
const COMPOSER_BOTTOM_GAP: f32 = 16.;
/// The jump button floats this far above the composer, or above the transcript's end without one.
const JUMP_BUTTON_GAP: f32 = 10.;
const JUMP_BUTTON_SIDE: f32 = 32.;
/// How close to the end the viewport has to be to go to the end when the room under the rows grows.
const NEAR_END: f32 = 80.;

pub struct TranscriptView {
    store: Entity<Store>,
    model: Entity<Transcript>,
    /// This transcript is the one of an agent the thread started, shown in the thread's place.
    agent: bool,
    /// The tool calls opened here, to show their input and output.
    expanded: HashSet<String>,
    requested_highlight: HashSet<String>,
    /// The rows that came into view needing highlighting, to ask for after the frame.
    needs_highlight: Rc<RefCell<Vec<String>>>,
    copied: Option<String>,
    copied_task: Option<Task<()>>,
    /// The item of the list the pointer is over, whose message or reply shows its time and copy
    /// button.
    hovered: Option<Item>,
    /// The rows whose time and copy button are fading in or out, and when they set out.
    meta_fades: HashMap<String, (bool, Instant)>,
    /// Whether earlier turns are asked for, or turns far above let go of, until the core answers.
    loading_earlier: bool,
    /// The way back at the top of the list is in sight, so the one at the end leaves its room empty.
    top_in_sight: bool,
    trimming: bool,
    /// Room left under the last row for what floats over the transcript's end.
    bottom_inset: f32,
    /// The rows changed: once they are laid out, whether to ask for earlier turns or let go of
    /// the ones far above is looked at again.
    measures_after_layout: bool,
    /// Moves the viewport to the end with every frame while a reply streams.
    glide: Option<Task<()>>,
    _ticks: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl TranscriptView {
    pub fn new(store: Entity<Store>, model: Entity<Transcript>, agent: bool, cx: &mut Context<Self>) -> Self {
        let subscriptions = vec![cx.observe(&model, |this, model, cx| {
            if model.read(cx).rows.is_empty() {
                this.expanded.clear();
                this.hovered = None;
                this.meta_fades.clear();
                this.requested_highlight.clear();
                this.loading_earlier = false;
                this.trimming = false;
                this.glide = None;
            }
            this.follow_stream(cx);
            this.measures_after_layout = true;
            cx.notify();
        })];
        // The times of what runs go on, once a second.
        let ticks = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let Ok(ticks) = this.read_with(cx, |this, cx| this.model.read(cx).ticks()) else { break };
                if ticks && this.update(cx, |_, cx| cx.notify()).is_err() {
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
            hovered: None,
            meta_fades: HashMap::new(),
            loading_earlier: false,
            top_in_sight: false,
            trimming: false,
            bottom_inset: 0.,
            measures_after_layout: false,
            glide: None,
            _ticks: ticks,
            _subscriptions: subscriptions,
        };
        view.watch_scroll(cx);
        view
    }

    /// A scroll by the user takes the viewport from the glide, and may bring the first row near or
    /// leave many rows far above.
    fn watch_scroll(&self, cx: &mut Context<Self>) {
        let list = self.model.read(cx).list.clone();
        let view = cx.entity().downgrade();
        list.set_scroll_handler(move |event, _, cx| {
            let view = view.clone();
            let top_in_sight = event.visible_range.start <= 1;
            cx.defer(move |cx| {
                let _ = view.update(cx, |view, cx| {
                    if view.top_in_sight != top_in_sight {
                        view.top_in_sight = top_in_sight;
                        cx.notify();
                    }
                    view.glide = None;
                    view.load_earlier_if_near(cx);
                    view.trim_earlier_if_far(cx);
                });
            });
        });
    }

    fn list(&self, cx: &App) -> ListState {
        self.model.read(cx).list.clone()
    }

    fn viewport_height(&self, cx: &App) -> f32 {
        f32::from(self.list(cx).viewport_bounds().size.height)
    }

    /// How far the viewport's top is below the top of the list, and how far it can go.
    fn scroll_offsets(&self, cx: &App) -> (f32, f32) {
        let list = self.list(cx);
        let top = -f32::from(list.scroll_px_offset_for_scrollbar().y);
        let end = f32::from(list.max_offset_for_scrollbar().y);
        (top, end)
    }

    /// How far the end is below the viewport.
    fn distance_to_end(&self, cx: &App) -> f32 {
        let (top, end) = self.scroll_offsets(cx);
        (end - top).max(0.)
    }

    /// Whether the view rests on the end and follows it, or glides there.
    fn pinned(&self, cx: &App) -> bool {
        self.glide.is_some() || self.list(cx).is_following_tail()
    }

    /// Where the rows show in the window, out from under what floats over their end.
    pub fn shown_area(&self, cx: &App) -> Bounds<Pixels> {
        let bounds = self.list(cx).viewport_bounds();
        let height = (bounds.size.height - px(self.bottom_inset)).max(px(0.));
        Bounds::new(bounds.origin, size(bounds.size.width, height))
    }

    /// Scrolls a part of the rows, by its bounds in the window, into the shown area (its top, when
    /// it is taller), and keeps the rows still there. How far it scrolled them.
    pub fn reveal(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) -> Pixels {
        self.hold_still(cx);
        let area = self.shown_area(cx);
        let distance = if bounds.top() < area.top() {
            bounds.top() - area.top()
        } else {
            (bounds.bottom() - area.bottom()).max(px(0.)).min(bounds.top() - area.top())
        };
        if distance != px(0.) {
            self.list(cx).scroll_by(distance);
            cx.notify();
        }
        distance
    }

    /// Room under the last row for what floats over its end. More room near the end, as when
    /// the composer grows, keeps the end in view.
    pub fn set_bottom_inset(&mut self, inset: f32, cx: &mut Context<Self>) {
        if inset == self.bottom_inset {
            return;
        }
        let near_end = self.distance_to_end(cx) < NEAR_END;
        if inset > self.bottom_inset && near_end && !self.pinned(cx) {
            self.list(cx).set_follow_mode(FollowMode::Tail);
        }
        self.bottom_inset = inset;
        self.model.read(cx).bottom_changed();
        cx.notify();
    }

    /// While a reply streams in, the viewport glides after the end instead of jumping to it.
    fn follow_stream(&mut self, cx: &mut Context<Self>) {
        let list = self.list(cx);
        if !self.model.read(cx).animates() || !list.is_following_tail() || self.viewport_height(cx) <= 0. {
            return;
        }
        list.pause_following_tail();
        if self.glide.is_some() {
            return;
        }
        self.glide = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(16)).await;
                let Ok(gliding) = this.update(cx, |this, cx| this.glide_step(cx)) else { break };
                if !gliding {
                    break;
                }
            }
        }));
    }

    /// Goes a little over a fifth of the way in every sixtieth of a second, and never slower than
    /// two points in that time. An end further than the viewport is jumped to.
    fn glide_step(&mut self, cx: &mut Context<Self>) -> bool {
        let list = self.list(cx);
        let remaining = self.distance_to_end(cx);
        if list.is_following_tail() || remaining <= 0.5 || remaining >= self.viewport_height(cx) {
            list.set_follow_mode(FollowMode::Tail);
            self.glide = None;
            cx.notify();
            self.trim_earlier_if_far(cx);
            return false;
        }
        list.scroll_by(px((remaining * 0.22).max(2.).min(remaining)));
        cx.notify();
        true
    }

    /// Asks for the earlier turns when the first row comes within three viewports, again once
    /// they are among the rows.
    fn load_earlier_if_near(&mut self, cx: &mut Context<Self>) {
        let model = self.model.read(cx);
        if self.agent || self.loading_earlier || !model.earlier || model.rows.is_empty() {
            return;
        }
        let Some(thread_id) = model.thread_id.clone() else { return };
        let viewport = self.viewport_height(cx);
        if viewport <= 0. || self.scroll_offsets(cx).0 >= viewport * 3. {
            return;
        }
        self.loading_earlier = true;
        let view = cx.entity().downgrade();
        self.store.update(cx, |store, _| {
            store.ask(Command::LoadEarlier { thread_id }, move |_, _, cx| {
                cx.defer(move |cx| {
                    let _ = view.update(cx, |view, cx| {
                        view.loading_earlier = false;
                        view.load_earlier_if_near(cx);
                    });
                });
            });
        });
    }

    /// Lets go of the turns more than twelve viewports above the end while the view rests on it,
    /// keeping the last six viewports.
    fn trim_earlier_if_far(&mut self, cx: &mut Context<Self>) {
        if self.agent || self.trimming || self.glide.is_some() || !self.list(cx).is_following_tail() {
            return;
        }
        let viewport = self.viewport_height(cx);
        let (top, _) = self.scroll_offsets(cx);
        if viewport <= 0. || top <= viewport * 12. {
            return;
        }
        let model = self.model.read(cx);
        let Some(thread_id) = model.thread_id.clone() else { return };
        // The rows above are measured as they were shown; they are taken as alike in height.
        let first_shown = model.list.logical_scroll_top().item_ix;
        let first_kept = ((first_shown as f32) * (top - viewport * 6.) / top) as usize;
        let Some(kept_row) = model.items[first_kept.min(model.items.len().saturating_sub(1))..]
            .iter()
            .find_map(|item| if let Item::Row(row) = item { Some(*row) } else { None })
        else {
            return;
        };
        let keep_rows = model.rows.len() - kept_row;
        self.trimming = true;
        let view = cx.entity().downgrade();
        self.store.update(cx, |store, _| {
            store.ask(Command::TrimEarlier { thread_id, keep_rows }, move |_, _, cx| {
                cx.defer(move |cx| {
                    let _ = view.update(cx, |view, _| view.trimming = false);
                });
            });
        });
    }

    /// The pointer came over an item of the list, or left it. Rows are stacked without room
    /// between them and are as wide as the column, so the pointer never leaves a message on
    /// its way to the message's copy button.
    fn set_hovered(&mut self, item: Item, hovered: bool, cx: &mut Context<Self>) {
        let before = self.meta_row(cx);
        if hovered {
            self.hovered = Some(item);
        } else if self.hovered.as_ref() == Some(&item) {
            self.hovered = None;
        }
        let after = self.meta_row(cx);
        if after == before {
            return;
        }
        let now = Instant::now();
        self.meta_fades.retain(|_, (_, at)| at.elapsed() < META_FADE);
        if let Some(before) = before {
            self.meta_fades.insert(before, (false, now));
        }
        if let Some(after) = after {
            self.meta_fades.insert(after, (true, now));
        }
        cx.notify();
    }

    /// The row whose time and copy button the pointer reveals.
    fn meta_row(&self, cx: &App) -> Option<String> {
        let item = self.hovered.as_ref()?;
        self.model.read(cx).meta_row(item)
    }

    /// Shows the check mark for a moment. Then, when the pointer has left, the time and the copy
    /// button fade out as they do when it leaves.
    pub fn show_copied(&mut self, key: String, cx: &mut Context<Self>) {
        self.copied = Some(key);
        self.copied_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1200)).await;
            let _ = this.update(cx, |this, cx| {
                let Some(copied) = this.copied.take() else { return };
                if this.meta_row(cx).as_deref() != Some(copied.as_str()) {
                    this.meta_fades.insert(copied, (false, Instant::now()));
                }
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
        self.hold_still(cx);
        let model = self.model.read(cx);
        if let Some(index) = model.row_index(row_id) {
            let item = model.items.iter().position(|item| *item == Item::Row(index));
            if let Some(item) = item {
                model.list.splice(item..item + 1, 1);
            }
        }
        cx.notify();
    }

    /// Opening a row shouldn't drag the view to the end: the rows on screen stay where they are,
    /// and the view follows the end again once it is back there.
    fn hold_still(&mut self, cx: &mut Context<Self>) {
        self.glide = None;
        self.list(cx).pause_following_tail();
    }

    /// Opens or closes a group or a fold, whose rows the core adds and removes.
    pub fn toggle_in_core(&mut self, row_id: &str, cx: &mut Context<Self>) {
        let Some(thread_id) = self.model.read(cx).thread_id.clone() else { return };
        self.hold_still(cx);
        let row_id = row_id.to_string();
        self.store.update(cx, |store, _| store.send(Command::ToggleRow { thread_id, row_id }));
    }

    /// Opens or closes a folder among a turn's changed files.
    pub fn toggle_folder(&mut self, entry_id: &str, cx: &mut Context<Self>) {
        let Some(thread_id) = self.model.read(cx).thread_id.clone() else { return };
        self.hold_still(cx);
        let row_id = entry_id.to_string();
        self.store.update(cx, |store, _| store.send(Command::ToggleRow { thread_id, row_id }));
    }

    pub fn scroll_to_end(&mut self, cx: &mut Context<Self>) {
        self.glide = None;
        let list = self.list(cx);
        list.set_follow_mode(FollowMode::Tail);
        list.scroll_to_end();
        self.trim_earlier_if_far(cx);
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

    /// The time and copy button of each row that is fading them in or out: whether they show,
    /// and when they set out.
    fn meta_fades(&self) -> Rc<HashMap<String, (bool, Instant)>> {
        Rc::new(
            self.meta_fades
                .iter()
                .filter(|(_, (_, at))| at.elapsed() < META_FADE)
                .map(|(id, fade)| (id.clone(), *fade))
                .collect(),
        )
    }
}

impl Render for TranscriptView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        // What the last frame found without colours is asked for now.
        if !self.needs_highlight.borrow().is_empty() {
            self.request_highlights(cx);
        }
        if std::mem::take(&mut self.measures_after_layout) {
            cx.on_next_frame(window, |this, _, cx| {
                this.load_earlier_if_near(cx);
                this.trim_earlier_if_far(cx);
            });
        }
        let viewport = self.viewport_height(cx);
        let show_jump = !self.pinned(cx)
            && !self.model.read(cx).rows.is_empty()
            && self.distance_to_end(cx) >= (viewport / 2.).max(200.);
        let started_by = match self.agent {
            true => None,
            false => self.store.read(cx).parent_thread().map(|thread| (thread.id.clone(), thread.title.clone())),
        };
        self.model.update(cx, |model, _| model.set_started_by(started_by));
        let model = self.model.read(cx);
        let list_state = model.list.clone();
        let items = model.items.clone();
        // The way back at the end only shows while the one at the top is out of sight; its room
        // stays, so that nothing moves when it does.
        let top_in_sight =
            items.get(1) == Some(&Item::StartedBy(false)) && list_state.logical_scroll_top().item_ix <= 1;
        let rows = model.rows.clone();
        let pending = model.pending.clone();
        let activity = model.activity.clone();
        let fresh = Rc::new(model.fresh_rows());
        let grown = Rc::new(model.grown_rows());
        let removed = self.store.read(cx).open_thread_removed();
        let ctx = RowContext {
            view: cx.entity().downgrade(),
            store: self.store.clone(),
            expanded: Rc::new(self.expanded.clone()),
            copied: self.copied.clone(),
            meta_row: self.meta_row(cx),
            meta_fades: self.meta_fades(),
            grown,
            rows: rows.clone(),
            width: (f32::from(list_state.viewport_bounds().size.width) - theme::CONTENT_PADDING * 2.)
                .clamp(0., theme::CONTENT_WIDTH),
            agent: self.agent,
            removed,
            thread_id: model.thread_id.clone(),
        };
        let needs_highlight = self.needs_highlight.clone();
        let requested = self.requested_highlight.clone();
        let entity = cx.entity().downgrade();
        let bottom = self.bottom_inset;

        let list = list(list_state.clone(), move |index, window, cx| {
            let Some(item) = items.get(index).cloned() else { return div().into_any_element() };
            let mut fades_in = None;
            // A press in a row keeps a selection to it, as the Mac's text view of a row does.
            let key = |model: &RowModel| Some(rows::row_key(&model.row.id));
            let (element, pressed) = match &item {
                Item::Top => return div().h(px(TOP_PADDING)).into_any_element(),
                Item::Bottom => return div().h(px(bottom)).into_any_element(),
                Item::Row(row) => match rows.get(*row) {
                    Some(model) => {
                        if model.needs_highlight() && !requested.contains(&model.row.id) {
                            needs_highlight.borrow_mut().push(model.row.id.clone());
                            let entity = entity.clone();
                            cx.defer(move |cx| {
                                let _ = entity.update(cx, |_, cx| cx.notify());
                            });
                        }
                        fades_in = fresh.get(&model.row.id).map(|at| (model.row.id.clone(), *at));
                        (rows::render_row(model, &ctx, window, cx), key(model))
                    }
                    None => (div().into_any_element(), None),
                },
                Item::Working => (rows::working_line(&activity, cx), None),
                Item::StartedBy(true) if top_in_sight => {
                    (div().h(px(ControlSize::Large.height())).into_any_element(), None)
                }
                Item::StartedBy(end) => (rows::started_by(*end, &ctx, cx), None),
                Item::Pending => match &pending {
                    Some(model) => (rows::render_row(model, &ctx, window, cx), key(model)),
                    None => (div().into_any_element(), None),
                },
            };
            let entity = entity.clone();
            let row = div().w_full().px(px(theme::CONTENT_PADDING)).flex().justify_center().child(
                div()
                    .id(("transcript-item", index))
                    .w_full()
                    .max_w(px(theme::CONTENT_WIDTH))
                    .when_some(pressed, |row, pressed| {
                        row.capture_any_mouse_down(move |event, _, cx| {
                            if event.button == MouseButton::Left {
                                crate::ui::rich_text::press_row(Some(pressed), cx);
                            }
                        })
                    })
                    .on_hover(move |hovered, _, cx| {
                        let (item, hovered) = (item.clone(), *hovered);
                        let _ = entity.update(cx, |view, cx| view.set_hovered(item, hovered, cx));
                    })
                    .child(element),
            );
            match fades_in {
                Some((id, at)) => {
                    fade_in(row, SharedString::from(format!("fresh-{id}-{:?}", at)), at).into_any_element()
                }
                None => row.into_any_element(),
            }
        })
        .size_full();
        // A trackpad stretches the rows past either end, as the Mac's scroll view does.
        let list = ScrollBounce::new("transcript-bounce", &Bounded(list_state.clone()), list).enabled(true);
        let scroller = div()
            .absolute()
            .inset_0()
            .child(Scrollbar::vertical(&list_state).id("transcript-scroller").viewport_from_layout());

        let fade_top = div().absolute().top_0().left_0().right_0().h(px(TOP_PADDING)).bg(linear_gradient(
            180.,
            linear_color_stop(c.background, 0.),
            linear_color_stop(c.background.opacity(0.), 1.),
        ));
        // The rows fade out from the middle of the composer's box down to the view's bottom.
        let composer_box = (bottom - crate::thread::COMPOSER_GAP - COMPOSER_BOTTOM_GAP).max(0.);
        let bottom_fade = COMPOSER_BOTTOM_GAP + composer_box / 2.;
        let fade_bottom = div().absolute().left_0().right_0().bottom_0().h(px(bottom_fade)).bg(linear_gradient(
            0.,
            linear_color_stop(c.background, 0.),
            linear_color_stop(c.background.opacity(0.), 1.),
        ));
        // Above the composer, which starts `COMPOSER_GAP` into the inset, or above the inset.
        let under_composer = self.agent || !removed;
        let composer = if under_composer { crate::thread::COMPOSER_GAP } else { 0. };
        let jump_bottom = bottom - composer + JUMP_BUTTON_GAP;

        div()
            .size_full()
            .relative()
            .child(list)
            .child(fade_top)
            .when(bottom > 0., |transcript| transcript.child(fade_bottom))
            .child(scroller)
            .when(show_jump, |transcript| {
                transcript.child(
                    div().absolute().left_0().right_0().bottom(px(jump_bottom)).flex().justify_center().child(
                        div()
                            .id("jump-to-end")
                            .size(px(JUMP_BUTTON_SIDE))
                            .rounded_full()
                            .bg(c.popover)
                            .hover(|button| button.bg(c.border_popover))
                            .border_1()
                            .border_color(c.border_popover)
                            .shadow(c.shadow(theme::ShadowSize::Md, theme::Opacity::Shadow))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icons::symbol("arrow-down", 12.).text_color(c.muted_foreground))
                            .tooltip(crate::ui::tooltip("Scroll to end"))
                            .on_click(cx.listener(|this, _, _, cx| this.scroll_to_end(cx))),
                    ),
                )
            })
    }
}

/// The list as its bounce sees it: at its top only once its first item is. The list counts the
/// rows above that it hasn't measured yet as no height, which would look like the top while a
/// fast scroll outruns its measuring.
#[derive(Clone)]
struct Bounded(ListState);

impl ScrollbarHandle for Bounded {
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.0.viewport_bounds()
    }

    fn offset(&self) -> Point<Pixels> {
        let offset = self.0.scroll_px_offset_for_scrollbar();
        if offset.y == px(0.) && self.0.logical_scroll_top().item_ix > 0 {
            return point(px(0.), px(-1.));
        }
        offset
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        self.0.set_offset_from_scrollbar(offset);
    }

    fn content_size(&self) -> Size<Pixels> {
        ScrollbarHandle::content_size(&self.0)
    }
}

/// The element fading in from when its row streamed in.
pub fn fade_in(element: impl IntoElement, id: SharedString, at: Instant) -> impl IntoElement {
    let left = FADE.saturating_sub(at.elapsed());
    let start = 1. - left.as_secs_f32() / FADE.as_secs_f32();
    div().child(element).with_animation(
        id,
        Animation::new(left.max(Duration::from_millis(1))),
        move |element, delta| element.opacity(start + (1. - start) * delta),
    )
}
