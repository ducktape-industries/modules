//! The one route model a view has at every width: its [`Pages`] stack,
//! and the [`page`] frame one page is drawn in under a Compact body.
use serde::{Deserialize, Serialize};

use super::{Ladder, TextType, heading, space};
use crate::prelude::*;
use crate::{AnyElement, Div, FontWeight, Stateful};
use gpui::SharedString;

/// The view's own route stack, newest last: part of the view's state, so
/// it snapshots and restores with it. One stack drives every width: a
/// Compact body shows [`top`](Self::top) alone and Back pops; a wider body
/// reads the same `top()` into its panes, so a resize across the Compact
/// edge loses nothing. gpui's own pattern (a view owns its state and
/// renders it), not a router.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct Pages<P> {
    /// Never empty: a restore of an empty stack is refused, as `top()`
    /// has nothing to answer and `depth()` nothing to count from.
    #[serde(deserialize_with = "rooted")]
    stack: Vec<P>,
}

fn rooted<'de, P: Deserialize<'de>, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<P>, D::Error> {
    let stack = Vec::<P>::deserialize(deserializer)?;
    match stack.is_empty() {
        true => Err(serde::de::Error::custom("a page stack has its root")),
        false => Ok(stack),
    }
}

impl<P> Pages<P> {
    /// A stack with `page` at its root.
    pub fn root(page: P) -> Self {
        Self { stack: vec![page] }
    }
    pub fn push(&mut self, page: P) {
        self.stack.push(page);
    }
    /// The top page, popped; never the root, which stays and answers
    /// `None`. A Back that arrives a tick late (JSC lands a frame behind)
    /// is therefore a no-op at the root.
    pub fn pop(&mut self) -> Option<P> {
        (self.stack.len() > 1).then(|| self.stack.pop()).flatten()
    }
    /// Replaces the top page: a list's selection moving while a detail is
    /// open.
    pub fn replace(&mut self, page: P) {
        *self.stack.last_mut().expect("a stack has its root") = page;
    }
    pub fn top(&self) -> &P {
        self.stack.last().expect("a stack has its root")
    }
    /// `0` at the root: `.when(pages.depth() > 0, |root| root.on_back(..))`
    /// sets Back exactly when there is somewhere to go back to.
    pub fn depth(&self) -> usize {
        self.stack.len() - 1
    }
    /// Root first.
    pub fn iter(&self) -> impl Iterator<Item = &P> {
        self.stack.iter()
    }
}

/// What the title row keeps clear on the right, under touch, for the
/// capsule the host floats over a running program (the owner's call of
/// 2026-10-15: a capsule only, no host bar; the view's own title row draws
/// Back).
pub const CAPSULE_CLEARANCE: Pixels = px(96.);

/// One page's frame: a title row (`{id}-title`, [`Ladder::header`] tall)
/// with the heading (`{id}-heading`, truncating), what the page adds
/// beside it ([`Page::back`], [`Page::leading`], [`Page::caption`],
/// [`Page::trailing`]), over the children, which fill the rest:
/// `design::page("rooms", "Chat", &ladder, &theme).root()` for a view's
/// first page, `design::page("room", name, &ladder, &theme).back(pop)` for
/// one under it.
///
/// Under a finger the row is the Expo kit's `Header` (`Header.tsx:28-73`):
/// 56 tall, 8 in, 4 between its parts, a hairline under it, the title in
/// [`Ladder::title`] medium.
pub fn page(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    ladder: &Ladder,
    theme: &Theme,
) -> Page {
    Page {
        id: id.into(),
        root: false,
        title: title.into(),
        caption: None,
        back: None,
        leading: Vec::new(),
        trailing: Vec::new(),
        ladder: *ladder,
        theme: *theme,
        body: Vec::new(),
    }
}

/// A [`page`] being filled: its children go under the title row, in a
/// column that takes the rest of the page's height. The title row is
/// built when the page is drawn, so its slots can land after [`page`]:
/// [`back`](Self::back) and [`leading`](Self::leading) first, the words
/// (the heading over a [`caption`](Self::caption)) taking the slack and
/// truncating, then [`trailing`](Self::trailing), before what the row
/// keeps clear for the capsule under touch.
pub struct Page {
    id: ElementId,
    /// the view's first page ([`Page::root`])
    root: bool,
    title: SharedString,
    caption: Option<SharedString>,
    back: Option<AnyElement>,
    leading: Vec<AnyElement>,
    trailing: Vec<AnyElement>,
    ladder: Ladder,
    theme: Theme,
    body: Vec<AnyElement>,
}

