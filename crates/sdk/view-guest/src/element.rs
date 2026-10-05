//! Guest-side GPUI-shaped authoring.
//!
//! The fluent style methods are the real `gpui::Styled` implementation. The
//! element and interaction traits are deliberately local: native GPUI
//! elements require a native layout arena, window, and application, none of
//! which exists in a wasm guest. Lowering turns this small recipe into wire
//! data once per frame.

use crate::interactivity::{EventListener, Interactivity};
use crate::kept::Kept;
use crate::view_element::Child;
use crate::{App, Window, slots, wire};
use gpui::{
    ElementId, ListHorizontalSizingBehavior, ListSizingBehavior, Overflow, ScrollStrategy,
    SharedString, StyleRefinement, Styled,
};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

/// A guest element that can be lowered by the driver.
///
/// This is intentionally a guest-side boundary with the same name as GPUI's
/// native trait. GPUI's real `Element` requires native layout and paint state;
/// a wasm guest has neither, so lowering is the only operation it can perform.
pub trait Element: 'static + IntoElement {
    /// The authored identity that enters the typed ancestry while this element lowers.
    fn id(&self) -> Option<ElementId> {
        None
    }

    #[doc(hidden)]
    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node;

    /// Whether this element lowers to the node of an element it builds (a
    /// view's render), rather than to a node of its own.
    #[doc(hidden)]
    fn defers(&self) -> bool {
        false
    }

    #[doc(hidden)]
    fn into_any(self) -> AnyElement {
        AnyElement(Box::new(self))
    }
}

/// A value that can be converted into a guest element recipe.
pub trait IntoElement: Sized {
    type Element: Element;

    fn into_element(self) -> Self::Element;

    fn into_any_element(self) -> AnyElement {
        self.into_element().into_any()
    }
}

trait ElementObject {
    fn id(&self) -> Option<ElementId>;
    fn defers(&self) -> bool;
    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node;
}

impl<T: Element> ElementObject for T {
    fn id(&self) -> Option<ElementId> {
        Element::id(self)
    }

    fn defers(&self) -> bool {
        Element::defers(self)
    }

    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        Element::lower(self, lowering)
    }
}

/// A type-erased guest element, used for conditional children and components.
pub struct AnyElement(Box<dyn ElementObject>);

impl Element for AnyElement {
    fn id(&self) -> Option<ElementId> {
        self.0.id()
    }

    fn defers(&self) -> bool {
        self.0.defers()
    }

    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        self.0.lower(lowering)
    }

    fn into_any(self) -> AnyElement {
        self
    }
}

impl IntoElement for AnyElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }

    fn into_any_element(self) -> AnyElement {
        self
    }
}

impl gpui::prelude::FluentBuilder for AnyElement {}

/// The explicit lowering context for one driver frame. Every element is
/// filed by the host's own rule ([`wire::identity`]), so the path a route,
/// a list or a tooltip is keyed by here is the path the host files the
/// node under. Lowering claims nothing: whether two ids meet is the host's
/// to say, and its sanitizer says it of every frame, a test's too.
pub struct Lowering<'a> {
    window: &'a mut Window,
    app: &'a mut App,
    /// The path of the element being lowered: its id last, when it has one.
    authored_path: Vec<wire::ElementIdWire>,
    /// The index of the list row whose root lowers next ([`Self::lower_row`]).
    row: Option<usize>,
    /// The cached entities whose renders are lowering, the root first: the
    /// innermost owns what is lowered now.
    owners: Vec<u64>,
    /// The cached boundaries open, outermost first: each owner, and how
    /// many segments of `authored_path` were in before it.
    boundaries: Vec<(usize, u64)>,
    /// Every child entity placed so far: whether cached, and where. One
    /// entity has one kept subtree, so a cached placement is its only one.
    pub(crate) placed: HashMap<u64, (bool, Vec<wire::ElementIdWire>)>,
    /// How many nodes this lowering produced (`TickReport::lowered`).
    pub(crate) lowered: usize,
    /// The content of a tooltip: nothing in it is kept, and nothing it
    /// renders is recorded ([`Self::records`]).
    tooltip: bool,
    /// The debug check's re-lowering of a kept entity: every table answers
    /// what it holds and writes nothing.
    scratch: bool,
}

