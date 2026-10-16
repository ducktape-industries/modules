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

/// The key of one part of page `id`: `{id}-{part}`.
fn named(id: &ElementId, part: &str) -> ElementId {
    ElementId::Name(format!("{id}-{part}").into())
}

/// One page's frame: a title row (`{id}-title`, [`Ladder::header`] tall)
/// with a Back control (`{id}-back`, [`Ladder::hit`] each way, named
/// "Back") when `back` is given, the heading (`{id}-heading`, truncating),
/// and what the page adds beside it ([`Page::account`], [`Page::leading`],
/// [`Page::caption`], [`Page::trailing`]), over the children, which fill
/// the rest.
///
/// Under a finger the row is the Expo kit's `Header`: 56 tall, 8 in, the
/// arrow-left Back, the title in [`Ladder::title`] medium and a hairline
/// under the row; a page with no Back is the view's first page, whose
/// title is [`Ladder::big_title`] with no hairline (the kit's module root).
pub fn page(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    back: Option<impl Fn(&ClickEvent, &mut Window, &mut App) + 'static>,
    ladder: &Ladder,
    theme: &Theme,
) -> Page {
    let id = id.into();
    let theme = *theme;
    let root = back.is_none();
    let back = back.map(|back| match ladder.touch {
        // the Expo kit's header: its `Hit` with the Lucide arrow
        true => super::icon_button(
            named(&id, "back"),
            super::icon("arrow-left", ladder.icon),
            "Back",
            ladder,
            &theme,
            back,
        )
        .into_any_element(),
        false => div()
            .id(named(&id, "back"))
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
    Page {
        id,
        root,
        title: title.into(),
        caption: None,
        back,
        account: None,
        leading: Vec::new(),
        trailing: Vec::new(),
        ladder: *ladder,
        theme,
        body: Vec::new(),
    }
}

/// A [`page`] being filled: its children go under the title row, in a
/// column that takes the rest of the page's height. The title row is
/// built when the page is drawn, so its slots can land after [`page`]:
/// Back, the reader's [`account`](Self::account) and
/// [`leading`](Self::leading) first, the words (the heading over a
/// [`caption`](Self::caption)) taking the slack and truncating, then
/// [`trailing`](Self::trailing), before what the row keeps clear for the
/// capsule under touch.
pub struct Page {
    id: ElementId,
    /// no Back: the view's first page
    root: bool,
    title: SharedString,
    caption: Option<SharedString>,
    back: Option<AnyElement>,
    account: Option<AnyElement>,
    leading: Vec<AnyElement>,
    trailing: Vec<AnyElement>,
    ladder: Ladder,
    theme: Theme,
    body: Vec<AnyElement>,
}

impl Page {
    /// A line under the title (`{id}-caption`, [`Ladder::caption`] size,
    /// truncating): a count, a state, who the page is with.
    pub fn caption(mut self, caption: impl Into<SharedString>) -> Self {
        self.caption = Some(caption.into());
        self
    }
    /// The reader's own avatar on a view's first page, under a finger: the
    /// Expo kit's module root (`Header.tsx:46-54`), so set it on the first
    /// page only. `{id}-account`, [`Ladder::avatar`] in a [`Ladder::hit`]
    /// box, named "Account", with no dim on a press (Expo's `Pressable`
    /// sets none). A press asks the host to bring up the reader's account
    /// (`host.show` [`Place::Account`], so the view declares
    /// `Capability::Host`) as a `notify`: a refusal (no account to show, a
    /// host older than `host.show`) is silent and the press does nothing.
    /// Under a pointer it draws nothing, as a pointer's first page is drawn
    /// like any other.
    ///
    /// `name` is the reader's account name, read as the view reads any
    /// account's name (the session names only the account number); while
    /// it is still being read it is empty, and the avatar draws `•`
    /// ([`initial`](super::initial)).
    pub fn account(mut self, name: &str) -> Self {
        if !self.ladder.touch {
            return self;
        }
        let show = |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            cx.host().notify::<HostShow>(Place::Account);
        };
        let avatar = div()
            .id(named(&self.id, "account"))
            .size(self.ladder.hit)
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .role(Role::Button)
            .aria_label("Account")
            .focusable()
            .on_click(show)
            .child(super::avatar(name, self.ladder.avatar, &self.theme));
        self.account = Some(avatar.into_any_element());
        self
    }
    /// An element before the heading, after Back and the account: another
    /// person's avatar (a DM room's peer).
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
                heading(named(&id, "heading"), self.title, 1, &theme)
                    .truncate()
                    .text_type(title)
                    .when(ladder.touch, |title| title.font_weight(FontWeight::MEDIUM)),
            )
            .children(self.caption.map(|caption| {
                div()
                    .id(named(&id, "caption"))
                    .text_type(ladder.caption)
                    .text_color(theme.muted)
                    .truncate()
                    .child(caption)
            }));
        let title_row = div()
            .id(named(&id, "title"))
            .flex_none()
            .min_h(ladder.header)
            .flex()
            .items_center()
            .map(|row| match ladder.touch {
                // its own band: the body scrolls under it, never through it
                true => row
                    .gap(space::XXS)
                    .px(space::SM)
                    .pr(CAPSULE_CLEARANCE)
                    .bg(theme.background)
                    .when(!root, |row| row.border_b_1().border_color(theme.border)),
                false => row.gap(space::SM).px(space::LG),
            })
            .children(self.back)
            .children(self.account)
            .map(|row| match ladder.touch {
                // the kit's title block: 8 in, an avatar 8 from the words
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
