use crate::{AnyElement, App, Element, IntoElement, Lowering, Window, wire};
use gpui::{Pixels, StyleRefinement, Styled, px};
use std::{
    cell::RefCell,
    ops::Range,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use gpui::{FollowMode, ListAlignment, ListOffset, ListScrollEvent, ListSizingBehavior};
type ScrollHandler = dyn FnMut(&ListScrollEvent, &mut Window, &mut App) + 'static;
type ItemRenderer = Box<dyn FnMut(usize, &mut Window, &mut App) -> AnyElement>;

#[derive(Clone)]
pub struct ListState(Rc<State>);
struct State {
    id: u64,
    inner: RefCell<Inner>,
    scroll_handler: RefCell<Option<Box<ScrollHandler>>>,
}
struct Inner {
    item_count: usize,
    alignment: ListAlignment,
    overdraw: Pixels,
    requested: Range<usize>,
    logical_scroll_top: ListOffset,
    tail_mode: bool,
    following_tail: bool,
    revision: u64,
    commands: Vec<wire::ListCommand>,
}
static NEXT_LIST_STATE: AtomicU64 = AtomicU64::new(1);

impl std::fmt::Debug for ListState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ListState")
    }
}
impl Default for ListState {
    fn default() -> Self {
        Self::new(0, ListAlignment::Top, px(0.))
    }
}

