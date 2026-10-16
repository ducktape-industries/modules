//! New change, or Edit change #n (Expo
//! `app/(tabs)/forge/[repo]/changes/new.tsx`, board 72): its source and
//! target with how they compare, the title, the body, the reviewers; Open
//! (or Save) in the title row. The source and target are the ones the
//! form was opened with (a ref's Compare, the change edited).
//!
//! Reads the change form and the comparison of its endpoints; writes the
//! form, and opens or saves the change (`submit_change`).
use view::design::Ladder;
use view::prelude::*;
use view::{Div, FontWeight, Stateful, wire};

use super::parts;
use crate::Forge;
use crate::id;
use crate::state::{ChangeForm, Sheet, change_key};
use crate::words::{self, ref_label};
use forge::{Query, Reply, Revision};

/// A removable chip's cross (`Choice.tsx` `RemovableChip`).
const CHIP_CROSS: Pixels = px(14.);

pub(super) fn render(
    forge: &Forge,
    form: &ChangeForm,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    let (title, verb, scope) = match form.edit {
        Some(n) => (words::edit_change(n), "Save", change_key(&form.repo, n)),
        None => (words::NEW_CHANGE.to_owned(), "Open", "changes".to_owned()),
    };
    let back = cx.listener(|forge, _: &ClickEvent, _, cx| forge.cancel_change(cx));
    let body = div()
        .id(id("forge-change-form"))
        .flex_1()
        .min_h(px(0.))
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap(design::space::XL)
        .p(design::space::BLOCK)
        .child(endpoints(forge, form, ladder, theme))
        .child(title_field(form, cx, ladder, theme))
        .child(body_field(form, ladder, theme))
        .child(reviewers(forge, form, cx, ladder, theme));
    design::page("forge-page-change-form", title, Some(back), ladder, theme)
        .caption(form.repo.clone())
        .trailing(send(verb, !parts::sending(forge), cx, ladder, theme))
        .children(parts::notices(forge, ladder, theme))
        .children(parts::pending(forge, &scope, ladder, theme))
        .child(body)
        .into_any_element()
}

/// Open or Save in the title row (`HitText`): medium, a finger's height,
/// faint while a write of this change is on its way.
fn send(
    verb: &'static str,
    ready: bool,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    let submit = cx.listener(|forge, _: &ClickEvent, _, cx| forge.submit_change(cx));
    let words = div()
        .id(id("forge-change-send"))
        .h(ladder.hit)
        .flex()
        .flex_none()
        .items_center()
        .px(design::space::SM)
        .font_weight(FontWeight::MEDIUM)
        .text_color(if ready { theme.foreground } else { theme.faint })
        .role(Role::Button)
        .child(verb);
    match ready {
        true => words
            .active(|style| style.opacity(0.6))
            .focusable()
            .on_click(submit),
        false => words.aria_disabled(true),
    }
}

/// "Source → target": the two refs as the form holds them (off: the form
/// keeps the endpoints it was opened with), and how they compare.
fn endpoints(forge: &Forge, form: &ChangeForm, ladder: &Ladder, theme: &Theme) -> Div {
    let source = match form.from.is_empty() {
        true => "Pick a source".to_owned(),
        false => ref_label(&form.from),
    };
    let fixed = |key: &'static str, label: String| {
        parts::small_button(
            id(key),
            label,
            ladder,
            theme,
            |_: &ClickEvent, _: &mut Window, _: &mut App| {},
        )
        .icon("git-branch")
        .enabled(false)
    };
    let pair = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(design::space::LG)
        .child(fixed("forge-change-from", source))
        .child(design::icon("arrow-right", ladder.icon_row).text_color(theme.foreground))
        .child(fixed("forge-change-into", ref_label(&form.into)));
    div()
        .flex()
        .flex_col()
        .gap(design::space::SM)
        .child(parts::caption("Source → target", ladder, theme))
        .child(pair)
        .children(comparison(forge, form).map(|said| parts::caption(said, ladder, theme)))
}

/// `N ahead · M behind · <word>` for a new change whose endpoints have
/// been compared.
fn comparison(forge: &Forge, form: &ChangeForm) -> Option<String> {
    if form.edit.is_some() {
        return None;
    }
    let query = Query::Compare {
        repo: form.repo.clone(),
        from: Revision::Ref(form.from.clone()),
        into: Revision::Ref(form.into.clone()),
    };
    let Reply::Compare { comparison, .. } = forge.ready(&query)? else {
        return None;
    };
    Some(words::compared(comparison))
}

