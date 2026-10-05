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
    /// The owners whose renders drew a picture that went out by hash alone
    /// for want of budget this frame.
    pictures_owed: HashSet<u64>,
    /// The lists the live frames draw, by their state.
    lists: Vec<DrawnList>,
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

/// Where a listener was authored: the scope of the element that carries it
/// (an id-less element's is its nearest identified ancestor's), what it is
/// for, and which one of that kind in that scope it is; a listener inside
/// a tooltip's content names the tooltip's own route too, since the content
/// lowers in a scope of its own and lives as long as that route.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    within: Option<u32>,
    scope: u32,
    kind: Kind,
    ordinal: u32,
}

/// A scope: an identified element, named by its segment under the scope
/// around it, or a cached entity's boundary, named by its entity
/// ([`ElementIdWire::View`]). Its number stands for its whole path, since
/// the scope around it is a number that stands for the path so far;
/// [`ROOT`] is around the outermost. A scope is looked up by these three,
/// exactly: two paths never share a number.
#[derive(PartialEq, Eq, Hash)]
struct ScopeKey {
    within: Option<u32>,
    parent: u32,
    segment: ElementIdWire,
}

/// The scope of a lowering itself, around every identified element in it.
const ROOT: u32 = 0;

/// What a scratch lowering answers for a key or a scope it does not hold:
/// a listener the kept tree has no route for, which is a change.
const UNHELD: u32 = u32::MAX;

/// A scope being lowered: its segment, and the owner whose render entered
/// it, until a listener in it or under it asks for a route, then its
/// number. A scope no listener asks in is never looked up.
enum Open {
    Unasked(ElementIdWire, Option<u64>),
    Numbered(u32),
}

struct Slot {
    id: u32,
    /// The frame that last lowered the key.
    seen: u64,
    /// The cached entity (or the root) whose render lowered it; `None`
    /// inside a tooltip's content, which lives by its tooltip's route.
    owner: Option<u64>,
}

/// The cached entities (and the root) a frame left live, each with whether
/// its render ran this frame: what every registry an owner's render wrote
/// into frees by ([`Routes::end_frame`]). An owner not here is gone.
pub(crate) type Owners = HashMap<u64, bool>;

/// Whether a slot an owner's render wrote lives on: its owner lives, and
/// either this frame lowered the slot or did not lower the owner at all.
fn kept_by(owners: &Owners, owner: u64, seen: u64, frame: u64) -> bool {
    owners
        .get(&owner)
        .is_some_and(|lowered| seen == frame || !lowered)
}

/// The route table: what the host's `handler` numbers mean. An id is
/// handed out the first time its key is seen and kept while the key is
/// lowered, so a route the host took from the frame it painted names the
/// same listener in the next frame, whatever changed around it. A frame
/// lowered without the key frees the id, unless its owner is a cached
/// entity the frame kept whole; no id is handed out twice, so a freed
/// route names nothing for the rest of the driver's life.
///
/// A scope's number is kept the same way: handed out the first time a
/// listener is lowered in it or under it, kept while one is, freed by a
/// frame that lowers none there, and never handed out twice. So a key,
/// which names its scope by number, is the same key for as long as the
/// path it was authored under is.
#[derive(Default)]
struct Routes {
    ids: HashMap<Key, Slot>,
    live: HashMap<u32, Rc<dyn Any>>,
    next: u32,
    frame: u64,
    within: Option<u32>,
    scopes: HashMap<ScopeKey, Slot>,
    /// The last scope number handed out; [`ROOT`] is never handed out.
    last_scope: u32,
    /// The scopes being lowered, outermost first.
    open: Vec<Open>,
    /// `(kind, taken)` for the scopes being lowered, innermost last;
    /// `marks` says where each scope's counters start.
    counters: Vec<(Kind, u32)>,
    marks: Vec<usize>,
    /// The cached entities whose renders are lowering, the root first.
    owners: Vec<u64>,
    /// A scratch lowering (the debug check): nothing is written, and a key
    /// or scope not held answers [`UNHELD`].
    scratch: bool,
}

