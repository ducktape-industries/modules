//! The pieces two or more phone screens draw: a page's frame and the line
//! the last write left, the Expo kit's `Row` (a lead glyph, a title over a
//! caption, a chevron), `KeyValue`, `Quiet`, the states a read is in, and
//! the kit pieces the SDK does not have yet. Each draws what it is handed.
use super::*;

/// A key's column in a key/value row (Expo `src/kit/Facts.tsx:64`).
const KEY_W: Pixels = px(96.);

// SDK gap: SHEET_INSET is private, swap when feat/phone-kit lands
/// How far a sheet's body sits in from its sides (Expo `Sheet.tsx:148`).
pub(super) const SHEET_INSET: Pixels = px(20.);

/// The room between a row's title and its caption (Expo `Row.tsx:58`).
const TITLE_GAP: Pixels = px(1.);

/// A page: its title row (Back by where it sits), the line the last write
/// left, then what the caller adds.
pub(super) fn page(
    id: &'static str,
    title: impl Into<SharedString>,
    view: &Files,
    cx: Cx,
    k: &Kit,
) -> design::Page {
    let nav = nav(view, cx);
    let page = design::page(id, title, nav, &k.ladder, &k.theme);
    match note(view, k) {
        Some(note) => page.child(note),
        None => page,
    }
}

/// The line the last write left (Expo `Notice`): a quiet strip under the
/// title row.
fn note(view: &Files, k: &Kit) -> Option<Stateful<Div>> {
    let note = view.note.as_ref()?;
    Some(
        div()
            .id("files-note")
            .px(space::BLOCK)
            .py(space::MD)
            .bg(k.theme.surface)
            .text_type(k.ladder.secondary)
            .text_color(k.theme.muted)
            .role(Role::Status)
            .aria_live(view::accesskit::Live::Polite)
            .child(words::note(note)),
    )
}

/// What scrolls under a page's title row.
pub(super) fn scroller(id: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .flex_1()
        .min_h(px(0.))
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .pb(space::BLOCK)
}

/// A row that opens a page (Expo `Row` with `chevron`): a glyph, its
/// title over its caption, the chevron; the whole row the press.
pub(super) fn opening_row(
    id: impl Into<ElementId>,
    glyph: &'static str,
    title: impl Into<SharedString>,
    caption: impl Into<SharedString>,
    k: &Kit,
    press: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let row = design::row(id, &k.ladder, &k.theme)
        .min_h(k.ladder.row_tall)
        .child(design::lead(
            design::icon(glyph, k.ladder.icon_row).text_color(k.theme.muted),
        ))
        .child(title_and_caption(row_title(title, k)).child(row_caption(caption, k)))
        .child(chevron(k));
    design::pressed(row, &k.ladder)
        .role(Role::Button)
        .focusable()
        .on_click(press)
}

/// A row that opens nothing: a glyph, its title over its caption.
pub(super) fn still_row(
    id: impl Into<ElementId>,
    glyph: &'static str,
    title: impl Into<SharedString>,
    caption: impl Into<SharedString>,
    k: &Kit,
) -> Stateful<Div> {
    design::row(id, &k.ladder, &k.theme)
        .min_h(k.ladder.row_tall)
        .child(design::lead(
            design::icon(glyph, k.ladder.icon_row).text_color(k.theme.faint),
        ))
        .child(title_and_caption(row_title(title, k)).child(row_caption(caption, k)))
}

/// A key and its value, on one row (Expo `KeyValue`).
pub(super) fn key_value(key: &'static str, value: impl IntoElement, k: &Kit) -> Div {
    div()
        .flex()
        .items_start()
        .gap(space::LG)
        .px(space::BLOCK)
        .py(space::MD)
        .border_b_1()
        .border_color(k.theme.border)
        .child(
            div()
                .w(KEY_W)
                .flex_none()
                .text_type(k.ladder.small)
                .text_color(k.theme.muted)
                .child(key),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_type(k.ladder.small)
                .child(value),
        )
}

/// Text in the data face: a path, an id.
pub(super) fn data(text: impl Into<SharedString>, k: &Kit) -> Div {
    div()
        .font_family(design::fonts::FAMILY_MONO)
        .text_type(k.ladder.code)
        .child(text.into())
}

/// One quiet line (Expo `Quiet`): a page still reading, a list with
/// nothing to say.
pub(super) fn quiet(id: &'static str, sentence: impl Into<SharedString>, k: &Kit) -> AnyElement {
    div()
        .id(id)
        .p(space::BLOCK)
        .text_type(k.ladder.secondary)
        .text_color(k.theme.muted)
        .child(sentence.into())
        .into_any_element()
}