/// An authored [`ElementId`] as the wire carries it. Every id a view can
/// author fits; the only refusals are ids gpui itself could not name.
pub(crate) fn wire_id(id: ElementId) -> wire::ElementIdWire {
    wire::ElementIdWire::from_gpui(id)
        .expect("element ID must be portable across the view boundary")
}

impl<'a> Lowering<'a> {
    pub(crate) fn new(window: &'a mut Window, app: &'a mut App) -> Self {
        Self::begin(window, app, None, false)
    }

    /// A lowering of the content the tooltip route `request` builds: its
    /// listeners are keyed inside that tooltip and live as long as it does.
    pub(crate) fn within_tooltip(window: &'a mut Window, app: &'a mut App, request: u32) -> Self {
        Self::begin(window, app, Some(request), false)
    }

    /// The debug check's lowering: it reads every table and writes none.
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    pub(crate) fn for_check(window: &'a mut Window, app: &'a mut App) -> Self {
        Self::begin(window, app, None, true)
    }

    fn begin(window: &'a mut Window, app: &'a mut App, within: Option<u32>, scratch: bool) -> Self {
        let root = app.inner.root_entity.get();
        slots::begin_lowering(&app.inner.slots, within, root, scratch);
        Self {
            window,
            app,
            authored_path: Vec::new(),
            row: None,
            owners: vec![root],
            boundaries: Vec::new(),
            placed: HashMap::new(),
            lowered: 0,
            tooltip: within.is_some(),
            scratch,
        }
    }

    pub(crate) fn slots(&self) -> &slots::Context {
        &self.app.inner.slots
    }

    pub(crate) fn theme(&self) -> crate::Theme {
        *self.app.global::<crate::Theme>()
    }

    /// The id `style` crosses under: its entry in the frame's style table,
    /// one for every node that carries the same style.
    pub(crate) fn style(&self, style: &StyleRefinement) -> wire::StyleId {
        let mut styles = self.app.inner.styles.borrow_mut();
        match self.scratch {
            true => styles.lookup(style),
            false => styles.intern(style),
        }
    }

    /// The id of the style that sets nothing, which a bare text carries.
    pub(crate) fn no_style(&self) -> wire::StyleId {
        self.app.inner.styles.borrow_mut().intern_empty()
    }

    pub(crate) fn parts(&mut self) -> (&mut Window, &mut App) {
        (self.window, self.app)
    }

    /// The cached entity (or the root) whose render is lowering.
    pub(crate) fn owner(&self) -> u64 {
        *self.owners.last().expect("the root owns the lowering")
    }

    /// Whether this is the debug check's lowering, which takes nothing.
    pub(crate) fn scratch(&self) -> bool {
        self.scratch
    }

    pub(crate) fn render_once(&mut self, component: impl RenderOnce) -> wire::Node {
        let element = component.render(self.window, self.app).into_element();
        self.lower_element(element)
    }

    pub(crate) fn lower<E: IntoElement>(&mut self, element: E) -> wire::Node {
        self.lower_element(element.into_element())
    }

    /// Lowers `element` in its scope: filed under its id (or, as a list
    /// row's root, its index), and everything it lowers under it.
    pub(crate) fn lower_element<E: Element>(&mut self, element: E) -> wire::Node {
        // a row's root is the first element under it that lowers to a node
        // of its own: a deferred view lowers to the one its render returns
        let defers = element.defers();
        let row = if defers { None } else { self.row.take() };
        let segment = wire::identity::segment(element.id().map(wire_id), row);
        let entered = segment.is_some();
        if let Some(segment) = segment {
            slots::enter_scope(&self.app.inner.slots, segment.clone());
            self.authored_path.push(segment);
        }
        self.lowered += usize::from(!defers);
        // the box an `AnyElement` already is, or the one box an element gets
        let node = element.into_any().0.lower(self);
        debug_assert!(
            defers
                || wire::identity::segment(node.identity().cloned(), row).as_ref()
                    == entered.then(|| self.authored_path.last()).flatten(),
            "an element is filed as the host files the node it lowers to"
        );
        if entered {
            self.authored_path.pop();
            slots::leave_scope(&self.app.inner.slots);
        }
        node
    }

    /// Lowers row `index` of a list: filed under its own id, else under
    /// its index, so the ids inside one row never meet another row's.
    pub(crate) fn lower_row<E: Element>(&mut self, index: usize, element: E) -> wire::Node {
        self.row = Some(index);
        let node = self.lower_element(element);
        debug_assert!(self.row.is_none(), "a row lowers to a node");
        node
    }

