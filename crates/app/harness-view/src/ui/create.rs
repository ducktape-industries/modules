//! The create form: the ops a new agent takes, each with how far it got, the configuration
//! file it writes (where, the model from harness's records, max output, wait, thinking, the
//! system prompt) and who it answers. Editing is the same form, its fields filled from the agent's
//! file, saved as a new version of it.
use view::design::Ladder;
use view::design::{size, space};
use view::prelude::*;
use view::{Div, FontWeight, Stateful};

use super::{back, caption, thinking_picker, waiting, window_words};
use crate::create::{Answers, Draft, proposed_path};
use crate::{Agents, Screen};
use harness::Thinking;

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
            "Saving commits a new version of its configuration file. Harness reads the file at each model call, so the next call uses it, in open threads too.",
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
            "Three ops in order, and a fourth when it answers more than you. The app asks you to confirm each one.",
        ),
    };
    let sending = draft.sending.is_some();
    let next = match (draft.again, draft.created) {
        (true, _) => "Save",
        (false, None) => "Create",
        (false, Some(_)) => "Continue",
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
        .when(!draft.again, |form| {
            form.child(steps(draft, view.home(), theme))
        })
        .child(fields(view, cx, theme, ladder))
        .children(draft.problem.clone().map(|problem| {
            div()
                .id("create-problem")
                .text_color(theme.danger)
                .child(problem)
        }))
        .child(
            div().flex().child(
                design::button("create-next", next, ladder, theme, advance)
                    .kind(design::Kind::Primary)
                    .enabled(!sending),
            ),
        )
        .into_any_element()
}

fn steps(draft: &Draft, home: Option<&str>, theme: &Theme) -> impl IntoElement + use<> {
    let created = draft
        .created
        .map(|agent| format!("Account {agent} created"));
    let committed = draft.committed.then(|| draft.path(home)).flatten();
    let rows = [
        (
            "Create the account",
            "identity · CreateAgent",
            created.is_some(),
            created,
        ),
        (
            "Write its configuration, which only you may change",
            "files · Commit, write list: Owner",
            draft.committed,
            committed,
        ),
        (
            "Hand it to harness",
            "identity · SetScript → harness",
            draft.scripted,
            None,
        ),
        (
            "Who it answers, if more than you",
            "identity · SetOwnership",
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
        "Configuration file",
        "Where in files its configuration lives. The script names this path; harness reads it at each model call. This commit sets the file's write list to your account alone; with no list, any account may rewrite a file.",
        input(
            Input::new("create-path", &draft.path, "Configuration file"),
            theme,
        )
        .placeholder(proposed_path(view.home(), &draft.name.text()).unwrap_or_default()),
        theme,
    ))
    .child(models(view, cx, theme, ladder))
    .child(
        div()
            .flex()
            .flex_wrap()
            .gap(space::LG)
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
            )
            .child(
                labelled(
                    "Wait",
                    "Blocks egress waits for the model's answer. Empty is egress's default.",
                    input(Input::new("create-wait", &draft.wait, "Wait"), theme)
                        .placeholder("egress's default"),
                    theme,
                )
                .w(px(200.)),
            ),
    )
    .child(labelled(
        "Thinking",
        "How hard the model thinks before it answers. Model's default sends nothing; Off turns thinking off; Low to Max ask for that much effort.",
        div().flex().child(thinking_picker(
            "create-thinking",
            draft.thinking,
            ladder,
            theme,
            cx.processor(|view: &mut Agents, thinking: Option<Thinking>, _, cx| {
                view.draft.thinking = thinking;
                cx.notify();
            }),
        )),
        theme,
    ))
    .child(labelled(
        "System prompt",
        "Harness sends each paragraph as its own system block, in order, then its ducktape resource manual (the agent's account, its manager, the conversation).",
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
    .when(!draft.again && !draft.scripted, |form| {
        form.child(answers(draft, cx, theme, ladder))
    })
}

/// Harness's model records, one press each: the id the file names, its wire, its service and
/// how many member nodes serve it now.
fn models(
    view: &Agents,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> impl IntoElement + use<> {
    let list = match waiting("create-models", &view.models, cx, theme, ladder) {
        Some(shown) => shown,
        None => {
            let models = view.models.ready().map_or(&[][..], Vec::as_slice);
            let rows = models.iter().map(|model| {
                let id = model.row.id.clone();
                let picked = view.draft.model.as_deref() == Some(id.as_str());
                let now = match model.serving {
                    0 => "no member node serves it now".to_owned(),
                    serving => format!("{serving} serving now"),
                };
                let pick = id.clone();
                pickable(format!("model-{id}"), picked, theme)
                    .on_click(
                        cx.listener(move |view: &mut Agents, _: &ClickEvent, _, cx| {
                            view.draft.model = Some(pick.clone());
                            cx.notify();
                        }),
                    )
                    .child(design::mono(id).w(px(160.)))
                    .child(div().w(px(160.)).child(model.row.wire.label()))
                    .child(caption(
                        format!(
                            "{} · window {}",
                            model.row.service,
                            window_words(model.row.window)
                        ),
                        theme,
                    ))
                    .child(
                        div()
                            .text_color(if model.serving == 0 {
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
                .when(models.is_empty(), |list| {
                    list.child(design::quiet(
                        "Harness keeps no model yet: set one on the Models page.",
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
        .child(div().font_weight(FontWeight::MEDIUM).child("Model"))
        .child(caption(
            "A model record harness keeps: the wire it speaks and the egress service that reaches it.",
            theme,
        ))
        .child(list)
}

pub(super) fn pickable(id: String, picked: bool, theme: &Theme) -> Stateful<Div> {
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

fn answers(
    draft: &Draft,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> impl IntoElement + use<> {
    let choices = [
        (Answers::Manager, "Only me"),
        (Answers::Anyone, "Anyone"),
        (Answers::Listed, "These accounts and programs"),
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
            .when(draft.answers == Answers::Listed, |field| {
                field.child(
                    input(
                        Input::new(
                            "create-accounts",
                            &draft.listed,
                            "Account numbers and programs",
                        ),
                        theme,
                    )
                    .placeholder("7, 12, forge"),
                )
            }),
        theme,
    )
}
