//! Event routes and sent pictures belong to one running driver.
use std::any::Any;
use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::wire::ElementIdWire;

type TooltipHandler =
    Rc<dyn Fn(Option<usize>, &mut crate::Window, &mut crate::App) -> Option<crate::AnyView>>;

/// What a tooltip route builds: the content, or `None` from an
/// index-sensitive builder with nothing to show there.
struct TooltipRoute(TooltipHandler);
pub(crate) type TooltipBuilder =
    Box<dyn Fn(&mut crate::Window, &mut crate::App) -> crate::AnyView + 'static>;
pub(crate) type RichTextTooltipBuilder =
    Box<dyn Fn(usize, &mut crate::Window, &mut crate::App) -> Option<crate::AnyView> + 'static>;
type EventHandler<A> = Rc<dyn Fn(&A, &mut crate::Window, &mut crate::App)>;

struct EventRoute<A>(EventHandler<A>);

#[derive(Default)]
struct Tables {
    host: crate::Host,
    routes: Routes,
    tooltip_responses: Vec<crate::wire::TooltipResponse>,
    pictures: HashSet<u64>,
    /// Picture bytes this frame carries so far, against the host's
    /// [`crate::wire::MAX_PICTURE_BYTES_PER_FRAME`].
    picture_bytes: usize,
    /// A picture this frame drew went out by hash alone for want of budget.
    pictures_owed: bool,
    /// Widget commands asked this tick, sent with its frame
    /// ([`crate::window::send_widgets`]).
    widgets: Vec<crate::wire::WidgetCommand>,
}

/// What a listener is for: the field of the node it lowers into. A node
/// takes one route per field, so its routes never renumber each other; a
/// mouse button is inside the field (`on_mouse_down` runs for every button
/// it was given) and a dispatch phase is a field of its own (`capture_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Kind {
    Click,
    AuxClick,
    MouseDown,
    CaptureMouseDown,
    MouseDownOut,
    MouseUp,
    CaptureMouseUp,
    MouseUpOut,
    MousePressure,
    CaptureMousePressure,
    MouseMove,
    MouseExit,
    ScrollWheel,
    Pinch,
    CapturePinch,
    KeyDown,
    CaptureKeyDown,
    KeyUp,
    CaptureKeyUp,
    ModifiersChanged,
    Hover,
    FileDropExit,
    Tooltip,
    Action(gpui::accesskit::Action),
    Change,
    Key,
    Submit,
    Show,
    Resize,
    Drag,
    Dismiss,
    ListRequest,
    ListScroll,
    RichClick,
    RichHover,
    RichTooltip,
}

/// Where a listener was authored: the path of the element that carries it
/// (an id-less element's is its nearest identified ancestor's), what it is
/// for, and which one of that kind under that path it is; a listener inside
/// a tooltip's content names the tooltip's own route too, since the content
/// lowers in a scope of its own and lives as long as that route.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    within: Option<u32>,
    path: Vec<ElementIdWire>,
    kind: Kind,
    ordinal: u32,
}

struct Slot {
    id: u32,
    /// The frame that last lowered the key.
    seen: u64,
}

/// The route table: what the host's `handler` numbers mean. An id is
/// handed out the first time its key is seen and kept while the key is
/// lowered, so a route the host took from the frame it painted names the
/// same listener in the next frame, whatever changed around it. A frame
/// lowered without the key frees the id, and no id is handed out twice, so
/// a freed route names nothing for the rest of the driver's life.
#[derive(Default)]
struct Routes {
    ids: HashMap<Key, Slot>,
    live: HashMap<u32, Rc<dyn Any>>,
    next: u32,
    frame: u64,
    within: Option<u32>,
    /// `(kind, taken)` for the scopes being lowered, innermost last;
    /// `marks` says where each scope's counters start.
    counters: Vec<(Kind, u32)>,
    marks: Vec<usize>,
}

impl Routes {
    fn take(&mut self, path: &[ElementIdWire], kind: Kind, route: Rc<dyn Any>) -> u32 {
        let start = self.marks.last().copied().unwrap_or(0);
        let ordinal = match self.counters[start..]
            .iter_mut()
            .find(|(taken, _)| *taken == kind)
        {
            Some((_, taken)) => {
                *taken += 1;
                *taken - 1
            }
            None => {
                self.counters.push((kind, 1));
                0
            }
        };
        let key = Key {
            within: self.within,
            path: path.to_vec(),
            kind,
            ordinal,
        };
        let id = match self.ids.entry(key) {
            Entry::Occupied(mut slot) => {
                slot.get_mut().seen = self.frame;
                slot.get().id
            }
            Entry::Vacant(slot) => {
                let id = self.next;
                self.next = id.checked_add(1).expect("route ids exhausted");
                slot.insert(Slot {
                    id,
                    seen: self.frame,
                });
                id
            }
        };
        self.live.insert(id, route);
        id
    }