    /// A child entity placed plain (`.child(entity)`): rendered and lowered
    /// in place, every frame its parent lowers, with its parent recorded
    /// so its notify reaches the root.
    pub(crate) fn lower_child(&mut self, child: &Child) -> wire::Node {
        self.place(child, false);
        if self.records() {
            self.app
                .inner
                .parents
                .borrow_mut()
                .insert(child.id, self.owner());
            self.app.count_lowered(child.id);
            // a notify raised before this render is answered by it
            self.app
                .inner
                .pending
                .borrow_mut()
                .entities
                .remove(&child.id);
        }
        let element = (child.render)(self.window, self.app);
        self.lower_element(element)
    }

    /// Whether this lowering is the frame's: the one that writes the
    /// tables. A tooltip's content and the debug check's re-lowering read
    /// them and record nothing, so neither takes a notify the frame owes
    /// nor files an entity under a parent it does not sit under.
    fn records(&self) -> bool {
        !self.scratch && !self.tooltip
    }

    /// A child entity placed cached (`entity.cached(style)`): its subtree
    /// is kept across frames in a box styled `style`, and rendered again
    /// only when it, or an entity rendered inside it, was notified
    /// ([`crate::kept`]). Inside a tooltip's content nothing is kept: the
    /// child lowers plain.
    pub(crate) fn lower_cached(&mut self, child: &Child, style: &StyleRefinement) -> wire::Node {
        if self.tooltip {
            return self.lower_child(child);
        }
        assert!(
            self.row.is_none(),
            "{} is cached as a list row's root at {:?}: a cached entity carries no row identity; \
             cache it inside the row, or give the row an id of its own",
            child.type_name,
            self.authored_path
        );
        self.place(child, true);
        // the box is the parent's node: lowered whenever the parent is
        self.lowered += 1;
        let style = self.style(style);
        let content = self.lower_kept(child, |this, child| {
            let kept = this.app.inner.kept.borrow();
            // kept here: at the same path, under the owner that placed it
            let same_place = kept.get(&child.id).is_some_and(|entry| {
                entry.path == this.authored_path && entry.parent == this.owner()
            });
            // notified before the frame, or during it before this point (a
            // fact its parent pushed from its render): rendered now
            same_place
                && !this.app.inner.rendering.borrow().holds(child.id)
                && !this.app.inner.pending.borrow().holds(child.id)
        });
        wire::Node::View {
            view: child.id,
            style,
            content,
        }
    }

    /// Lowers `child` inside its own boundary: a stand-in when `clean`
    /// says its kept subtree stands, else its render, recorded as kept.
    pub(crate) fn lower_kept(
        &mut self,
        child: &Child,
        clean: impl FnOnce(&Self, &Child) -> bool,
    ) -> Option<Box<wire::Node>> {
        let parent = self.owner();
        let clean = clean(self, child);
        slots::enter_boundary(&self.app.inner.slots, child.id);
        self.owners.push(child.id);
        self.boundaries.push((self.authored_path.len(), child.id));
        let content = match clean {
            true => {
                if !self.scratch
                    && let Some(entry) = self.app.inner.kept.borrow_mut().get_mut(&child.id)
                {
                    entry.seen = true;
                }
                None
            }
            false => {
                if !self.scratch {
                    self.app.inner.parents.borrow_mut().insert(child.id, parent);
                    self.app.count_lowered(child.id);
                    // a notify raised before this render is answered by it
                    self.app
                        .inner
                        .pending
                        .borrow_mut()
                        .entities
                        .remove(&child.id);
                }
                self.app.inner.lowering.borrow_mut().push(child.id);
                let element = (child.render)(self.window, self.app);
                let node = self.lower_element(element);
                self.app.inner.lowering.borrow_mut().pop();
                if !self.scratch {
                    let boundaries = self.boundaries[..self.boundaries.len() - 1].to_vec();
                    self.app.inner.kept.borrow_mut().insert(
                        child.id,
                        Kept {
                            parent,
                            path: self.authored_path.clone(),
                            boundaries,
                            render: child.render.clone(),
                            type_name: child.type_name,
                            seen: true,
                            lowered: true,
                        },
                    );
                }
                Some(Box::new(node))
            }
        };
        self.boundaries.pop();
        self.owners.pop();
        slots::leave_boundary(&self.app.inner.slots);
        content
    }

