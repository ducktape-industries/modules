//! The create form: the ops a new agent takes, each with how far it got, the fields the
//! hire sends, the services that reach a model, and the hire shown for the person's yes.
//! Hiring again is the same form, its fields filled with what the agent was hired with.
use view::design::Ladder;
use view::design::{size, space};
use view::prelude::*;
use view::{Div, FontWeight, Stateful};

use super::{back, caption, panel, waiting};
use crate::create::{Answers, Draft};
use crate::{Agents, Screen};

pub(super) fn render(
    view: &Agents,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let draft = &view.draft;
    let name = draft.name.text();
    let (back, title, intro) = match (draft.again, draft.created) {
        (true, Some(agent)) => (
            back(
                "create-back",
                format!("← {name}").into(),
                Screen::Agent(agent),
                cx,
                theme,
                ladder,
            )
            .into_any_element(),
            format!("Edit {name}"),
            "Saving hires it again. Edits do not reach open threads: system, model, service and max_tokens are fixed when a thread opens, so the next thread opens with these.",
        ),
        _ => (
            back(
                "create-back",
                "← Agents".into(),
                Screen::List,
                cx,
                theme,
                ladder,
            )
            .into_any_element(),
            "Create an agent".to_owned(),
            "Three ops in order, and a fourth when it answers more than you. The app asks you to confirm each identity op; this form asks before the hire.",
        ),
    };
    let sending = draft.sending.is_some();
    let next = match (draft.created, draft.handed) {
        _ if draft.again => "Save…",
        (None, _) => "Create",
        _ => "Continue",
    };
    let advance = cx.listener(|view: &mut Agents, _: &ClickEvent, _, cx| view.advance(cx));
    div()
        .flex()
        .flex_col()
        .gap(space::BLOCK)
        .child(div().flex().child(back))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(space::HAIR)
                .child(design::heading("create-title", title, 1, theme))
                .child(caption(intro, theme)),
        )
        .when(!draft.again, |form| form.child(steps(draft, theme)))
        .child(fields(view, cx, theme, ladder))
        .children(draft.problem.clone().map(|problem| {
            div()
                .id("create-problem")
                .text_color(theme.danger)
                .child(problem)
        }))
        .child(match draft.confirming {
            true => confirm(view, cx, theme, ladder).into_any_element(),
            false => div()
                .flex()
                .child(
                    design::button("create-next", next, ladder, theme, advance)
                        .kind(design::Kind::Primary)
                        .enabled(!sending),
                )
                .into_any_element(),
        })
        .into_any_element()
}

fn steps(draft: &Draft, theme: &Theme) -> impl IntoElement + use<> {
    let created = draft
        .created
        .map(|agent| format!("Account {agent} created"));
    let rows = [
        (
            "Create the account",
            "identity · CreateAgent",
            created.is_some(),
            created,
        ),
        (
            "Hand it to harness",
            "identity · SetScript → harness",
            draft.handed,
            None,
        ),
        ("Hire", "harness · Hire", draft.hired, None),
        (
            "Who it answers, if more than you",
            "identity · SetAudience",
            draft.answered,
            None,
        ),
    ];
    div()
        .flex()
        .flex_col()
        .gap(space::XS)
        .children(
            rows.into_iter()
                .enumerate()
                .map(|(index, (what, op, done, said))| {
                    let mark = match done {
                        true => "✓".to_owned(),
                        false => (index + 1).to_string(),
                    };
                    div()
                        .id(SharedString::from(format!("create-step-{}", index + 1)))
                        .flex()
                        .items_center()
                        .gap(space::SM)
                        .child(
                            div()
                                .w(size::AVATAR)
                                .text_color(if done { theme.success } else { theme.muted })
                                .child(mark),
                        )
                        .child(div().font_weight(FontWeight::MEDIUM).child(what))
                        .child(caption(op, theme))
                        .children(said.map(|said| caption(said, theme)))
                }),
        )
}

fn labelled(
    label: &'static str,
    hint: &'static str,
    field: impl IntoElement,
    theme: &Theme,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(space::XXS)
        .child(div().font_weight(FontWeight::MEDIUM).child(label))
        .child(field)
        .child(caption(hint, theme))
}

fn input(field: Input, theme: &Theme) -> Input {
    field
        .h(size::CONTROL)
        .w_full()
        .px(space::SM)
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.background)
        .text_color(theme.foreground)
}

fn fields(
    view: &Agents,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> impl IntoElement + use<> {
    let draft = &view.draft;
    let named = draft.created.is_some();
    let mut form = div().flex().flex_col().gap(space::LG);
    if !named {
        form = form.child(labelled(
            "Name",
            "The account’s name: chat shows it with the agent badge.",
            input(Input::new("create-name", &draft.name, "Name"), theme).placeholder("Scout"),
            theme,
        ));
    }
    form.child(labelled(
        "System prompt",
        "Harness puts its own preamble first (who the agent is, and where people reach it), then this.",
        Textarea::new("create-system", &draft.system, "System prompt")
            .min_h(size::CONTROL * 4.)
            .w_full()
            .px(space::SM)
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.background)
            .text_color(theme.foreground)
            .placeholder("What this agent does, and how"),
        theme,
    ))
    .child(
        div()
            .flex()
            .flex_wrap()
            .gap(space::LG)
            .child(
                labelled(
                    "Model",
                    "The model id as is: letters, digits, - . _ @ and :. The network keeps no list of models.",
                    input(Input::new("create-model", &draft.model, "Model"), theme)
                        .placeholder("claude-sonnet-5-5"),
                    theme,
                )
                .flex_1()
                .min_w(px(220.)),
            )
            .child(
                labelled(
                    "Max output tokens",
                    "The most one answer writes, at least 1.",
                    input(
                        Input::new("create-max-tokens", &draft.max_tokens, "Max output tokens"),
                        theme,
                    )
                    .placeholder("4096"),
                    theme,
                )
                .w(px(200.)),
            ),
    )
    .child(services(view, cx, theme, ladder))
    .when(!draft.again && !draft.hired, |form| {
        form.child(audience(draft, cx, theme, ladder))
    })
}

