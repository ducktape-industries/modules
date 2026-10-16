//! A write's form, as a sheet over the page: its title, the fields its
//! write reads (a name or a new path, the text, the message), what a
//! removal takes away, why the last commit did not land, then Cancel and
//! Commit. Reads the open form; writes its fields, the commit (`submit`)
//! and Cancel (`close_sheet`).
use view::Autocapitalize;
use view::accesskit::Invalid;
use view::design::space;

use super::*;
use crate::write::{Form, Write};

/// How tall the text field starts.
const TEXT_H: Pixels = px(220.);

pub(super) fn sheet(view: &Files, cx: Cx, k: &Kit) -> Option<AnyElement> {
    let form = view.form()?;
    let title = words::form_title(&form.write);
    let dismiss = cx.listener(|view: &mut Files, _: &(), _, cx| view.close_sheet(cx));
    let refused = form.refused.as_ref().map(|why| {
        div()
            .id("files/write/refused")
            .text_size(design::text::SECONDARY)
            .text_color(k.theme.danger)
            .role(Role::Alert)
            .aria_live(view::accesskit::Live::Assertive)
            .child(why.clone())
    });
    let removes = match &form.write {
        Write::Remove { path, folder } => Some(
            div()
                .text_color(k.theme.muted)
                .child(words::removes(path, *folder)),
        ),
        _ => None,
    };
    Some(
        design::sheet("files/write", title.clone(), &k.ladder, &k.theme)
            .on_dismiss(dismiss)
            .child(
                div()
                    .id("files/write/body")
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(design::heading("files/write/title", title, 2, &k.theme))
                    .children(removes)
                    .children(naming(form, cx, k))
                    .children(text(form, k))
                    .child(message(form, cx, k))
                    .children(refused)
                    .child(buttons(form, cx, k)),
            )
            .into_any_element(),
    )
}

/// A field under its label.
fn labelled(label: &'static str, field: impl IntoElement, k: &Kit) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_size(design::text::SECONDARY)
                .text_color(k.theme.muted)
                .child(label),
        )
        .child(field)
}

/// A new name, or a rename's new path; a problem with it is the field's,
/// said to assistive tech too. Return commits, unless the form takes text
/// still to be typed below it.
fn naming(form: &Form, cx: Cx, k: &Kit) -> Option<Div> {
    let naming = form.write.naming()?;
    let label = words::naming(naming);
    let input = Input::new("files/write/name", &form.name, label)
        .w_full()
        .font_family(design::fonts::FAMILY_MONO)
        .autocapitalize(Autocapitalize::None);
    let input = match form.write.takes_text() {
        true => input,
        false => input.on_submit(cx.listener(|view: &mut Files, _: &(), _, cx| view.submit(cx))),
    };
    let input = match &form.problem {
        Some(problem) => input.invalid(Invalid::True).description(problem.clone()),
        None => input,
    };
    let problem = form.problem.as_ref().map(|problem| {
        div()
            .id("files/write/problem")
            .text_size(design::text::SECONDARY)
            .text_color(k.theme.danger)
            .child(problem.clone())
    });
    Some(labelled(label, design::field(input, &k.ladder, &k.theme), k).children(problem))
}

/// A new file's text, or an edit's.
fn text(form: &Form, k: &Kit) -> Option<Div> {
    if !form.write.takes_text() {
        return None;
    }
    let area = Textarea::new("files/write/text", &form.text, words::TEXT)
        .w_full()
        .h(TEXT_H)
        .p(space::SM)
        .border_1()
        .border_color(k.theme.border_strong)
        .bg(k.theme.background)
        .font_family(design::fonts::FAMILY_MONO)
        .text_size(design::text::MONO);
    Some(labelled(words::TEXT, area, k))
}

/// The commit's message; its placeholder is what it says left empty.
fn message(form: &Form, cx: Cx, k: &Kit) -> Div {
    let submit = cx.listener(|view: &mut Files, _: &(), _, cx| view.submit(cx));
    let input = Input::new("files/write/message", &form.message, words::MESSAGE)
        .w_full()
        .placeholder(form.placeholder())
        .on_submit(submit);
    labelled(words::MESSAGE, design::field(input, &k.ladder, &k.theme), k)
}

/// Cancel, then Commit, which waits on its answer.
fn buttons(form: &Form, cx: Cx, k: &Kit) -> Div {
    let cancel = cx.listener(|view: &mut Files, _: &ClickEvent, _, cx| view.close_sheet(cx));
    let commit = cx.listener(|view: &mut Files, _: &ClickEvent, _, cx| view.submit(cx));
    let verb = words::commit(form.sending);
    div()
        .flex()
        .justify_end()
        .gap_2()
        .child(
            design::button(
                "files/write/cancel",
                words::CANCEL,
                &k.ladder,
                &k.theme,
                cancel,
            )
            .kind(design::Kind::Quiet),
        )
        .child(
            design::button("files/write/submit", verb, &k.ladder, &k.theme, commit)
                .kind(design::Kind::Primary)
                .enabled(!form.sending),
        )
}