impl ListState {
    pub fn new(item_count: usize, alignment: ListAlignment, overdraw: Pixels) -> Self {
        let item_count = item_count.min(wire::MAX_LIST_ITEMS);
        Self(Rc::new(State {
            id: NEXT_LIST_STATE.fetch_add(1, Ordering::Relaxed),
            inner: RefCell::new(Inner {
                item_count,
                alignment,
                overdraw,
                requested: initial_range(item_count, alignment),
                logical_scroll_top: ListOffset {
                    item_ix: if alignment == ListAlignment::Bottom {
                        item_count
                    } else {
                        0
                    },
                    offset_in_item: px(0.),
                },
                tail_mode: false,
                following_tail: false,
                revision: 0,
                commands: Vec::new(),
            }),
            scroll_handler: RefCell::new(None),
        }))
    }
    pub fn reset(&self, element_count: usize) {
        let count = element_count.min(wire::MAX_LIST_ITEMS);
        let mut inner = self.0.inner.borrow_mut();
        inner.item_count = count;
        inner.requested = initial_range(count, inner.alignment);
        inner.logical_scroll_top = ListOffset {
            item_ix: if inner.alignment == ListAlignment::Bottom {
                count
            } else {
                0
            },
            offset_in_item: px(0.),
        };
        inner.push(wire::ListCommand::Reset { count });
    }
    pub fn remeasure(&self) {
        self.remeasure_items(0..self.item_count());
    }
    pub fn remeasure_items(&self, range: Range<usize>) {
        let mut inner = self.0.inner.borrow_mut();
        let range = bounded_range(range, inner.item_count);
        if !range.is_empty() {
            inner.push(wire::ListCommand::Remeasure {
                start: range.start,
                end: range.end,
            });
        }
    }
    pub fn item_count(&self) -> usize {
        self.0.inner.borrow().item_count
    }
    pub fn splice(&self, old_range: Range<usize>, count: usize) {
        let mut inner = self.0.inner.borrow_mut();
        let old_range = bounded_range(old_range, inner.item_count);
        let count = count.min(wire::MAX_LIST_ITEMS);
        inner.item_count = inner
            .item_count
            .saturating_sub(old_range.len())
            .saturating_add(count)
            .min(wire::MAX_LIST_ITEMS);
        inner.requested = initial_range(inner.item_count, inner.alignment);
        inner.push(wire::ListCommand::Splice {
            start: old_range.start,
            end: old_range.end,
            count,
        });
    }
    pub fn set_scroll_handler(
        &self,
        handler: impl FnMut(&ListScrollEvent, &mut Window, &mut App) + 'static,
    ) {
        *self.0.scroll_handler.borrow_mut() = Some(Box::new(handler));
    }
    pub fn logical_scroll_top(&self) -> ListOffset {
        self.0.inner.borrow().logical_scroll_top
    }
    pub fn scroll_to(&self, scroll_top: ListOffset) {
        let mut inner = self.0.inner.borrow_mut();
        let offset = ListOffset {
            item_ix: scroll_top.item_ix.min(inner.item_count),
            offset_in_item: px(f32::from(scroll_top.offset_in_item).max(0.)),
        };
        inner.logical_scroll_top = offset;
        inner.push(wire::ListCommand::ScrollTo(to_wire_offset(offset)));
    }
    pub fn scroll_to_end(&self) {
        let mut inner = self.0.inner.borrow_mut();
        inner.logical_scroll_top = ListOffset {
            item_ix: inner.item_count,
            offset_in_item: px(0.),
        };
        inner.push(wire::ListCommand::ScrollToEnd);
    }
    pub fn scroll_to_reveal_item(&self, ix: usize) {
        let mut inner = self.0.inner.borrow_mut();
        let ix = ix.min(inner.item_count.saturating_sub(1));
        inner.requested = bounded_window(ix, ix.saturating_add(1), inner.item_count);
        inner.push(wire::ListCommand::ScrollToRevealItem(ix));
    }
    pub fn set_follow_mode(&self, mode: FollowMode) {
        let tail = mode == FollowMode::Tail;
        let mut inner = self.0.inner.borrow_mut();
        if inner.tail_mode != tail {
            inner.tail_mode = tail;
            inner.following_tail = tail;
            inner.push(wire::ListCommand::SetFollowMode { tail });
        }
    }
    pub fn pause_following_tail(&self) {
        let mut inner = self.0.inner.borrow_mut();
        if inner.following_tail {
            inner.following_tail = false;
            inner.push(wire::ListCommand::PauseFollowingTail);
        }
    }
    pub fn is_following_tail(&self) -> bool {
        self.0.inner.borrow().following_tail
    }
    /// A request next to the window grows it rather than replacing it: the
    /// host asks for the rows it is missing, not for all it shows, so
    /// replacing dropped the rows on screen and the next frame asked for those
    /// back. A request away from the window is a jump, and replaces it.
    fn request(&self, request: &wire::ListRequest) {
        let mut inner = self.0.inner.borrow_mut();
        let count = inner.item_count;
        let asked = bounded_range(request.start..request.end, count);
        let held = inner.requested.clone();
        let touches = asked.start <= held.end && held.start <= asked.end;
        let (start, end) = (held.start.min(asked.start), held.end.max(asked.end));
        inner.requested = match (touches, end - start <= wire::MAX_LIST_ROWS) {
            (false, _) => bounded_window(asked.start, asked.end, count),
            (true, true) => start..end,
            (true, false) if asked.start < held.start => bounded_window(start, end, count),
            (true, false) => end - wire::MAX_LIST_ROWS..end,
        };
    }
    fn observe(&self, event: &wire::ListScroll, window: &mut Window, app: &mut App) {
        {
            let mut inner = self.0.inner.borrow_mut();
            inner.logical_scroll_top = from_wire_offset(event.offset);
            inner.following_tail = event.is_following_tail;
            // Rows well off screen leave the window, so scrolling through a
            // long list keeps its tick near a screenful rather than growing
            // to the cap. The margin outreaches the host's overdraw.
            let keep = event.visible_start.saturating_sub(MARGIN_ROWS)
                ..event.visible_end.saturating_add(MARGIN_ROWS);
            let held = inner.requested.clone();
            let trimmed = held.start.max(keep.start)..held.end.min(keep.end);
            if !trimmed.is_empty() && event.visible_start < event.visible_end {
                inner.requested = trimmed;
            }
        }
        let mut handler = self.0.scroll_handler.borrow_mut().take();
        if let Some(callback) = handler.as_mut() {
            callback(
                &ListScrollEvent {
                    visible_range: event.visible_start..event.visible_end,
                    count: event.count,
                    is_scrolled: event.is_scrolled,
                    is_following_tail: event.is_following_tail,
                },
                window,
                app,
            );
        }
        *self.0.scroll_handler.borrow_mut() = handler;
    }
}
impl Inner {
    fn push(&mut self, command: wire::ListCommand) {
        self.revision = self.revision.wrapping_add(1);
        if self.commands.len() < wire::MAX_LIST_COMMANDS {
            self.commands.push(command);
        }
    }
}