    fn get(&self, id: u32) -> Option<Rc<dyn Any>> {
        self.live.get(&id).cloned()
    }

    fn begin_lowering(&mut self, within: Option<u32>) {
        self.within = within;
        self.counters.clear();
        self.marks.clear();
    }

    fn enter_scope(&mut self) {
        self.marks.push(self.counters.len());
    }

    fn leave_scope(&mut self) {
        if let Some(mark) = self.marks.pop() {
            self.counters.truncate(mark);
        }
    }

    /// Frees every key the frame did not lower. A route authored inside a
    /// tooltip's content is lowered once, when the host asks for that
    /// tooltip, and lives as long as the tooltip's own route does.
    fn end_frame(&mut self) {
        let frame = self.frame;
        let kept: HashSet<u32> = self
            .ids
            .values()
            .filter(|slot| slot.seen == frame)
            .map(|slot| slot.id)
            .collect();
        let live = &mut self.live;
        self.ids.retain(|key, slot| {
            let alive = slot.seen == frame || key.within.is_some_and(|owner| kept.contains(&owner));
            if !alive {
                live.remove(&slot.id);
            }
            alive
        });
    }
}

#[derive(Clone, Default)]
pub struct Context(Rc<RefCell<Tables>>);

impl Context {
    pub(crate) fn with_host(host: crate::Host) -> Self {
        let context = Self::default();
        context.0.borrow_mut().host = host;
        context
    }
}

/// Returns a picture hash, and its bytes the first time this driver sends
/// it within the frame's picture budget. The host's sanitizer drops the
/// bytes of every picture past [`crate::wire::MAX_PICTURE_BYTES_PER_FRAME`]
/// in one frame, so a picture that does not fit in what is left goes out by
/// hash alone, is not marked sent, and is owed: [`start_frame`] says so,
/// and the driver draws again until every picture is held. `cost` is what
/// the host counts for it, which for raw pixels is not their header.
pub fn picture(context: &Context, bytes: impl AsRef<[u8]>, cost: usize) -> (u64, Option<Vec<u8>>) {
    use crate::wire::MAX_PICTURE_BYTES_PER_FRAME;
    use std::hash::{Hash, Hasher};
    let bytes = bytes.as_ref();
    let mut hasher = std::hash::DefaultHasher::new();
    bytes.hash(&mut hasher);
    let hash = hasher.finish();
    let tables = &mut *context.0.borrow_mut();
    if tables.pictures.contains(&hash) {
        return (hash, None);
    }
    // No frame can carry it: the host drops it whole, so it is neither
    // sent nor owed, and the host draws its hash as a picture it lacks.
    if cost > MAX_PICTURE_BYTES_PER_FRAME {
        return (hash, None);
    }
    if cost > MAX_PICTURE_BYTES_PER_FRAME - tables.picture_bytes {
        tables.pictures_owed = true;
        return (hash, None);
    }
    if tables.pictures.len() >= 4_096 {
        tables.pictures.clear();
    }
    tables.picture_bytes += cost;
    tables.pictures.insert(hash);
    (hash, Some(bytes.to_vec()))
}

/// Starts a frame's picture budget, and answers whether the last frame
/// owed a picture it drew.
pub(crate) fn start_frame(context: &Context) -> bool {
    let tables = &mut *context.0.borrow_mut();
    tables.picture_bytes = 0;
    std::mem::take(&mut tables.pictures_owed)
}

/// Whether this frame drew a picture it had no budget left to send.
pub(crate) fn pictures_owed(context: &Context) -> bool {
    context.0.borrow().pictures_owed
}

pub(crate) fn clear_pictures(context: &Context) {
    context.0.borrow_mut().pictures.clear();
}

/// A lowering begins: its routes are keyed in the tree, or inside the
/// tooltip whose content it lowers.
pub(crate) fn begin_lowering(context: &Context, within: Option<u32>) {
    context.0.borrow_mut().routes.begin_lowering(within);
}

/// An identified element starts lowering: its listeners are the first of
/// their kind under its path.
pub(crate) fn enter_scope(context: &Context) {
    context.0.borrow_mut().routes.enter_scope();
}

