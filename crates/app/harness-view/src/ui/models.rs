//! The models: every model record harness keeps (its id, wire, service, vendor's name, owner
//! and the context window harness learned), and the form that sets one, its `SetModel` shown
//! for the person's yes before it is sent.
use harness::Wire;
use view::design::Ladder;
use view::design::{size, space};
use view::prelude::*;
use view::{Div, FontWeight};

use super::create::pickable;
use super::{back, caption, panel, waiting, window_words};
use crate::{Agents, Screen};

pub(super) fn render(
    view: &Agents,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let back = back(
        "models-back",
        "← Agents".into(),
        Screen::List,
        cx,
        theme,
        ladder,
    );
    let list = waiting("models", &view.models, cx, theme, ladder)
        .unwrap_or_else(|| rows(view, cx, theme).into_any_element());
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
                .child(design::heading("models-title", "Models", 1, theme))
                .child(caption(
                    "The models harness asks. An agent's configuration names one by its id; the first account to set an id owns it.",
                    theme,
                )),
        )
        .child(list)
        .children(view.me.map(|_| form(view, cx, theme, ladder)))
        .into_any_element()
}

fn rows(view: &Agents, cx: &mut Context<Agents>, theme: &Theme) -> impl IntoElement + use<> {
    let models = view.models.ready().map_or(&[][..], Vec::as_slice);
    let roster = view.roster.ready();
    let rows = models.iter().map(|model| {
        let row = model.row.clone();
        let owned = view.me == Some(row.owner);
        let owner = roster.map_or_else(
            || format!("account {}", row.owner),
            |roster| roster.name(row.owner),
        );
        let now = match model.serving {
            0 => "no member node serves it now".to_owned(),
            serving => format!("{serving} serving now"),
        };
        let editing = row.clone();
        div()
            .id(SharedString::from(format!("model-{}", row.id)))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(space::LG)
            .py(space::SM)
            .border_b_1()
            .border_color(theme.border)
            .child(
                design::mono(row.id.clone())
                    .w(px(160.))
                    .font_weight(FontWeight::MEDIUM),
            )
            .child(div().w(px(160.)).child(row.wire.label()))
            .child(caption(format!("{} · {now}", row.service), theme))
            .child(design::mono(row.name.clone()))
            .child(caption(
                format!("window {}", window_words(row.window)),
                theme,
            ))
            .child(caption(format!("owner {owner}"), theme))
            .when(owned, |line| {
                line.cursor_pointer()
                    .role(Role::Button)
                    .focusable()
                    .on_click(
                        cx.listener(move |view: &mut Agents, _: &ClickEvent, _, cx| {
                            view.edit_model(&editing, cx)
                        }),
                    )
            })
    });
    div()
        .flex()
        .flex_col()
        .children(rows)
        .when(models.is_empty(), |list| {
            list.child(design::quiet(
                "Harness keeps no model yet: set the first below.",
                theme,
            ))
        })
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

fn form(
    view: &Agents,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> impl IntoElement + use<> {
    let draft = &view.model_draft;
    let wires = design::segmented(
        "model-wire",
        "Wire",
        ladder,
        theme,
        Some(draft.wire),
        Wire::ALL.iter().enumerate().map(|(index, wire)| {
            (
                ElementId::Name(format!("model-wire-{index}").into()),
                SharedString::from(wire.label()),
            )
        }),
        cx.processor(|view: &mut Agents, index: usize, _, cx| {
            view.model_draft.wire = index;
            cx.notify();
        }),
    );
    let confirm = cx.listener(|view: &mut Agents, _: &ClickEvent, _, cx| view.confirm_model(cx));
    div()
        .flex()
        .flex_col()
        .gap(space::LG)
        .child(design::heading("model-form", "Set a model", 2, theme))
        .child(labelled(
            "Model id",
            "What an agent's configuration names: lowercase letters, digits, -, _ and .",
            input(Input::new("model-id", &draft.id, "Model id"), theme).placeholder("sonnet"),
            theme,
        ))
        .child(labelled(
            "Wire",
            "The API the model speaks.",
            div().flex().child(wires),
            theme,
        ))
        .child(services(view, cx, theme, ladder))
        .child(labelled(
            "Vendor's name",
            "The model as the vendor names it: letters, digits, - . _ @ and :.",
            input(
                Input::new("model-name", &draft.name, "Vendor's name"),
                theme,
            )
            .placeholder("claude-sonnet-4-5"),
            theme,
        ))
        .children(draft.problem.clone().map(|problem| {
            div()
                .id("model-problem")
                .text_color(theme.danger)
                .child(problem)
        }))
        .child(match draft.confirming {
            true => confirming(view, cx, theme, ladder).into_any_element(),
            false => div()
                .flex()
                .child(
                    design::button("model-next", "Set model…", ladder, theme, confirm)
                        .kind(design::Kind::Primary),
                )
                .into_any_element(),
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
    let list = match waiting("model-services", &view.offers, cx, theme, ladder) {
        Some(shown) => shown,
        None => {
            let offers = view.offers.ready().map_or(&[][..], Vec::as_slice);
            let roster = view.roster.ready();
            let rows = offers.iter().map(|offer| {
                let name = offer.row.name.clone();
                let picked = view.model_draft.service.as_deref() == Some(name.as_str());
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
                            view.model_draft.service = Some(pick.clone());
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

fn confirming(
    view: &Agents,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> impl IntoElement + use<> {
    let draft = &view.model_draft;
    let set = cx.listener(|view: &mut Agents, _: &ClickEvent, _, cx| view.set_model(cx));
    let cancel = cx.listener(|view: &mut Agents, _: &ClickEvent, _, cx| {
        view.model_draft.confirming = false;
        cx.notify();
    });
    panel("model-confirm", theme.surface, theme)
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .child(format!("Set model · {}", draft.id.text().trim())),
        )
        .child(format!("wire {}", draft.wire().label()))
        .child(format!(
            "service {}",
            draft.service.clone().unwrap_or_default()
        ))
        .child(format!("vendor's name {}", draft.name.text().trim()))
        .child(
            div()
                .flex()
                .gap(space::SM)
                .child(design::button(
                    "model-confirm-back",
                    "Back",
                    ladder,
                    theme,
                    cancel,
                ))
                .child(
                    design::button("model-set", "Set", ladder, theme, set)
                        .kind(design::Kind::Primary)
                        .enabled(draft.sending.is_none()),
                ),
        )
}