/// The registered services, one press each: its owner, the nodes assigned to it, and how
/// many member nodes serve it now.
fn services(
    view: &Agents,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> impl IntoElement + use<> {
    let list = match waiting("create-services", &view.offers, cx, theme, ladder) {
        Some(shown) => shown,
        None => {
            let offers = view.offers.ready().map_or(&[][..], Vec::as_slice);
            let roster = view.roster.ready();
            let rows = offers.iter().map(|offer| {
                let name = offer.row.name.clone();
                let picked = view.draft.service.as_deref() == Some(name.as_str());
                let owner = roster.map_or_else(
                    || format!("account {}", offer.row.owner),
                    |roster| roster.name(offer.row.owner),
                );
                let now = match offer.serving {
                    0 => "no member node serves it now".to_owned(),
                    serving => format!("{serving} serving now"),
                };
                let pick = name.clone();
                pickable(format!("service-{name}"), picked, theme)
                    .on_click(
                        cx.listener(move |view: &mut Agents, _: &ClickEvent, _, cx| {
                            view.draft.service = Some(pick.clone());
                            cx.notify();
                        }),
                    )
                    .child(design::mono(name).w(px(160.)))
                    .child(div().w(px(160.)).child(format!("owner {owner}")))
                    .child(caption(
                        design::plural(
                            offer.row.nodes.len() as u64,
                            "node assigned",
                            "nodes assigned",
                        ),
                        theme,
                    ))
                    .child(
                        div()
                            .text_color(if offer.serving == 0 {
                                theme.danger
                            } else {
                                theme.success
                            })
                            .child(now),
                    )
            });
            div()
                .flex()
                .flex_col()
                .children(rows)
                .when(offers.is_empty(), |list| {
                    list.child(design::quiet(
                        "No service is registered: an account registers one in egress, and assigns the nodes that serve it.",
                        theme,
                    ))
                })
                .into_any_element()
        }
    };
    div()
        .flex()
        .flex_col()
        .gap(space::XXS)
        .child(div().font_weight(FontWeight::MEDIUM).child("Service"))
        .child(caption(
            "How the model is reached: an egress service the network registered.",
            theme,
        ))
        .child(list)
}

fn pickable(id: String, picked: bool, theme: &Theme) -> Stateful<Div> {
    let theme = *theme;
    div()
        .id(SharedString::from(id))
        .flex()
        .flex_wrap()
        .items_center()
        .gap(space::LG)
        .px(space::SM)
        .py(space::XS)
        .border_1()
        .border_color(if picked {
            theme.foreground
        } else {
            theme.border
        })
        .cursor_pointer()
        .hover(move |style| style.bg(theme.surface))
        .role(Role::Button)
        .aria_toggled(picked.into())
        .focusable()
}

fn audience(
    draft: &Draft,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> impl IntoElement + use<> {
    let choices = [
        (Answers::Manager, "Only me"),
        (Answers::Anyone, "Anyone"),
        (Answers::Accounts, "These accounts"),
    ];
    let picked = choices
        .iter()
        .position(|(answers, _)| *answers == draft.answers);
    let segmented = design::segmented(
        "create-answers",
        "Who it answers",
        ladder,
        theme,
        picked,
        choices.iter().enumerate().map(|(index, (_, label))| {
            (
                ElementId::Name(format!("create-answers-{index}").into()),
                SharedString::from(*label),
            )
        }),
        cx.processor(move |view: &mut Agents, index: usize, _, cx| {
            view.draft.answers = choices[index].0;
            cx.notify();
        }),
    );
    labelled(
        "Who it answers",
        "Its manager always. Only me sends nothing more; anything else is one more identity op.",
        div()
            .flex()
            .flex_col()
            .gap(space::XS)
            .child(div().flex().child(segmented))
            .when(draft.answers == Answers::Accounts, |field| {
                field.child(
                    input(
                        Input::new("create-accounts", &draft.accounts, "Account numbers"),
                        theme,
                    )
                    .placeholder("7, 12"),
                )
            }),
        theme,
    )
}

/// The hire as `describe(Hire)` prints it, for the person's yes: harness's op gets no card
/// from the app.
fn confirm(
    view: &Agents,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> impl IntoElement + use<> {
    let draft = &view.draft;
    let agent = draft.created.unwrap_or_default();
    let hire = cx.listener(|view: &mut Agents, _: &ClickEvent, _, cx| view.hire(cx));
    let cancel = cx.listener(|view: &mut Agents, _: &ClickEvent, _, cx| {
        view.draft.confirming = false;
        cx.notify();
    });
    panel("create-confirm", theme.surface, theme)
        .child(div().font_weight(FontWeight::MEDIUM).child("Hire an agent"))
        .child(format!("agent {} (account {agent})", draft.name.text()))
        .child(format!("model {}", draft.model.text().trim()))
        .child(format!(
            "service {}",
            draft.service.clone().unwrap_or_default()
        ))
        .child(
            div()
                .flex()
                .gap(space::SM)
                .child(design::button(
                    "create-confirm-back",
                    "Back",
                    ladder,
                    theme,
                    cancel,
                ))
                .child(
                    design::button("create-hire", "Hire", ladder, theme, hire)
                        .kind(design::Kind::Primary)
                        .enabled(draft.sending.is_none()),
                ),
        )
}