pub(crate) fn leave_scope(context: &Context) {
    context.0.borrow_mut().routes.leave_scope();
}

/// The root is about to lower a frame.
pub(crate) fn begin_frame(context: &Context) {
    context.0.borrow_mut().routes.frame += 1;
}

/// The frame is lowered: every route it did not take is freed.
pub(crate) fn end_frame(context: &Context) {
    context.0.borrow_mut().routes.end_frame();
}

pub(crate) fn tooltip(context: &Context, scope: &[ElementIdWire], build: TooltipBuilder) -> u32 {
    let route: Rc<dyn Any> = Rc::new(TooltipRoute(Rc::new(move |_, window, cx| {
        Some(build(window, cx))
    })));
    context
        .0
        .borrow_mut()
        .routes
        .take(scope, Kind::Tooltip, route)
}

pub(crate) fn rich_text_tooltip(
    context: &Context,
    scope: &[ElementIdWire],
    build: RichTextTooltipBuilder,
) -> u32 {
    let route: Rc<dyn Any> = Rc::new(TooltipRoute(Rc::new(move |index, window, cx| {
        index.and_then(|index| build(index, window, cx))
    })));
    context
        .0
        .borrow_mut()
        .routes
        .take(scope, Kind::RichTooltip, route)
}

/// Builds the tooltip route `request` names, if it names one.
pub(crate) fn build_tooltip(
    context: &Context,
    request: u32,
    character_index: Option<usize>,
    window: &mut crate::Window,
    app: &mut crate::App,
) -> Option<Option<crate::AnyView>> {
    let route = context.0.borrow().routes.get(request)?;
    let route = route.downcast_ref::<TooltipRoute>()?.0.clone();
    Some(route(character_index, window, app))
}

pub(crate) fn tooltip_response(context: &Context, response: crate::wire::TooltipResponse) {
    context.0.borrow_mut().tooltip_responses.push(response);
}

pub(crate) fn take_tooltip_responses(context: &Context) -> Vec<crate::wire::TooltipResponse> {
    std::mem::take(&mut context.0.borrow_mut().tooltip_responses)
}

pub(crate) fn route<A: 'static>(
    context: &Context,
    scope: &[ElementIdWire],
    kind: Kind,
    listener: impl Fn(&A, &mut crate::Window, &mut crate::App) + 'static,
) -> u32 {
    let route: Rc<dyn Any> = Rc::new(EventRoute::<A>(Rc::new(listener)));
    context.0.borrow_mut().routes.take(scope, kind, route)
}

/// Runs the route `index` names, if it names one that takes an `A`.
pub(crate) fn run_route<A: 'static>(
    context: &Context,
    index: u32,
    event: &A,
    window: &mut crate::Window,
    app: &mut crate::App,
) -> bool {
    let handler = context.0.borrow().routes.get(index);
    let Some(route) =
        handler.and_then(|route| route.downcast_ref::<EventRoute<A>>().map(|r| r.0.clone()))
    else {
        return false;
    };
    route(event, window, app);
    true
}

pub(crate) fn host(context: &Context) -> crate::Host {
    context.0.borrow().host.clone()
}

pub(crate) fn widget(context: &Context, command: crate::wire::WidgetCommand) {
    context.0.borrow_mut().widgets.push(command);
}

pub(crate) fn take_widgets(context: &Context) -> Vec<crate::wire::WidgetCommand> {
    std::mem::take(&mut context.0.borrow_mut().widgets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_contexts_keep_their_own_routes_and_picture_history() {
        let first = Context::default();
        let first_route = route::<String>(&first, &[], Kind::Change, |_, _, _| {});
        assert!(picture(&first, b"svg", 3).1.is_some());
        {
            let second = Context::default();
            let route = route::<String>(&second, &[], Kind::Change, |_, _, _| {});
            assert_eq!(
                (first_route, route),
                (0, 0),
                "each driver starts its own route table"
            );
            assert!(
                picture(&second, b"svg", 3).1.is_some(),
                "a new host needs its own picture bytes"
            );
        }
        assert!(
            picture(&first, b"svg", 3).1.is_none(),
            "returning to the first driver preserves its picture history"
        );
    }

    #[test]
    fn clearing_picture_history_resends_content() {
        let context = Context::default();
        assert!(picture(&context, b"image", 5).1.is_some());
        assert!(picture(&context, b"image", 5).1.is_none());
        clear_pictures(&context);
        assert_eq!(
            picture(&context, b"image", 5).1.as_deref(),
            Some(b"image".as_slice())
        );
    }
}