pub struct List {
    state: ListState,
    render_item: ItemRenderer,
    style: StyleRefinement,
    sizing_behavior: ListSizingBehavior,
}
pub fn list(
    state: ListState,
    render_item: impl FnMut(usize, &mut Window, &mut App) -> AnyElement + 'static,
) -> List {
    List {
        state,
        render_item: Box::new(render_item),
        style: StyleRefinement::default(),
        sizing_behavior: ListSizingBehavior::default(),
    }
}
impl List {
    pub fn with_sizing_behavior(mut self, behavior: ListSizingBehavior) -> Self {
        self.sizing_behavior = behavior;
        self
    }
}
impl Styled for List {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
impl Element for List {
    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        let Self {
            state,
            mut render_item,
            style,
            sizing_behavior,
        } = *self;
        let request_state = state.clone();
        let request_handler =
            lowering.route(move |request: &wire::ListRequest, _, _| request_state.request(request));
        let scroll_handler = state.0.scroll_handler.borrow().is_some().then(|| {
            let scroll_state = state.clone();
            lowering.route(move |event: &wire::ListScroll, window, app| {
                scroll_state.observe(event, window, app)
            })
        });
        let (item_count, alignment, overdraw, following_tail, revision, commands, range) = {
            let mut inner = state.0.inner.borrow_mut();
            (
                inner.item_count,
                inner.alignment,
                inner.overdraw,
                inner.tail_mode,
                inner.revision,
                std::mem::take(&mut inner.commands),
                inner.requested.clone(),
            )
        };
        let mut children = Vec::with_capacity(range.len().min(wire::MAX_LIST_ROWS));
        for index in range.clone().take(wire::MAX_LIST_ROWS) {
            let outer = lowering.enter_row(state.0.id << 32 ^ index as u64);
            let (window, app) = lowering.parts();
            let row = render_item(index, window, app);
            children.push(lowering.lower_element(row));
            lowering.leave_row(outer);
        }
        wire::Node::List {
            state: state.0.id,
            path: lowering.current_path().to_vec(),
            item_count,
            alignment: wire_alignment(alignment),
            overdraw: f32::from(overdraw),
            sizing: wire_sizing(sizing_behavior),
            following_tail,
            revision,
            commands,
            request_handler,
            scroll_handler,
            range_start: range.start,
            style,
            interactivity: wire::Interactivity::default(),
            children,
        }
    }
}
impl IntoElement for List {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl gpui::prelude::FluentBuilder for List {}

/// Rows a list renders before the host says what it shows: a screenful of
/// short rows. The host asks for more as they come into view — each row the
/// guest renders is paid for on every tick.
const INITIAL_ROWS: usize = 24;
/// Rows kept past each edge of what the host shows.
const MARGIN_ROWS: usize = 12;

fn initial_range(count: usize, alignment: ListAlignment) -> Range<usize> {
    match alignment {
        ListAlignment::Top => 0..count.min(INITIAL_ROWS),
        ListAlignment::Bottom => count.saturating_sub(INITIAL_ROWS)..count,
    }
}
fn bounded_range(range: Range<usize>, count: usize) -> Range<usize> {
    let start = range.start.min(count);
    start..range.end.clamp(start, count)
}
fn bounded_window(start: usize, end: usize, count: usize) -> Range<usize> {
    let start = start.min(count);
    start
        ..end
            .clamp(start, count)
            .min(start.saturating_add(wire::MAX_LIST_ROWS))
}
fn wire_alignment(v: ListAlignment) -> wire::ListAlignment {
    match v {
        ListAlignment::Top => wire::ListAlignment::Top,
        ListAlignment::Bottom => wire::ListAlignment::Bottom,
    }
}
fn wire_sizing(v: ListSizingBehavior) -> wire::ListSizingBehavior {
    match v {
        ListSizingBehavior::Infer => wire::ListSizingBehavior::Infer,
        ListSizingBehavior::Auto => wire::ListSizingBehavior::Auto,
    }
}
fn to_wire_offset(v: ListOffset) -> wire::ListOffset {
    wire::ListOffset {
        item_ix: v.item_ix,
        offset_in_item: f32::from(v.offset_in_item),
    }
}
fn from_wire_offset(v: wire::ListOffset) -> ListOffset {
    ListOffset {
        item_ix: v.item_ix,
        offset_in_item: px(v.offset_in_item.max(0.)),
    }
}

#[cfg(test)]
mod tests;
