//! Expo `src/network/explorer/programs.tsx` `FormSheet` (mobile dev
//! 266633c): the open form as a sheet over the Programs page or a program's
//! page. Its title, the fields its edit reads, its notes, why it builds
//! nothing, Close and Schedule or Propose. Reads the form (`acting.form`),
//! the programs role, the accounts and the tip; writes the fields, the
//! program or view choice (`add_view`), the send (`submit_form`) and Close
//! (`close_form`).
use view::{Div, FontWeight};
use view::design::space;
use view::prelude::*;

use crate::{Cx, Explorer};
use crate::acting::{Asked, Edit, Form, FormField, Problem, Takes};
use crate::words;

use super::Look;

// SDK gap: the sheet's inset, swap when feat/phone-kit lands
/// The form's inset (Expo `programs.tsx:465`).
const FORM_INSET: Pixels = px(20.);

pub(super) fn sheet(view: &Explorer, cx: Cx, k: &Look) -> Option<AnyElement> {
    let form = view.acting.form.as_ref()?;
    let network = view.network.ready()?;
    let tip = view.status.ready().map_or(0, |status| status.height);
    let notes = account_note(view, form)
        .into_iter()
        .chain(form.notes(network, tip))
        .map(|note| {
            div()
                .text_type(k.ladder.secondary)
                .text_color(k.theme.muted)
                .child(note)
        });
    let problem = form.problem.as_ref().map(|problem| {
        div()
            .id("explorer-form-problem")
            .text_type(k.ladder.secondary)
            .text_color(k.theme.danger)
            .child(problem.sentence.clone())
    });
    let dismiss = cx.listener(|view: &mut Explorer, _: &(), _, cx| view.close_form(cx));
    let body = div()
        .flex()
        .flex_col()
        .gap(space::LG)
        .p(FORM_INSET)
        .children(fields(form, cx, k))
        .children(notes)
        .children(problem)
        .child(buttons(view, form, cx, k));
    Some(
        design::sheet("explorer-form-sheet", form.title(), &k.ladder, &k.theme)
            .on_dismiss(dismiss)
            .child(
                div()
                    .id("explorer-form")
                    .size_full()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    // SDK gap: `sheet_title`, swap when feat/phone-kit lands
                    .child(
                        design::sheet_head("explorer-form-head", &k.theme).child(
                            design::heading("explorer-form-title", form.title(), 2, &k.theme)
                                .text_type(k.ladder.title)
                                .font_weight(FontWeight::MEDIUM),
                        ),
                    )
                    .child(body),
            )
            .into_any_element(),
    )
}

/// The account a grant names, as typed (Expo's first grant note): its
/// number and name, or why there is none.
fn account_note(view: &Explorer, form: &Form) -> Option<String> {
    if form.edit != Edit::Grant {
        return None;
    }
    Some(match form.account_number() {
        None => words::TYPE_ACCOUNT.into(),
        Some(number) => match view.account(number) {
            Some(account) => format!("#{number} · {}", account.card.name),
            None => words::no_account_numbered(number),
        },
    })
}

/// What its edit asks for, in the Expo form's order.
fn fields(form: &Form, cx: Cx, k: &Look) -> Vec<AnyElement> {
    form.asks()
        .into_iter()
        .map(|asked| match asked {
            Asked::Field(field) => labelled(form, field, k),
            Asked::Kind => program_or_view(form, cx, k),
        })
        .collect()
}

// SDK gap: a labelled field with its problem, swap when feat/phone-kit lands
/// One labelled field (Expo `Field label` + `mono`): the label over a
/// strong hairline box, the text in the data face, the keys it takes; a
/// problem that is the field's marks it invalid and says why to assistive
/// tech.
fn labelled(form: &Form, field: FormField, k: &Look) -> AnyElement {
    let input = Input::new(field.id(), form.text(field), field.label())
        .w_full()
        .font_family(design::fonts::FAMILY_MONO)
        .placeholder(field.hint());
    let input = match field.takes() {
        Takes::Number => input.virtual_keyboard(VirtualKeyboard::Numeric),
        Takes::Id => input
            .autocapitalize(Autocapitalize::None)
            .autocorrect(false),
        Takes::Hex => input.autocapitalize(Autocapitalize::None),
    };
    let input = match Problem::of(form.problem.as_ref(), field) {
        Some(sentence) => input
            .invalid(view::accesskit::Invalid::True)
            .description(sentence.to_owned()),
        None => input,
    };
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .mb(space::XS)
                .text_type(k.ladder.caption)
                .text_color(k.theme.muted)
                .child(field.label()),
        )
        .child(design::field(input, &k.ladder, &k.theme).text_type(k.ladder.secondary))
        .into_any_element()
}

/// What an Add adds: a program, or a view (Expo `Segmented`).
fn program_or_view(form: &Form, cx: Cx, k: &Look) -> AnyElement {
    let pick =
        cx.processor(|view: &mut Explorer, index: usize, _, cx| view.add_view(index == 1, cx));
    design::segmented(
        "explorer-form-kind",
        "What it adds",
        &k.ladder,
        &k.theme,
        Some(usize::from(form.view)),
        [
            (
                ElementId::from("explorer-form-program"),
                SharedString::from("program"),
            ),
            (
                ElementId::from("explorer-form-view"),
                SharedString::from("view"),
            ),
        ],
        pick,
    )
    .into_any_element()
}

// SDK gap: the form sheet's Close + primary footer, swap when feat/phone-kit lands
/// Close, quiet; and Schedule or Propose, the primary, off while a write is
/// out.
fn buttons(view: &Explorer, form: &Form, cx: Cx, k: &Look) -> Div {
    let send = cx.listener(|view: &mut Explorer, _: &ClickEvent, _, cx| view.submit_form(cx));
    let close = cx.listener(|view: &mut Explorer, _: &ClickEvent, _, cx| view.close_form(cx));
    div()
        .flex()
        .gap(space::SM)
        .child(
            design::button("explorer-form-close", "Close", &k.ladder, &k.theme, close)
                .kind(design::Kind::Quiet),
        )
        .child(
            div().flex_1().flex().flex_col().child(
                design::button(
                    "explorer-form-submit",
                    form.verb(),
                    &k.ladder,
                    &k.theme,
                    send,
                )
                .kind(design::Kind::Primary)
                .enabled(!view.acting.busy),
            ),
        )
}
