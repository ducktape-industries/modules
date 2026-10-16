//! The sheets over a screen: the page's writes as a list (Expo `Sheet`
//! with `SheetRow`s), and one write's form (Expo `Sheet` holding a form:
//! its title, its fields, Cancel and Commit). New design in the kit's
//! look; nearest Expo screen
//! `mobile/app/(tabs)/forge/[repo]/code/[...path].tsx`. Reads the page's
//! writes and the open form; writes the form's fields, `begin`, `submit`
//! and `close_sheet`.
use view::Autocapitalize;

use super::*;
use crate::write::{Form, Write, built};

/// Where a sheet stops when it opens, and how far it may be pulled.
const DETENTS: [f32; 2] = [0.5, 1.0];

/// The page's writes, each opening its form; none to offer, no sheet.
pub(super) fn actions(view: &Files, cx: Cx, k: &Kit) -> Option<AnyElement> {
    let offered = view.offered();
    if offered.is_empty() {
        return None;
    }
    let dismiss = cx.listener(|view: &mut Files, _: &(), _, cx| view.close_sheet(cx));
    let rows = offered.into_iter().map(|write| {
        let id = format!("files/write/{}", write.slug());
        let danger = matches!(write, Write::Remove { .. });
        let glyph = words::verb_glyph(&write);
        let label = words::verb(&write);
        let begin = cx
            .listener(move |view: &mut Files, _: &ClickEvent, _, cx| view.begin(write.clone(), cx));
        design::sheet_row(id, Some(glyph), label, danger, &k.ladder, &k.theme, begin)
    });
    Some(
        design::sheet("files/actions-sheet", words::ACTIONS, &k.ladder, &k.theme)
            .detents(DETENTS)
            .on_dismiss(dismiss)
            .child(div().flex().flex_col().pb(space::BLOCK).children(rows))
            .into_any_element(),
    )
}

/// One write's form.
pub(super) fn form(view: &Files, form: &Form, cx: Cx, k: &Kit) -> AnyElement {
    let title = words::form_title(&form.write);
    let dismiss = cx.listener(|view: &mut Files, _: &(), _, cx| view.close_sheet(cx));
    let removes = match &form.write {
        Write::Remove { path } => Some(
            div()
                .text_type(k.ladder.secondary)
                .text_color(k.theme.muted)
                .child(words::removes(
                    path,
                    matches!(view.page(), Route::Folder(_)),
                )),
        ),
        _ => None,
    };
    let refused = form.refused.as_ref().map(|why| {
        div()
            .id("files/write/refused")
            .text_type(k.ladder.secondary)
            .text_color(k.theme.danger)
            .role(Role::Alert)
            .aria_live(view::accesskit::Live::Assertive)
            .child(why.clone())
    });
    let body = div()
        .flex()
        .flex_col()
        .gap(space::LG)
        .p(SHEET_INSET)
        .children(removes)
        .children(naming(form, cx, k))
        .children(text(form, k))
        .child(message(form, cx, k))
        .children(refused)
        .child(verbs(form, cx, k));
    design::sheet("files/write", title.clone(), &k.ladder, &k.theme)
        .detents(DETENTS)
        .on_dismiss(dismiss)
        .child(
            div()
                .id("files/write/body")
                .size_full()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .child(sheet_title("files/write/head", title, k))
                .child(body),
        )
        .into_any_element()
}

/// A new name, or a rename's new path.
fn naming(form: &Form, cx: Cx, k: &Kit) -> Option<Div> {
    let naming = form.write.naming()?;
    let label = words::naming(naming);
    let submit = cx.listener(|view: &mut Files, _: &(), _, cx| view.submit(cx));
    let input = Input::new("files/write/name", &form.name, label)
        .w_full()
        .autocapitalize(Autocapitalize::None)
        .autocorrect(false)
        .on_submit(submit);
    Some(labelled(label, input, form.problem.as_deref(), k))
}

/// How tall the text field starts: about ten lines.
const TEXT_H: Pixels = px(200.);

/// A new file's text, or an edit's: many lines, so Return is a new line
/// and Commit is the only send.
fn text(form: &Form, k: &Kit) -> Option<Div> {
    if !form.write.takes_text() {
        return None;
    }
    let area = Textarea::new("files/write/text", &form.text, words::TEXT)
        .w_full()
        .h(TEXT_H)
        .px(space::LG)
        .py(space::MD)
        .border_1()
        .border_color(k.theme.border_strong)
        .bg(k.theme.background)
        .font_family(design::fonts::FAMILY_MONO)
        .text_type(k.ladder.code)
        .autocapitalize(Autocapitalize::None)
        .autocorrect(false);
    Some(
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .mb(space::XS)
                    .text_type(k.ladder.caption)
                    .text_color(k.theme.muted)
                    .child(words::TEXT),
            )
            .child(area),
    )
}

/// The commit's message; its placeholder is what it says left empty.
fn message(form: &Form, cx: Cx, k: &Kit) -> Div {
    let submit = cx.listener(|view: &mut Files, _: &(), _, cx| view.submit(cx));
    let said = built(form).map_or_else(|_| words::MESSAGE.to_owned(), |(message, _)| message);
    let input = Input::new("files/write/message", &form.message, words::MESSAGE)
        .w_full()
        .placeholder(said)
        .on_submit(submit);
    labelled(words::MESSAGE, input, None, k)
}

/// Cancel, then Commit, which waits on its answer.
fn verbs(form: &Form, cx: Cx, k: &Kit) -> Div {
    let cancel = cx.listener(|view: &mut Files, _: &ClickEvent, _, cx| view.close_sheet(cx));
    let commit = cx.listener(|view: &mut Files, _: &ClickEvent, _, cx| view.submit(cx));
    let verb = match form.sending {
        true => words::COMMITTING,
        false => words::COMMIT,
    };
    form_verbs(
        design::button(
            "files/write/cancel",
            words::CANCEL,
            &k.ladder,
            &k.theme,
            cancel,
        )
        .kind(design::Kind::Quiet),
        design::button("files/write/submit", verb, &k.ladder, &k.theme, commit)
            .kind(design::Kind::Primary)
            .enabled(!form.sending),
    )
}
