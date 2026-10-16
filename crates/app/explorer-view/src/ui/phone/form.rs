//! Expo `src/network/explorer/programs.tsx` `FormSheet` (mobile
//! `explorer-detail` b2de83e): the open form as a sheet over the Programs
//! page or a program's page. Its title, the fields its edit reads, its
//! notes, why it builds nothing, Close and Schedule or Propose. Reads the
//! form (`acting.form`), the programs role, the accounts and the tip;
//! writes the fields, the program or view choice (`add_view`), the send
//! (`submit_form`) and Close (`close_form`).
use super::*;
use crate::acting::{Edit, Form};

/// The form's inset (Expo `programs.tsx:465`).
const FORM_INSET: Pixels = px(20.);

/// What a field takes, which sets the keyboard a phone shows for it (Expo's
/// `Field` props).
#[derive(Clone, Copy)]
enum Typed {
    /// an account or a height: the number pad
    Number,
    /// a name or a code id: no capitals, no corrections
    Id,
    /// params as hex: no capitals
    Hex,
}

pub(super) fn sheet(view: &Explorer, cx: Cx, k: &Kit) -> Option<AnyElement> {
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
            .child(problem.clone())
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
        None => "Type the account's number.".into(),
        Some(number) => match view.account(number) {
            Some(account) => format!("#{number} · {}", account.card.name),
            None => format!("No account {number} here."),
        },
    })
}

/// The fields its edit reads, in the Expo form's order.
fn fields(form: &Form, cx: Cx, k: &Kit) -> Vec<AnyElement> {
    let mut fields = Vec::new();
    if form.edit == Edit::Add {
        fields.push(field(
            "explorer-form-name",
            &form.name,
            "Name",
            "program id",
            Typed::Id,
            k,
        ));
        fields.push(program_or_view(form, cx, k));
    }
    if form.edit == Edit::Grant {
        fields.push(field(
            "explorer-form-account",
            &form.account,
            "Account",
            "account number",
            Typed::Number,
            k,
        ));
        fields.push(field(
            "explorer-form-name",
            &form.name,
            "Program",
            "a program, or empty for every program",
            Typed::Id,
            k,
        ));
    }
    if matches!(form.edit, Edit::Upgrade { .. } | Edit::Add) {
        fields.push(field(
            "explorer-form-code",
            &form.code,
            "Code id",
            "sha256:… of the published code",
            Typed::Id,
            k,
        ));
    }
    if form.edit == Edit::Add && !form.view {
        fields.push(field(
            "explorer-form-params",
            &form.params,
            "Params",
            "params hex, or empty",
            Typed::Hex,
            k,
        ));
    }
    if form.edit != Edit::Grant {
        fields.push(field(
            "explorer-form-height",
            &form.height,
            "At block",
            "block height",
            Typed::Number,
            k,
        ));
    }
    fields
}

/// One labelled field (Expo `Field label` + `mono`): the label over a
/// strong hairline box, the text in the data face.
fn field(
    id: &'static str,
    text: &TextField,
    label: &'static str,
    hint: &'static str,
    typed: Typed,
    k: &Kit,
) -> AnyElement {
    let input = Input::new(id, text, label)
        .w_full()
        .font_family(design::fonts::FAMILY_MONO)
        .placeholder(hint);
    let input = match typed {
        Typed::Number => input.virtual_keyboard(VirtualKeyboard::Numeric),
        Typed::Id => input
            .autocapitalize(Autocapitalize::None)
            .autocorrect(false),
        Typed::Hex => input.autocapitalize(Autocapitalize::None),
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
        .child(design::field(input, &k.ladder, &k.theme).text_type(k.ladder.secondary))
        .into_any_element()
}

/// What an Add adds: a program, or a view (Expo `Segmented`).
fn program_or_view(form: &Form, cx: Cx, k: &Kit) -> AnyElement {
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

/// Close, quiet; and Schedule or Propose, the primary, off while a write is
/// out.
fn buttons(view: &Explorer, form: &Form, cx: Cx, k: &Kit) -> Div {
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