impl Page {
    /// The way back to the page under this one: a control (`{id}-back`,
    /// [`Ladder::hit`] each way, named "Back") that runs `back`. Under a
    /// finger it is the Expo kit's header `Hit` with the Lucide arrow
    /// (`Header.tsx:45`); under a pointer a `‹` at the title size.
    pub fn back(mut self, back: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        let (ladder, theme) = (self.ladder, self.theme);
        let id = ElementId::Name(format!("{}-back", self.id).into());
        self.back = Some(match ladder.touch {
            true => super::icon_button(
                id,
                super::icon("arrow-left", ladder.icon),
                "Back",
                &ladder,
                &theme,
                back,
            )
            .into_any_element(),
            false => div()
                .id(id)
                .min_w(ladder.hit)
                .min_h(ladder.hit)
                .flex()
                .items_center()
                .justify_center()
                .text_type(ladder.title)
                .text_color(theme.muted)
                .cursor_pointer()
                .hover(move |style| style.text_color(theme.foreground))
                .role(Role::Button)
                .aria_label("Back")
                .focusable()
                .on_click(back)
                .child("‹")
                .into_any_element(),
        });
        self
    }
    /// The page is the view's first page (the Expo kit's `Header` `root`,
    /// `Header.tsx:19-23`): under a finger its title is
    /// [`Ladder::big_title`] and its hairline clear (`Header.tsx:41`,
    /// `:59`). A pointer draws it as any other page.
    pub fn root(mut self) -> Self {
        self.root = true;
        self
    }
    /// A line under the title (`{id}-caption`, [`Ladder::caption`] size,
    /// truncating): a count, a state, who the page is with.
    pub fn caption(mut self, caption: impl Into<SharedString>) -> Self {
        self.caption = Some(caption.into());
        self
    }
    /// An element before the heading, after Back: a person's avatar.
    pub fn leading(mut self, element: impl IntoElement) -> Self {
        self.leading.push(element.into_any_element());
        self
    }
    /// A control of the page itself at the row's end (New, Copy, Edit,
    /// a search, ‹ ›), sized by the caller to the ladder.
    pub fn trailing(mut self, control: impl IntoElement) -> Self {
        self.trailing.push(control.into_any_element());
        self
    }
}

impl ParentElement for Page {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.body.extend(elements);
    }
}

impl IntoElement for Page {
    type Element = Stateful<Div>;
    fn into_element(self) -> Self::Element {
        let (ladder, theme, id, root) = (self.ladder, self.theme, self.id, self.root);
        let named = |suffix: &str| ElementId::Name(format!("{id}-{suffix}").into());
        let title = match (ladder.touch, root) {
            (true, true) => ladder.big_title,
            _ => ladder.title,
        };
        let words = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(
                heading(named("heading"), self.title, 1, &theme)
                    .truncate()
                    .text_type(title)
                    .when(ladder.touch, |title| title.font_weight(FontWeight::MEDIUM)),
            )
            .children(self.caption.map(|caption| {
                div()
                    .id(named("caption"))
                    .text_type(ladder.caption)
                    .text_color(theme.muted)
                    .truncate()
                    .child(caption)
            }));
        let title_row = div()
            .id(named("title"))
            .flex_none()
            .min_h(ladder.header)
            .flex()
            .items_center()
            .map(|row| match ladder.touch {
                // its own band (`Header.tsx:33-43`): the body scrolls under
                // it, never through it; a root's hairline is there but clear
                true => row
                    .gap(space::XXS)
                    .px(space::SM)
                    .pr(CAPSULE_CLEARANCE)
                    .bg(theme.background)
                    .border_b_1()
                    .border_color(match root {
                        true => crate::hsla(0., 0., 0., 0.),
                        false => theme.border,
                    }),
                false => row.gap(space::SM).px(space::LG),
            })
            .children(self.back)
            .map(|row| match ladder.touch {
                // the kit's title block (`Header.tsx:57`): 8 in, an avatar 8
                // from the words
                true => row.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(space::SM)
                        .px(space::SM)
                        .children(self.leading)
                        .child(words),
                ),
                false => row.children(self.leading).child(words),
            })
            .children(self.trailing);
        div()
            .id(id)
            .flex()
            .flex_col()
            .size_full()
            .child(title_row)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .children(self.body),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stack_never_pops_its_root() {
        let mut pages = Pages::root("rooms");
        assert_eq!(pages.depth(), 0);
        assert_eq!(pages.pop(), None);
        pages.push("room");
        pages.push("thread");
        assert_eq!(pages.depth(), 2);
        assert_eq!(pages.top(), &"thread");
        assert_eq!(pages.pop(), Some("thread"));
        pages.replace("details");
        assert_eq!(
            pages.iter().copied().collect::<Vec<_>>(),
            ["rooms", "details"]
        );
        assert_eq!(pages.pop(), Some("details"));
        assert_eq!(pages.pop(), None);
        assert_eq!(pages.top(), &"rooms");
        let bytes = serde_json::to_string(&pages).unwrap();
        assert_eq!(serde_json::from_str::<Pages<&str>>(&bytes).unwrap(), pages);
        assert!(serde_json::from_str::<Pages<&str>>(r#"{"stack":[]}"#).is_err());
    }
}