/// Nothing here yet: what is missing, then what would fill it.
pub(super) fn empty(
    id: &'static str,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    k: &Kit,
) -> AnyElement {
    design::empty_state(id, title, detail, &k.theme).into_any_element()
}

/// A refused read, and Retry.
pub(super) fn refused(name: &str, refusal: &Error, cx: Cx, k: &Kit) -> AnyElement {
    let retry = cx.listener(|view: &mut Files, _: &ClickEvent, _, cx| view.read_page(cx));
    design::refused(name, refusal.message.clone(), &k.ladder, &k.theme, retry)
        .m(space::BLOCK)
        .into_any_element()
}

/// The page's writes behind one control in its title row, when it has
/// any (Expo `Header` right `Hit` with `MoreHorizontal`).
pub(super) fn actions_control(view: &Files, cx: Cx, k: &Kit) -> Option<AnyElement> {
    if view.offered().is_empty() {
        return None;
    }
    let open = cx.listener(|view: &mut Files, _: &ClickEvent, _, cx| view.open_actions(cx));
    Some(
        design::icon_button(
            "files/actions",
            design::icon("ellipsis", k.ladder.icon),
            words::ACTIONS,
            &k.ladder,
            &k.theme,
            open,
        )
        .into_any_element(),
    )
}

// SDK gap: design::title_and_caption, swap when feat/phone-kit lands
/// A row's words (Expo `Row.tsx:58-73`): the title, and what the caller
/// adds 1 under it.
pub(super) fn title_and_caption(title: impl IntoElement) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(TITLE_GAP)
        .child(title)
}

// SDK gap: design::row_title, swap when feat/phone-kit lands
/// A row's title (Expo `Row.tsx:59-62`): one line of body text.
pub(super) fn row_title(text: impl Into<SharedString>, k: &Kit) -> Div {
    div()
        .truncate()
        .text_type(k.ladder.body)
        .text_color(k.theme.foreground)
        .child(text.into())
}

// SDK gap: design::row_caption, swap when feat/phone-kit lands
/// A row's caption (Expo `Row.tsx:66-69`): one line of caption, muted.
pub(super) fn row_caption(text: impl Into<SharedString>, k: &Kit) -> Div {
    div()
        .truncate()
        .text_type(k.ladder.caption)
        .text_color(k.theme.muted)
        .child(text.into())
}

// SDK gap: design::chevron, swap when feat/phone-kit lands
/// The chevron at the end of a row that opens a page (Expo `Row.tsx:75`).
pub(super) fn chevron(k: &Kit) -> view::Svg {
    design::icon("chevron-right", k.ladder.icon_row).text_color(k.theme.faint)
}

// SDK gap: design::sheet_title, swap when feat/phone-kit lands
/// A form sheet's head (Expo `SheetHead` holding the form's title): a
/// level-2 heading in title medium.
pub(super) fn sheet_title(
    id: &'static str,
    title: impl Into<SharedString>,
    k: &Kit,
) -> Stateful<Div> {
    design::sheet_head(id, &k.theme).child(
        design::heading("files/write/title", title, 2, &k.theme)
            .text_type(k.ladder.title)
            .font_weight(FontWeight::MEDIUM),
    )
}

// SDK gap: design::field with .label and .problem, swap when feat/phone-kit lands
/// A field under its label (Expo `Field label`), the problem with its
/// value under it and said to assistive tech.
pub(super) fn labelled(label: &'static str, input: Input, problem: Option<&str>, k: &Kit) -> Div {
    let input = match problem {
        Some(problem) => input
            .invalid(view::accesskit::Invalid::True)
            .description(problem.to_owned()),
        None => input,
    };
    let edge = match problem {
        Some(_) => k.theme.danger,
        None => k.theme.border_strong,
    };
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .mb(space::XS)
                .text_type(k.ladder.caption)
                .text_color(k.theme.muted)
                .child(label),
        )
        .child(design::field(input, &k.ladder, &k.theme).border_color(edge))
        .children(problem.map(|problem| {
            div()
                .id("files/write/problem")
                .mt(space::XS)
                .text_type(k.ladder.caption)
                .text_color(k.theme.danger)
                .child(problem.to_owned())
        }))
}

// SDK gap: design::form_verbs, swap when feat/phone-kit lands
/// A form sheet's foot (Expo `Agents.tsx:224-232`): Cancel quiet, then
/// the verb as the primary, taking the rest of the row.
pub(super) fn form_verbs(cancel: impl IntoElement, verb: impl IntoElement) -> Div {
    div()
        .flex()
        .gap(space::SM)
        .child(cancel)
        .child(div().flex_1().flex().flex_col().child(verb))
}