impl Routes {
    /// The number of the scope being lowered, innermost: every scope
    /// around it that has none yet is looked up on the way, outermost
    /// first, each under the number of the one around it.
    fn scope(&mut self) -> u32 {
        let numbered = self
            .open
            .iter()
            .rposition(|scope| matches!(scope, Open::Numbered(_)));
        let mut parent = match numbered.map(|at| &self.open[at]) {
            Some(Open::Numbered(number)) => *number,
            _ => ROOT,
        };
        for at in numbered.map_or(0, |at| at + 1)..self.open.len() {
            let Open::Unasked(segment, owner) =
                std::mem::replace(&mut self.open[at], Open::Numbered(0))
            else {
                unreachable!("every scope past the last numbered one is unasked")
            };
            let key = ScopeKey {
                within: self.within,
                parent,
                segment,
            };
            parent = match self.scopes.entry(key) {
                Entry::Occupied(mut slot) => {
                    if !self.scratch {
                        slot.get_mut().seen = self.frame;
                    }
                    slot.get().id
                }
                Entry::Vacant(_) if self.scratch => UNHELD,
                Entry::Vacant(slot) => {
                    self.last_scope = self
                        .last_scope
                        .checked_add(1)
                        .expect("scope numbers exhausted");
                    slot.insert(Slot {
                        id: self.last_scope,
                        seen: self.frame,
                        owner,
                    });
                    self.last_scope
                }
            };
            self.open[at] = Open::Numbered(parent);
        }
        parent
    }

    /// The owner a slot taken now is filed under: the innermost cached
    /// entity lowering, or none inside a tooltip's content.
    fn owner(&self) -> Option<u64> {
        match self.within {
            Some(_) => None,
            None => self.owners.last().copied(),
        }
    }

    fn take(&mut self, kind: Kind, route: Rc<dyn Any>) -> u32 {
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
            scope: self.scope(),
            kind,
            ordinal,
        };
        if self.scratch {
            return self.ids.get(&key).map_or(UNHELD, |slot| slot.id);
        }
        let owner = self.owner();
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
                    owner,
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

    fn begin_lowering(&mut self, within: Option<u32>, root: u64, scratch: bool) {
        self.within = within;
        self.scratch = scratch;
        self.open.clear();
        self.counters.clear();
        self.marks.clear();
        self.owners.clear();
        self.owners.push(root);
    }

    fn enter_scope(&mut self, segment: ElementIdWire) {
        let owner = self.owner();
        self.open.push(Open::Unasked(segment, owner));
        self.marks.push(self.counters.len());
    }

    fn leave_scope(&mut self) {
        self.open.pop();
        if let Some(mark) = self.marks.pop() {
            self.counters.truncate(mark);
        }
    }

    /// A cached entity's render starts lowering: a scope of its own, named
    /// by the entity, so its listeners' ordinals start at zero and never
    /// shift its parent's, and every slot in it, the scope itself first,
    /// is filed under it.
    fn enter_boundary(&mut self, owner: u64) {
        self.owners.push(owner);
        self.enter_scope(ElementIdWire::View(owner));
    }

    fn leave_boundary(&mut self) {
        self.leave_scope();
        self.owners.pop();
    }

    /// Frees every key and every scope the frame did not lower, unless its
    /// owner is a cached entity the frame kept whole (`owners`: each live
    /// owner, and whether its render ran), or, for a scope, unless a key or
    /// a scope that lives sits under it: a number stands for its whole
    /// path, so a scope outlives everything numbered under it, asked or
    /// not. A route authored inside a tooltip's content is lowered once,
    /// when the host asks for that tooltip, and lives as long as the
    /// tooltip's own route does; so does the scope it was authored in.
    fn end_frame(&mut self, owners: &Owners) {
        let frame = self.frame;
        let kept: HashSet<u32> = self
            .ids
            .iter()
            .filter(|(key, slot)| {
                key.within.is_none()
                    && slot
                        .owner
                        .is_some_and(|owner| kept_by(owners, owner, slot.seen, frame))
            })
            .map(|(_, slot)| slot.id)
            .collect();
        let alive = |slot: &Slot, within: Option<u32>| match (within, slot.owner) {
            (None, Some(owner)) => kept_by(owners, owner, slot.seen, frame),
            (within, _) => slot.seen == frame || within.is_some_and(|owner| kept.contains(&owner)),
        };
        let live = &mut self.live;
        self.ids.retain(|key, slot| {
            let alive = alive(slot, key.within);
            if !alive {
                live.remove(&slot.id);
            }
            alive
        });
        let mut scopes: HashSet<u32> = self.ids.keys().map(|key| key.scope).collect();
        scopes.extend(
            self.scopes
                .iter()
                .filter(|(key, slot)| alive(slot, key.within))
                .map(|(_, slot)| slot.id),
        );
        let parents: HashMap<u32, u32> = self
            .scopes
            .iter()
            .map(|(key, slot)| (slot.id, key.parent))
            .collect();
        let mut above: Vec<u32> = scopes.iter().copied().collect();
        while let Some(scope) = above.pop() {
            if let Some(parent) = parents.get(&scope)
                && *parent != ROOT
                && scopes.insert(*parent)
            {
                above.push(*parent);
            }
        }
        self.scopes.retain(|_, slot| scopes.contains(&slot.id));
    }
}

