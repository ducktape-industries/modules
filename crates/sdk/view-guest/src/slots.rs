//! Event routes and sent pictures belong to one running driver.
use std::any::Any;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

type ClickRoute = Rc<dyn Fn(&gpui::ClickEvent, &mut crate::Window, &mut crate::App)>;
type TooltipRoute =
    Rc<dyn Fn(Option<usize>, &mut crate::Window, &mut crate::App) -> Option<crate::AnyView>>;
pub(crate) type TooltipBuilder =
    Box<dyn Fn(&mut crate::Window, &mut crate::App) -> crate::AnyView + 'static>;
pub(crate) type RichTextTooltipBuilder =
    Box<dyn Fn(usize, &mut crate::Window, &mut crate::App) -> Option<crate::AnyView> + 'static>;
type EventHandler<A> = Rc<dyn Fn(&A, &mut crate::Window, &mut crate::App)>;

struct EventRoute<A>(EventHandler<A>);

#[derive(Default)]
struct Tables {
    host: crate::Host,
    messages: Routes<Rc<dyn Any>>,
    handlers: Routes<Rc<dyn Any>>,
    clicks: Routes<ClickRoute>,
    tooltips: Routes<TooltipRoute>,
    row: Option<Row>,
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

/// A route table. Routes taken while a list row lowers get ids from the row
/// and their order in it, not from the order of the whole frame: a row
/// scrolled in above the others renumbered every route after it otherwise,
/// and every node carrying one went out again as a props patch.
struct Routes<T> {
    frame: Vec<T>,
    rows: std::collections::HashMap<u32, T>,
}

impl<T> Default for Routes<T> {
    fn default() -> Self {
        Self {
            frame: Vec::new(),
            rows: std::collections::HashMap::new(),
        }
    }
}

/// The list row being lowered: its key, and how many routes it has taken.
#[derive(Clone, Copy)]
pub(crate) struct Row {
    key: u64,
    taken: u32,
}

/// Row ids have the top bit set; frame ids never get that far.
const ROW_IDS: u32 = 1 << 31;

impl<T: Clone> Routes<T> {
    fn push(&mut self, row: &mut Option<Row>, route: T, what: &str) -> u32 {
        if let Some(row) = row {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::hash::DefaultHasher::new();
            (row.key, row.taken).hash(&mut hasher);
            row.taken += 1;
            let id = ROW_IDS | hasher.finish() as u32;
            // A collision only costs the stable id: the route takes a frame id.
            if let std::collections::hash_map::Entry::Vacant(slot) = self.rows.entry(id) {
                slot.insert(route);
                return id;
            }
        }
        let id = u32::try_from(self.frame.len())
            .ok()
            .filter(|id| *id < ROW_IDS)
            .unwrap_or_else(|| panic!("too many {what} routes"));
        self.frame.push(route);
        id
    }

    fn get(&self, id: u32) -> Option<T> {
        match id & ROW_IDS {
            0 => self.frame.get(id as usize).cloned(),
            _ => self.rows.get(&id).cloned(),
        }
    }
}

/// Routes taken from here to [`leave_row`] are keyed by `key`.
pub(crate) fn enter_row(context: &Context, key: u64) -> Option<Row> {
    context.0.borrow_mut().row.replace(Row { key, taken: 0 })
}

pub(crate) fn leave_row(context: &Context, outer: Option<Row>) {
    context.0.borrow_mut().row = outer;
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

pub(crate) fn reset(context: &Context) {
    let old = {
        let mut tables = context.0.borrow_mut();
        (
            std::mem::take(&mut tables.messages),
            std::mem::take(&mut tables.handlers),
            std::mem::take(&mut tables.clicks),
            std::mem::take(&mut tables.tooltips),
        )
    };
    drop(old);
}

pub(crate) fn tooltip(context: &Context, build: TooltipBuilder) -> u32 {
    let tables = &mut *context.0.borrow_mut();
    let route: TooltipRoute = Rc::new(move |_, window, cx| Some(build(window, cx)));
    tables.tooltips.push(&mut tables.row, route, "tooltip")
}

pub(crate) fn rich_text_tooltip(context: &Context, build: RichTextTooltipBuilder) -> u32 {
    let tables = &mut *context.0.borrow_mut();
    let route: TooltipRoute =
        Rc::new(move |index, window, cx| index.and_then(|index| build(index, window, cx)));
    tables.tooltips.push(&mut tables.row, route, "tooltip")
}

pub(crate) fn tooltip_route(context: &Context, index: u32) -> Option<TooltipRoute> {
    context.0.borrow().tooltips.get(index)
}

pub(crate) fn tooltip_response(context: &Context, response: crate::wire::TooltipResponse) {
    context.0.borrow_mut().tooltip_responses.push(response);
}

pub(crate) fn take_tooltip_responses(context: &Context) -> Vec<crate::wire::TooltipResponse> {
    std::mem::take(&mut context.0.borrow_mut().tooltip_responses)
}

pub(crate) fn route<A: 'static>(
    context: &Context,
    listener: impl Fn(&A, &mut crate::Window, &mut crate::App) + 'static,
) -> u32 {
    let tables = &mut *context.0.borrow_mut();
    let route: Rc<dyn Any> = Rc::new(EventRoute::<A>(Rc::new(listener)));
    tables.handlers.push(&mut tables.row, route, "handler")
}

pub(crate) fn message_route(
    context: &Context,
    listener: impl Fn(&(), &mut crate::Window, &mut crate::App) + 'static,
) -> u32 {
    let tables = &mut *context.0.borrow_mut();
    let route: Rc<dyn Any> = Rc::new(EventRoute::<()>(Rc::new(listener)));
    tables.messages.push(&mut tables.row, route, "message")
}

pub(crate) fn run_route<A: 'static>(
    context: &Context,
    index: u32,
    event: &A,
    window: &mut crate::Window,
    app: &mut crate::App,
) -> bool {
    let handler = context.0.borrow().handlers.get(index);
    let Some(route) =
        handler.and_then(|route| route.downcast_ref::<EventRoute<A>>().map(|r| r.0.clone()))
    else {
        return false;
    };
    route(event, window, app);
    true
}

pub(crate) fn run_message_route(
    context: &Context,
    index: u32,
    window: &mut crate::Window,
    app: &mut crate::App,
) -> bool {
    let route = context.0.borrow().messages.get(index).and_then(|route| {
        route
            .downcast_ref::<EventRoute<()>>()
            .map(|route| route.0.clone())
    });
    let Some(route) = route else { return false };
    route(&(), window, app);
    true
}

pub(crate) fn click(
    context: &Context,
    listener: impl Fn(&gpui::ClickEvent, &mut crate::Window, &mut crate::App) + 'static,
) -> u32 {
    let tables = &mut *context.0.borrow_mut();
    tables
        .clicks
        .push(&mut tables.row, Rc::new(listener), "click")
}

pub(crate) fn run_click(
    context: &Context,
    index: u32,
    event: &gpui::ClickEvent,
    window: &mut crate::Window,
    app: &mut crate::App,
) -> bool {
    let listener = context.0.borrow().clicks.get(index);
    if let Some(listener) = listener {
        listener(event, window, app);
        true
    } else {
        false
    }
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
        let first_route = route::<String>(&first, |_, _, _| {});
        assert!(picture(&first, b"svg", 3).1.is_some());
        {
            let second = Context::default();
            let route = route::<String>(&second, |_, _, _| {});
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