/// "Title", and why the form was refused; typing takes the refusal away.
fn title_field(form: &ChangeForm, cx: &mut Context<Forge>, ladder: &Ladder, theme: &Theme) -> Div {
    let typed = cx.listener(|forge, _: &wire::TextChange, _, cx| {
        if let Some(form) = &mut forge.form {
            form.error.clear();
        }
        cx.notify();
    });
    let input = Input::new(id("forge-change-title"), &form.title, "Title")
        .autocorrect(true)
        .autocapitalize(Autocapitalize::Sentences)
        .placeholder(words::TITLE_PROMPT)
        .on_change(typed);
    parts::input_field(Some("Title"), input, (&form.error, None), ladder, theme)
}

/// "Body": many lines.
fn body_field(form: &ChangeForm, ladder: &Ladder, theme: &Theme) -> Div {
    let field = Textarea::new(id("forge-change-body"), &form.body, "Body")
        .autocorrect(true)
        .autocapitalize(Autocapitalize::Sentences)
        .placeholder(words::BODY_PROMPT);
    parts::text_area_field(Some("Body"), field, "", ladder, theme)
}

/// "Reviewers": a chip each that removes them, and Add.
fn reviewers(
    forge: &Forge,
    form: &ChangeForm,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> Div {
    let chips = form.reviewers.iter().map(|who| {
        let name = forge.principal_name(who);
        let gone = who.clone();
        let remove = cx.listener(move |forge, _: &ClickEvent, _, cx| {
            if let Some(form) = &mut forge.form {
                form.reviewers.retain(|kept| *kept != gone);
            }
            cx.notify();
        });
        removable_chip(&name, ladder, theme, remove)
    });
    let add = cx.listener(|forge, _: &ClickEvent, _, cx| forge.open_sheet(Sheet::Reviewers, cx));
    let row = div()
        .flex()
        .flex_wrap()
        .gap(design::space::SM)
        .children(chips)
        .child(
            parts::small_button(id("forge-change-add-reviewer"), "Add", ladder, theme, add)
                .icon("plus"),
        );
    div()
        .flex()
        .flex_col()
        .gap(design::space::SM)
        .child(parts::caption("Reviewers", ladder, theme))
        .child(row)
}

/// A reviewer's chip (`RemovableChip`): the name and a cross in an
/// outline box; the press removes them.
// SDK gap: the removable chip, swap when feat/phone-kit lands
fn removable_chip(
    name: &str,
    ladder: &Ladder,
    theme: &Theme,
    remove: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let label = format!("Remove {name}");
    div()
        .relative()
        .h(parts::CHIP)
        .flex()
        .flex_none()
        .items_center()
        .gap(design::space::XS)
        .px(design::space::MD)
        .border_1()
        .border_color(theme.border_strong)
        .text_type(ladder.secondary)
        .whitespace_nowrap()
        .child(name.to_owned())
        .child(design::icon("x", CHIP_CROSS).text_color(theme.foreground))
        // the press reaches a finger's height, across the chip's width
        .child(
            design::hit(
                id(format!("forge-reviewer-{name}")),
                parts::CHIP,
                parts::CHIP,
                ladder,
            )
            .left_0()
            .right_0()
            .role(Role::Button)
            .aria_label(label)
            .focusable()
            .on_click(remove),
        )
}

/// A member to ask for a review (`PeopleSheet` "Reviewers"): everyone
/// not asked yet.
pub(super) fn reviewers_sheet(
    forge: &Forge,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    let skip = forge
        .form
        .as_ref()
        .map(|form| form.reviewers.clone())
        .unwrap_or_default();
    let ask = parts::PeopleAsk {
        key: "forge-reviewers-sheet",
        title: "Reviewers",
        caption: None,
        skip: &skip,
    };
    let add = |forge: &mut Forge, person: forge::Principal, cx: &mut Context<Forge>| {
        if let Some(form) = &mut forge.form {
            form.reviewers.push(person);
        }
        cx.notify();
    };
    parts::people_sheet(ask, add, forge, cx, ladder, theme)
}