/// A list drawn by a frame: the state it draws, where, and whose render
/// drew it. A state is one list's, so two live records of one state are
/// refused at the frame's end, naming both lists.
struct DrawnList {
    state: crate::ListState,
    path: Vec<ElementIdWire>,
    owner: u64,
    seen: u64,
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
pub fn picture(
    context: &Context,
    owner: u64,
    bytes: impl AsRef<[u8]>,
    cost: usize,
) -> (u64, Option<Vec<u8>>) {
    use crate::wire::MAX_PICTURE_BYTES_PER_FRAME;
    use std::hash::{Hash, Hasher};
    let bytes = bytes.as_ref();
    let mut hasher = std::hash::DefaultHasher::new();
    bytes.hash(&mut hasher);
    let hash = hasher.finish();
    let tables = &mut *context.0.borrow_mut();
    // a scratch lowering sends nothing and owes nothing
    if tables.pictures.contains(&hash) || tables.routes.scratch {
        return (hash, None);
    }
    // No frame can carry it: the host drops it whole, so it is neither
    // sent nor owed, and the host draws its hash as a picture it lacks.
    if cost > MAX_PICTURE_BYTES_PER_FRAME {
        return (hash, None);
    }
    if cost > MAX_PICTURE_BYTES_PER_FRAME - tables.picture_bytes {
        tables.pictures_owed.insert(owner);
        return (hash, None);
    }
    if tables.pictures.len() >= 4_096 {
        tables.pictures.clear();
    }
    tables.picture_bytes += cost;
    tables.pictures.insert(hash);
    (hash, Some(bytes.to_vec()))
}

/// Starts a frame's picture budget, and answers the owners whose renders
/// drew a picture the last frame owed: each renders again.
pub(crate) fn start_frame(context: &Context) -> HashSet<u64> {
    let tables = &mut *context.0.borrow_mut();
    tables.picture_bytes = 0;
    std::mem::take(&mut tables.pictures_owed)
}

/// Whether this frame drew a picture it had no budget left to send.
pub(crate) fn pictures_owed(context: &Context) -> bool {
    !context.0.borrow().pictures_owed.is_empty()
}

pub(crate) fn clear_pictures(context: &Context) {
    context.0.borrow_mut().pictures.clear();
}

/// A lowering begins: its routes are keyed in the tree, or inside the
/// tooltip whose content it lowers, and filed under `root` until a cached
/// entity's boundary is entered. A `scratch` lowering writes nothing.
pub(crate) fn begin_lowering(context: &Context, within: Option<u32>, root: u64, scratch: bool) {
    context
        .0
        .borrow_mut()
        .routes
        .begin_lowering(within, root, scratch);
}

/// A cached entity's render starts lowering ([`Routes::enter_boundary`]).
pub(crate) fn enter_boundary(context: &Context, owner: u64) {
    context.0.borrow_mut().routes.enter_boundary(owner);
}

pub(crate) fn leave_boundary(context: &Context) {
    context.0.borrow_mut().routes.leave_boundary();
}

/// The list being lowered at `path`, under `owner`, draws `state`.
pub(crate) fn draws_list(
    context: &Context,
    state: &crate::ListState,
    path: &[ElementIdWire],
    owner: u64,
) {
    let tables = &mut *context.0.borrow_mut();
    if tables.routes.scratch {
        return;
    }
    let frame = tables.routes.frame;
    match tables.lists.iter_mut().find(|drawn| drawn.state.is(state)) {
        Some(drawn) if drawn.seen != frame => {
            drawn.path = path.to_vec();
            drawn.owner = owner;
            drawn.seen = frame;
        }
        Some(drawn) => panic!(
            "one ListState drawn by two lists: {:?} and {path:?}; each list takes a state of its own",
            drawn.path
        ),
        None => tables.lists.push(DrawnList {
            state: state.clone(),
            path: path.to_vec(),
            owner,
            seen: frame,
        }),
    }
}

/// An identified element starts lowering, filed under `segment` in the
/// scope around it: its listeners are the first of their kind in its scope.
pub(crate) fn enter_scope(context: &Context, segment: ElementIdWire) {
    context.0.borrow_mut().routes.enter_scope(segment);
}

pub(crate) fn leave_scope(context: &Context) {
    context.0.borrow_mut().routes.leave_scope();
}

/// The root is about to lower a frame.
pub(crate) fn begin_frame(context: &Context) {
    context.0.borrow_mut().routes.frame += 1;
}

/// The frame is lowered: every route, scope and list record it did not
/// take is freed, unless a cached entity the frame kept whole owns it.
pub(crate) fn end_frame(context: &Context, owners: &Owners) {
    let tables = &mut *context.0.borrow_mut();
    tables.routes.end_frame(owners);
    let frame = tables.routes.frame;
    tables
        .lists
        .retain(|drawn| kept_by(owners, drawn.owner, drawn.seen, frame));
}

pub(crate) fn tooltip(context: &Context, build: TooltipBuilder) -> u32 {
    let route: Rc<dyn Any> = Rc::new(TooltipRoute(Rc::new(move |_, window, cx| {
        Some(build(window, cx))
    })));
    context.0.borrow_mut().routes.take(Kind::Tooltip, route)
}

pub(crate) fn rich_text_tooltip(context: &Context, build: RichTextTooltipBuilder) -> u32 {
    let route: Rc<dyn Any> = Rc::new(TooltipRoute(Rc::new(move |index, window, cx| {
        index.and_then(|index| build(index, window, cx))
    })));
    context.0.borrow_mut().routes.take(Kind::RichTooltip, route)
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
    kind: Kind,
    listener: impl Fn(&A, &mut crate::Window, &mut crate::App) + 'static,
) -> u32 {
    let route: Rc<dyn Any> = Rc::new(EventRoute::<A>(Rc::new(listener)));
    context.0.borrow_mut().routes.take(kind, route)
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
        let first_route = route::<String>(&first, Kind::Change, |_, _, _| {});
        assert!(picture(&first, 0, b"svg", 3).1.is_some());
        {
            let second = Context::default();
            let route = route::<String>(&second, Kind::Change, |_, _, _| {});
            assert_eq!(
                (first_route, route),
                (0, 0),
                "each driver starts its own route table"
            );
            assert!(
                picture(&second, 0, b"svg", 3).1.is_some(),
                "a new host needs its own picture bytes"
            );
        }
        assert!(
            picture(&first, 0, b"svg", 3).1.is_none(),
            "returning to the first driver preserves its picture history"
        );
    }