    /// Re-enters the scopes a kept entity was lowered inside, so a scratch
    /// lowering of it numbers its scopes and ordinals as the real one did.
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    pub(crate) fn replay(&mut self, kept: &Kept) {
        for at in 0..=kept.path.len() {
            for (_, owner) in kept.boundaries.iter().filter(|(before, _)| *before == at) {
                slots::enter_boundary(&self.app.inner.slots, *owner);
                self.owners.push(*owner);
                self.boundaries.push((at, *owner));
            }
            if let Some(segment) = kept.path.get(at) {
                slots::enter_scope(&self.app.inner.slots, segment.clone());
                self.authored_path.push(segment.clone());
            }
        }
    }

    /// Records a child entity's placement; a second placement of an entity
    /// with a cached one, or a cached placement of one already placed, is
    /// refused naming both, since one entity has one kept subtree.
    fn place(&mut self, child: &Child, cached: bool) {
        match self.placed.get(&child.id) {
            Some((first_cached, first)) if cached || *first_cached => panic!(
                "{} is one element: it is a child twice (under {first:?} and {:?})",
                child.type_name, self.authored_path
            ),
            _ => {
                self.placed
                    .insert(child.id, (cached, self.authored_path.clone()));
            }
        }
    }

    /// The list being lowered draws `state`. A state is one list's (its
    /// window of rows, the commands waiting for it), so a second list
    /// drawing it in this frame is refused, naming both.
    pub(crate) fn draws_list(&mut self, state: &crate::ListState) {
        let owner = self.owner();
        slots::draws_list(&self.app.inner.slots, state, &self.authored_path, owner);
    }

    pub(crate) fn current_path(&self) -> &[wire::ElementIdWire] {
        &self.authored_path
    }

    /// The id of the identified element being lowered, as the wire carries
    /// it: the segment it was filed under when its lowering began.
    pub(crate) fn own_id(&self) -> wire::ElementIdWire {
        self.authored_path
            .last()
            .cloned()
            .expect("an identified element lowers inside its authored scope")
    }

    /// A route for a listener of `kind` on the element being lowered.
    pub(crate) fn route<A: 'static>(
        &self,
        kind: slots::Kind,
        listener: impl Fn(&A, &mut Window, &mut App) + 'static,
    ) -> u32 {
        slots::route(&self.app.inner.slots, kind, listener)
    }

    pub(crate) fn picture(&self, bytes: impl AsRef<[u8]>, cost: usize) -> (u64, Option<Vec<u8>>) {
        slots::picture(&self.app.inner.slots, self.owner(), bytes, cost)
    }

    pub(crate) fn tooltip(&self, build: slots::TooltipBuilder) -> u32 {
        slots::tooltip(&self.app.inner.slots, build)
    }

    pub(crate) fn rich_text_tooltip(&self, build: slots::RichTextTooltipBuilder) -> u32 {
        slots::rich_text_tooltip(&self.app.inner.slots, build)
    }
}

/// A guest container backed by a real GPUI style refinement.
#[derive(Default)]
pub struct Div {
    pub(crate) interactivity: Box<Interactivity>,
    children: Vec<AnyElement>,
}

impl Styled for Div {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.interactivity.base_style
    }
}

impl Element for Div {
    fn id(&self) -> Option<ElementId> {
        self.interactivity.id.clone()
    }

    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        let Self {
            mut interactivity,
            children,
        } = *self;
        let id = interactivity.id.as_ref().map(|_| lowering.own_id());
        if id.is_some() {
            bar_gutter(&mut interactivity.base_style);
        }
        let style = lowering.style(&interactivity.base_style);
        let wire_interactivity = interactivity.into_wire(lowering);
        let children = children
            .into_iter()
            .map(|child| lowering.lower_element(child))
            .collect();
        wire::Node::Container(crate::wire::ContainerNode {
            id,
            style,
            interactivity: wire_interactivity,
            children,
        })
    }
}

/// A scroller the host gives a vertical bar (one with an id) keeps its
/// right edge for that bar: the bar paints over the scroller, so content
/// inset less than its width would sit under it.
fn bar_gutter(style: &mut StyleRefinement) {
    use gpui::{AbsoluteLength, DefiniteLength};
    if style.overflow.y != Some(Overflow::Scroll) {
        return;
    }
    let bar = crate::design::size::SCROLLBAR;
    let right = &mut style.padding.right;
    match right {
        Some(DefiniteLength::Absolute(AbsoluteLength::Pixels(inset))) if *inset >= bar => {}
        _ => *right = Some(bar.into()),
    }
}

impl IntoElement for Div {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl gpui::prelude::FluentBuilder for Div {}

/// Construct an empty guest container.
pub fn div() -> Div {
    Div::default()
}

/// A text field the host's editing engine owns, shown as the host's native
/// one-line field. It shows a [`TextField`](crate::TextField): the host
/// adopts the field's text when its generation moves and otherwise tells
/// the view what it holds through `on_change`. Its label is what assistive
/// technology calls it: a field has one from birth.
pub struct Input {
    id: ElementId,
    multiline: bool,
    field: Option<crate::TextField>,
    placeholder: String,
    options: wire::InputOptions,
    secure: bool,
    claims: Vec<wire::KeyClaim>,
    style: Box<StyleRefinement>,
    on_change: Option<EventListener<wire::TextChange>>,
    on_key: Option<EventListener<gpui::KeyDownEvent>>,
    on_submit: Option<EventListener<()>>,
}

impl Input {
    pub fn new(id: impl Into<ElementId>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            multiline: false,
            field: None,
            placeholder: String::new(),
            options: wire::InputOptions {
                label: label.into(),
                ..Default::default()
            },
            secure: false,
            claims: Vec::new(),
            style: Box::default(),
            on_change: None,
            on_key: None,
            on_submit: None,
        }
    }

    /// The field whose text this shows; without one the field is empty
    /// and the view hears nothing typed into it.
    pub fn value(mut self, field: &crate::TextField) -> Self {
        self.field = Some(field.clone());
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.options.description = Some(description.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.options.disabled = disabled;
        self
    }

    /// The value is wrong; [`Self::description`] says why.
    pub fn invalid(mut self, invalid: gpui::accesskit::Invalid) -> Self {
        self.options.invalid = Some(invalid);
        self
    }

    pub fn required(mut self, required: bool) -> Self {
        self.options.required = required;
        self
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.options.read_only = read_only;
        self
    }

    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    /// Hears the host's text whenever it moves: typed, pasted, undone, or
    /// an asked-for edit landed. A view keeps its field current with
    /// [`TextField::apply`](crate::TextField::apply).
    pub fn on_change(
        mut self,
        listener: impl Fn(&wire::TextChange, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Box::new(listener));
        self
    }

    /// Enter in a one-line field.
    pub fn on_submit(mut self, listener: impl Fn(&(), &mut Window, &mut App) + 'static) -> Self {
        self.on_submit = Some(Box::new(listener));
        self
    }
}

impl Styled for Input {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Element for Input {
    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        let this = *self;
        let id = wire_id(this.id);
        let field = this.field.unwrap_or_default();
        crate::text::lowered_generation(field.generation);
        let on_change = this
            .on_change
            .map(|listener| lowering.route(slots::Kind::Change, listener));
        let on_key = this
            .on_key
            .map(|listener| lowering.route(slots::Kind::Key, listener));
        let on_submit = this
            .on_submit
            .map(|listener| lowering.route(slots::Kind::Submit, listener));
        let claims = this.claims;
        debug_assert!(
            !claims.iter().any(wire::KeyClaim::engine_owned),
            "Backspace, Delete, Tab and the undo chords are the engine's: the host refuses a claim on them"
        );
        wire::Node::Field {
            id,
            multiline: this.multiline,
            value: field.text,
            cursor: field.cursor,
            generation: field.generation,
            revision: field.revision,
            tokens: field.tokens.into(),
            claims: claims.into(),
            options: this.options.into(),
            placeholder: this.placeholder,
            secure: this.secure,
            on_change,
            on_key,
            on_submit,
            style: lowering.style(&this.style),
        }
    }
}

impl IntoElement for Input {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl gpui::prelude::FluentBuilder for Input {}

/// The many-line [`Input`]: the host's native text area over a
/// [`TextField`](crate::TextField). Enter is a new line unless the view
/// claims it; a claimed key reaches `on_key` instead of editing.
pub struct Textarea(Input);

impl Textarea {
    pub fn new(
        id: impl Into<ElementId>,
        field: &crate::TextField,
        label: impl Into<String>,
    ) -> Self {
        let mut input = Input::new(id, label).value(field);
        input.multiline = true;
        Self(input)
    }