    /// A frame that lowers one listener in each scope `paths` names;
    /// answers the scope number each was keyed under.
    fn frame(routes: &mut Routes, paths: &[&[&'static str]]) -> Vec<u32> {
        routes.frame += 1;
        routes.begin_lowering(None, 0, false);
        let scopes = paths
            .iter()
            .map(|path| {
                for segment in *path {
                    routes.enter_scope(ElementIdWire::Name((*segment).into()));
                }
                let scope = routes.scope();
                routes.take(Kind::Click, Rc::new(()));
                path.iter().for_each(|_| routes.leave_scope());
                scope
            })
            .collect();
        routes.end_frame(&HashMap::from([(0, true)]));
        scopes
    }

    /// A scope's number stands for its whole path: one segment under two
    /// parents is two scopes, and a scope keeps its number for as long as
    /// a frame lowers a listener in it, whatever is lowered around it.
    #[test]
    fn a_scope_number_names_one_path_and_is_kept_across_frames() {
        let mut routes = Routes::default();
        let first = frame(&mut routes, &[&["a", "row"], &["b", "row"], &["row"]]);
        let [a_row, b_row, row] = first[..] else {
            unreachable!()
        };
        assert!(a_row != b_row && a_row != row && b_row != row, "{first:?}");
        assert!(!first.contains(&ROOT), "the root is no element's scope");

        // the same paths, lowered in another order beside a new one
        let second = frame(&mut routes, &[&["row"], &["c", "row"], &["b", "row"]]);
        assert_eq!((second[0], second[2]), (row, b_row));
        assert!(!first.contains(&second[1]), "a new path is a new scope");

        // `a/row` was not lowered: its number is gone for good
        let third = frame(&mut routes, &[&["a", "row"], &["b", "row"]]);
        assert_eq!(third[1], b_row);
        assert!(!first.contains(&third[0]) && !second.contains(&third[0]));
    }

    /// A scope no listener is lowered in, or under, is never looked up.
    #[test]
    fn a_scope_without_a_listener_is_not_numbered() {
        let mut routes = Routes::default();
        routes.frame += 1;
        routes.begin_lowering(None, 0, false);
        routes.enter_scope(ElementIdWire::Name("page".into()));
        routes.enter_scope(ElementIdWire::Name("plain".into()));
        routes.leave_scope();
        routes.enter_scope(ElementIdWire::Name("button".into()));
        routes.take(Kind::Click, Rc::new(()));
        routes.leave_scope();
        routes.leave_scope();
        let mut numbered: Vec<_> = routes
            .scopes
            .keys()
            .map(|key| key.segment.name().unwrap().to_owned())
            .collect();
        numbered.sort();
        assert_eq!(numbered, ["button", "page"]);
    }

    #[test]
    fn clearing_picture_history_resends_content() {
        let context = Context::default();
        assert!(picture(&context, 0, b"image", 5).1.is_some());
        assert!(picture(&context, 0, b"image", 5).1.is_none());
        clear_pictures(&context);
        assert_eq!(
            picture(&context, 0, b"image", 5).1.as_deref(),
            Some(b"image".as_slice())
        );
    }
}