    pub fn placeholder(self, placeholder: impl Into<String>) -> Self {
        Self(self.0.placeholder(placeholder))
    }

    pub fn read_only(self, read_only: bool) -> Self {
        Self(self.0.read_only(read_only))
    }

    /// A key the view hears on `on_key` instead of the engine. The engine's
    /// own keys — Backspace, Delete, Tab, undo and redo — cannot be claimed.
    pub fn claim(mut self, claim: wire::KeyClaim) -> Self {
        self.0.claims.push(claim);
        self
    }

    pub fn claims(mut self, claims: impl IntoIterator<Item = wire::KeyClaim>) -> Self {
        self.0.claims.extend(claims);
        self
    }

    /// See [`Input::on_change`].
    pub fn on_change(
        self,
        listener: impl Fn(&wire::TextChange, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self(self.0.on_change(listener))
    }

    /// Hears every claimed key as the host receives it, text untouched.
    pub fn on_key(
        mut self,
        listener: impl Fn(&gpui::KeyDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.0.on_key = Some(Box::new(listener));
        self
    }
}

impl Styled for Textarea {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.0.style
    }
}

impl Element for Textarea {
    fn id(&self) -> Option<ElementId> {
        Element::id(&self.0)
    }

    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        Element::lower(Box::new(self.0), lowering)
    }
}

impl IntoElement for Textarea {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl gpui::prelude::FluentBuilder for Textarea {}

/// Add children to an element recipe.
pub trait ParentElement {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>);

    fn child(mut self, child: impl IntoElement) -> Self
    where
        Self: Sized,
    {
        self.extend(std::iter::once(child.into_any_element()));
        self
    }

    /// Adds each of `children`, as [`Self::child`] would.
    ///
    /// Children are not scopes of their own, as the rows of a `list` or a
    /// `uniform_list` are under their list's id: an id inside one child
    /// meets the same id inside the next. A row of data built here carries
    /// its own id (its item's key), which scopes the ids inside it. Two
    /// children named by one key are refused, naming the id and its scope.
    fn children(mut self, children: impl IntoIterator<Item = impl IntoElement>) -> Self
    where
        Self: Sized,
    {
        self.extend(children.into_iter().map(IntoElement::into_any_element));
        self
    }
}

impl ParentElement for Div {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Element for SharedString {
    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        wire::Node::Text(crate::wire::TextNode {
            id: None,
            style: lowering.no_style(),
            content: self.to_string(),
        })
    }
}

impl Element for &'static str {
    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        Element::lower(Box::new(SharedString::from(*self)), lowering)
    }
}

impl IntoElement for String {
    type Element = SharedString;

    fn into_element(self) -> Self::Element {
        self.into()
    }
}

impl IntoElement for &'static str {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl IntoElement for SharedString {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl IntoElement for Cow<'static, str> {
    type Element = SharedString;

    fn into_element(self) -> Self::Element {
        self.into()
    }
}

/// A one-shot component with the same call shape as GPUI's `RenderOnce`.
pub trait RenderOnce: 'static {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement;
}

mod uniform_list;
pub(crate) use uniform_list::UniformLists;
pub use uniform_list::{UniformList, UniformListScrollHandle, uniform_list};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod ancestry_tests;

#[cfg(test)]
mod bar_gutter_tests {
    use super::*;
    use crate::prelude::*;
    use gpui::px;

    #[test]
    fn a_scroller_keeps_its_right_edge_for_the_bar() {
        let bar = crate::design::size::SCROLLBAR;
        let mut inset = crate::div().id("s").overflow_y_scroll().p(px(8.));
        let mut style = inset.style().clone();
        bar_gutter(&mut style);
        assert_eq!(style.padding.right, Some(bar.into()));
        assert_eq!(
            style.padding.left,
            Some(px(8.).into()),
            "only the bar's edge"
        );
        let mut wide = crate::div().id("s").overflow_y_scroll().pr(px(24.));
        let mut style = wide.style().clone();
        bar_gutter(&mut style);
        assert_eq!(style.padding.right, Some(px(24.).into()));
        let mut still = crate::div().p(px(8.));
        let mut style = still.style().clone();
        bar_gutter(&mut style);
        assert_eq!(
            style.padding.right,
            Some(px(8.).into()),
            "no scroll, no bar"
        );
    }
}
