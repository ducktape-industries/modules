//! Expo `src/account/Agents.tsx`: the Agents pane and its sheets. The pane
//! lists every agent this person manages, then the rows that open the two
//! add sheets (create an agent, add a key to one). An agent's row opens its
//! one sheet, in three modes: its actions, its rename, whom it answers.
//!
//! Reads the account's agents (`view.account`), the open sheet
//! (`view.sheet`) and each form: create, add key, rename, standing, whom it
//! answers. Writes the open sheet (`open_sheet`, `close_sheet`,
//! `open_answers`, `start_rename_agent`, `cancel_rename_agent`) and each
//! op through its submit (`submit_create_agent`, `submit_agent_key`,
//! `submit_rename_agent`, `set_standing`, `pick_audience`,
//! `submit_audience_person`, `remove_audience_person`).
use super::parts::{TALL, labelled, mono, muted_icon, note, problem_of, quiet, words};
use super::*;
use crate::queries::Agent;
use crate::state::Choice;
use identity::{Audience, Standing};
use view::Svg;

/// A sheet's inset (Expo `src/account/Agents.tsx:24`).
const PAD: Pixels = px(20.);
/// A revoked agent's face, faded (Expo `src/kit/Avatar.tsx:38`).
const OFF: f32 = 0.5;

/// The Agents pane: every agent this person manages, then the rows that
/// open the create and add-key sheets.
pub(super) fn pane(
    view: &Settings,
    cx: &mut Context<Settings>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let Some(Some(Seat::Account(account))) = view.account.ready() else {
        // the strip lists Agents only for an account that manages them
        return div().into_any_element();
    };
    let rows: Vec<_> = account
        .agents
        .iter()
        .map(|agent| agent_row(agent, cx, theme, ladder))
        .collect();
    column("settings/agents")
        .gap_0()
        .child(quiet(
            "settings/agents/help",
            "Agents act as accounts you manage. You answer for what they do.",
            theme,
            ladder,
        ))
        .child(design::section_head(
            "settings/agents/list",
            "Agents",
            Some(account.agents.len()),
            ladder,
            theme,
        ))
        .children(rows)
        .child(design::section_head(
            "settings/agents/add",
            "Add",
            None,
            ladder,
            theme,
        ))
        .child(add_row(
            Sheet::CreateAgent,
            "plus",
            "Create an agent",
            "Name it; it holds no key yet.",
            cx,
            theme,
            ladder,
        ))
        .child(add_row(
            Sheet::AddKey,
            "key-round",
            "Add a key to an agent",
            "Paste the key request the agent's device shows.",
            cx,
            theme,
            ladder,
        ))
        .into_any_element()
}

/// A row that opens one of the two add sheets.
fn add_row(
    sheet: Sheet,
    glyph: &str,
    title: &str,
    about: &str,
    cx: &mut Context<Settings>,
    theme: &Theme,
    ladder: &Ladder,
) -> Stateful<Div> {
    let id = match sheet {
        Sheet::AddKey => "settings/agents/key/open",
        _ => "settings/agents/create/open",
    };
    design::row(id, ladder, theme)
        .min_h(TALL)
        .role(Role::Button)
        .focusable()
        .on_click(
            cx.listener(move |v: &mut Settings, _: &ClickEvent, _, cx| v.open_sheet(sheet, cx)),
        )
        .child(design::lead(muted_icon(glyph, ladder, theme)))
        .child(words(
            title,
            theme.foreground,
            caption(about, theme, ladder),
            ladder,
        ))
        .child(chevron(theme, ladder))
}

/// One agent's row; a press opens its sheet, a revoked one's too.
fn agent_row(
    agent: &Agent,
    cx: &mut Context<Settings>,
    theme: &Theme,
    ladder: &Ladder,
) -> Stateful<Div> {
    let number = agent.number;
    let revoked = agent.standing() == Standing::Revoked;
    let keys = design::plural(agent.keys as u64, "key", "keys");
    let about = match revoked {
        true => format!("account {number} · {keys}"),
        false => format!(
            "account {number} · {keys} · answers {}",
            answers(&agent.audience)
        ),
    };
    let open = cx.listener(move |v: &mut Settings, _: &ClickEvent, _, cx| {
        v.open_sheet(Sheet::Actions(number), cx)
    });
    design::row(format!("settings/agents/{number}"), ladder, theme)
        .min_h(TALL)
        .role(Role::Button)
        .focusable()
        .on_click(open)
        .child(design::lead(agent_avatar(agent, ladder.avatar, theme)))
        .child(words(
            &agent.name,
            if revoked {
                theme.faint
            } else {
                theme.foreground
            },
            caption(about, theme, ladder),
            ladder,
        ))
        .child(standing_badge(agent, ladder, theme))
        .child(chevron(theme, ladder))
}

/// Whom an agent answers, as its row says it (Expo
/// `src/account/words.ts:68`).
fn answers(audience: &Audience) -> String {
    match audience {
        Audience::Manager => "only me".into(),
        Audience::Anyone => "anyone".into(),
        Audience::Accounts(people) => design::plural(people.len() as u64, "person", "people"),
    }
}

fn standing_word(standing: Standing) -> &'static str {
    match standing {
        Standing::Active => "active",
        Standing::Suspended => "suspended",
        Standing::Revoked => "revoked",
    }
}

fn standing_badge(agent: &Agent, ladder: &Ladder, theme: &Theme) -> Stateful<Div> {
    let (foreground, background) = match agent.standing() {
        Standing::Active => (theme.success, theme.success_soft),
        Standing::Suspended => (theme.warning, theme.warning_soft),
        Standing::Revoked => (theme.danger, theme.danger_soft),
    };
    design::badge(
        format!("settings/agents/{}/standing", agent.number),
        standing_word(agent.standing()),
        ladder,
        foreground,
        background,
    )
}

/// An agent's initial in the agent tint, faded once revoked.
fn agent_avatar(agent: &Agent, face: design::Face, theme: &Theme) -> Div {
    design::avatar(&agent.name, face, theme)
        .bg(theme.agent_soft)
        .text_color(theme.agent)
        .when(agent.standing() == Standing::Revoked, |face| {
            face.opacity(OFF)
        })
}

/// The sheet open over the screen, if one is; dismissing it closes it.
pub(super) fn sheet(
    view: &Settings,
    cx: &mut Context<Settings>,
    theme: &Theme,
    ladder: &Ladder,
) -> Option<AnyElement> {
    let open = view.sheet?;
    let close = cx.listener(|v: &mut Settings, _: &(), _, cx| v.close_sheet(cx));
    let (id, label, body) = match open {
        Sheet::Actions(number) | Sheet::Rename(number) | Sheet::Answers(number) => {
            let agent = view.agent(number)?;
            (
                format!("settings/agents/{number}/actions"),
                agent.name.clone(),
                agent_sheet(view, agent, open, cx, theme, ladder),
            )
        }
        Sheet::CreateAgent => (
            "settings/agents/create/sheet".to_owned(),
            "Create an agent".to_owned(),
            create_agent(view, cx, theme, ladder),
        ),
        Sheet::AddKey => (
            "settings/agents/key/sheet".to_owned(),
            "Add a key to an agent".to_owned(),
            add_key(view, cx, theme, ladder),
        ),
    };
    Some(
        design::sheet(id, label, ladder, theme)
            .on_dismiss(close)
            .child(body)
            .into_any_element(),
    )
}

/// One agent's sheet: who it is, then its actions, its rename or whom it
/// answers. A revoked agent's sheet only says who it was.
fn agent_sheet(
    view: &Settings,
    agent: &Agent,
    open: Sheet,
    cx: &mut Context<Settings>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let body = match (agent.standing(), open) {
        (Standing::Revoked, _) => None,
        (_, Sheet::Rename(_)) => rename(view, agent, cx, theme, ladder),
        (_, Sheet::Answers(_)) => Some(answering(view, agent, cx, theme, ladder)),
        _ => Some(actions(view, agent, cx, theme, ladder)),
    };
    div()
        .flex()
        .flex_col()
        .child(agent_head(agent, theme, ladder))
        .children(body)
        .into_any_element()
}

/// An agent's sheet head: its face, its name over its number and standing.
fn agent_head(agent: &Agent, theme: &Theme, ladder: &Ladder) -> Stateful<Div> {
    let number = agent.number;
    design::sheet_head(format!("settings/agents/{number}/head"), theme).child(
        div()
            .flex()
            .items_center()
            .gap(design::space::LG)
            .child(agent_avatar(agent, ladder.avatar_sheet, theme))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .truncate()
                            .text_type(ladder.body)
                            .font_weight(FontWeight::MEDIUM)
                            .child(agent.name.clone()),
                    )
                    .child(mono(
                        format!("account {number} · {}", standing_word(agent.standing())),
                        theme,
                        ladder,
                    )),
            ),
    )
}

/// An agent's actions: Rename, whom it answers, Suspend or Resume, Revoke.
/// Suspend and Revoke are one press here: the host confirms them before
/// the key signs.
fn actions(
    view: &Settings,
    agent: &Agent,
    cx: &mut Context<Settings>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let number = agent.number;
    let listing = view
        .answering
        .get(&number)
        .is_some_and(|answering| answering.listing);
    let rename = cx
        .listener(move |v: &mut Settings, _: &ClickEvent, _, cx| v.start_rename_agent(number, cx));
    let answers =
        cx.listener(move |v: &mut Settings, _: &ClickEvent, _, cx| v.open_answers(number, cx));
    let revoke = cx.listener(move |v: &mut Settings, _: &ClickEvent, _, cx| {
        v.set_standing(identity::Op::Revoke { account: number }, cx)
    });
    column(format!("settings/agents/{number}/sheet"))
        .gap_0()
        .child(design::sheet_row(
            format!("settings/agents/{number}/rename/open"),
            Some("pencil"),
            "Rename",
            false,
            ladder,
            theme,
            rename,
        ))
        .child(
            design::sheet_row(
                format!("settings/agents/{number}/answers/open"),
                Some("users"),
                "Who it answers",
                false,
                ladder,
                theme,
                answers,
            )
            .child(hint(
                Choice::shown(&agent.audience, listing).label(),
                theme,
                ladder,
            )),
        )
        .child(standing_row(view, agent, cx, theme, ladder))
        .child(
            design::sheet_row(
                format!("settings/agents/{number}/revoke"),
                Some("x"),
                "Revoke",
                true,
                ladder,
                theme,
                revoke,
            )
            .child(hint("final", theme, ladder)),
        )
        .children(view.agent_standing.problem.as_ref().map(|problem| {
            div()
                .px(PAD)
                .child(note(problem_text(problem, ""), theme.danger, ladder))
        }))
        .into_any_element()
}

/// Suspend an active agent, or Resume a suspended one; `Working…` while
/// either is in flight.
fn standing_row(
    view: &Settings,
    agent: &Agent,
    cx: &mut Context<Settings>,
    theme: &Theme,
    ladder: &Ladder,
) -> Stateful<Div> {
    let account = agent.number;
    let (slug, glyph, word, op) = match agent.standing() {
        Standing::Suspended => ("resume", "play", "Resume", identity::Op::Resume { account }),
        Standing::Active | Standing::Revoked => (
            "suspend",
            "clock",
            "Suspend",
            identity::Op::Suspend { account },
        ),
    };
    let press =
        cx.listener(move |v: &mut Settings, _: &ClickEvent, _, cx| v.set_standing(op.clone(), cx));
    design::sheet_row(
        format!("settings/agents/{account}/{slug}"),
        Some(glyph),
        if view.agent_standing.busy {
            "Working…"
        } else {
            word
        },
        false,
        ladder,
        theme,
        press,
    )
}

/// An agent's new name: the field holding it, Cancel back to its actions,
/// Save. None once its form is gone (the rename landed).
fn rename(
    view: &Settings,
    agent: &Agent,
    cx: &mut Context<Settings>,
    theme: &Theme,
    ladder: &Ladder,
) -> Option<AnyElement> {
    let number = agent.number;
    let form = view.rename_agent.get(&number)?;
    let label = format!("Rename {}", agent.name);
    let input = Input::new(
        format!("settings/agents/{number}/name"),
        &form.text,
        label.clone(),
    )
    .placeholder("New name")
    .disabled(form.busy)
    .autocapitalize(Autocapitalize::Words)
    .when(!form.busy, |input| {
        input.on_submit(
            cx.listener(move |v: &mut Settings, _: &(), _, cx| v.submit_rename_agent(number, cx)),
        )
    });
    let save = cx
        .listener(move |v: &mut Settings, _: &ClickEvent, _, cx| v.submit_rename_agent(number, cx));
    let cancel = cx
        .listener(move |v: &mut Settings, _: &ClickEvent, _, cx| v.cancel_rename_agent(number, cx));
    let form = column(format!("settings/agents/{number}/rename/form"))
        .gap(design::space::LG)
        .p(PAD)
        .child(labelled(
            &label,
            design::field(input, ladder, theme),
            problem_of(form, "Enter the agent's new name."),
            theme,
            ladder,
        ))
        .child(verbs(
            &format!("settings/agents/{number}/rename"),
            "Save",
            form.busy.then_some("Renaming…"),
            save,
            cancel,
            theme,
            ladder,
        ));
    Some(form.into_any_element())
}

/// Whom an agent answers: the choice, and under Specific the people it
/// lists with the field that adds one; Done goes back to its actions.
fn answering(
    view: &Settings,
    agent: &Agent,
    cx: &mut Context<Settings>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let number = agent.number;
    let answering = view.answering.get(&number);
    let listing = answering.is_some_and(|answering| answering.listing);
    let busy = answering.is_some_and(|answering| answering.form.busy);
    let shown = Choice::shown(&agent.audience, listing);
    let problem = answering
        .and_then(|answering| problem_of(&answering.form, "Enter a name or an account number."));
    let people = match (shown, answering) {
        (Choice::Specific, Some(answering)) => Some(people(
            agent,
            &answering.form,
            problem.clone(),
            cx,
            theme,
            ladder,
        )),
        _ => None,
    };
    let done = cx.listener(move |v: &mut Settings, _: &ClickEvent, _, cx| {
        v.open_sheet(Sheet::Actions(number), cx)
    });
    let saving = if busy { " · Saving…" } else { "" };
    column(format!("settings/agents/{number}/answering"))
        .gap(design::space::LG)
        .p(PAD)
        .pb(design::space::BLOCK)
        .child(note(format!("Who it answers{saving}"), theme.muted, ladder))
        .child(audience_choices(agent, shown, cx, theme, ladder))
        .when(agent.audience == Audience::Anyone, |block| {
            block.child(note(
                "Anyone on the network can tell this agent what to do.",
                theme.warning,
                ladder,
            ))
        })
        // under Specific the field says what stopped it
        .when(people.is_none(), |block| {
            block.children(problem.map(|problem| note(problem, theme.danger, ladder)))
        })
        .children(people)
        .child(
            div().flex().child(
                design::button(
                    format!("settings/agents/{number}/answers/done"),
                    "Done",
                    ladder,
                    theme,
                    done,
                )
                .kind(ButtonKind::Quiet),
            ),
        )
        .into_any_element()
}

/// The three answers an agent can take, `shown` picked: one press sends
/// Only me or Anyone; Specific opens its people.
fn audience_choices(
    agent: &Agent,
    shown: Choice,
    cx: &mut Context<Settings>,
    theme: &Theme,
    ladder: &Ladder,
) -> Stateful<Div> {
    let number = agent.number;
    design::segmented(
        format!("settings/agents/{number}/answers"),
        format!("Who {} answers", agent.name),
        ladder,
        theme,
        Choice::ALL.iter().position(|choice| *choice == shown),
        Choice::ALL.map(|choice| {
            (
                format!("settings/agents/{number}/answers/{}", choice.slug()).into(),
                choice.label().into(),
            )
        }),
        cx.processor(move |v: &mut Settings, index: usize, _, cx| {
            v.pick_audience(number, Choice::ALL[index], cx)
        }),
    )
}

/// The people an agent answers under Specific: each with Remove, then the
/// field that adds one by name or number, and Add.
fn people(
    agent: &Agent,
    form: &Form,
    problem: Option<String>,
    cx: &mut Context<Settings>,
    theme: &Theme,
    ladder: &Ladder,
) -> Stateful<Div> {
    let number = agent.number;
    let listed: &[u64] = match &agent.audience {
        Audience::Accounts(numbers) => numbers,
        Audience::Manager | Audience::Anyone => &[],
    };
    let label = format!("Add someone {} answers", agent.name);
    // a name or a number: nothing to correct, nothing to capitalise
    let input = Input::new(
        format!("settings/agents/{number}/answers/name"),
        &form.text,
        label.clone(),
    )
    .placeholder("Name or account number")
    .disabled(form.busy)
    .autocorrect(false)
    .autocapitalize(Autocapitalize::None);
    let add = cx.listener(move |v: &mut Settings, _: &ClickEvent, _, cx| {
        v.submit_audience_person(number, cx)
    });
    column(format!("settings/agents/{number}/answers/people"))
        .gap(design::space::SM)
        .children(
            listed
                .iter()
                .map(|&person| person_row(agent, person, form, cx, theme, ladder)),
        )
        .when(listed.is_empty(), |people| {
            people.child(note(
                "Add people by name or account number.",
                theme.muted,
                ladder,
            ))
        })
        .child(labelled(
            &label,
            design::field(input, ladder, theme),
            problem,
            theme,
            ladder,
        ))
        .child(
            div().flex().child(
                design::button(
                    format!("settings/agents/{number}/answers/add"),
                    if form.busy { "Adding…" } else { "Add" },
                    ladder,
                    theme,
                    add,
                )
                .kind(ButtonKind::Outline)
                .enabled(!form.busy),
            ),
        )
}

/// One person an agent answers: their name (or `#number`, unnamed) over
/// their number, and Remove.
fn person_row(
    agent: &Agent,
    person: u64,
    form: &Form,
    cx: &mut Context<Settings>,
    theme: &Theme,
    ladder: &Ladder,
) -> Stateful<Div> {
    let number = agent.number;
    let name = match agent.names.get(&person) {
        Some(name) => name.clone(),
        None => format!("#{person}"),
    };
    let remove = cx.listener(move |v: &mut Settings, _: &ClickEvent, _, cx| {
        v.remove_audience_person(number, person, cx)
    });
    div()
        .id(format!("settings/agents/{number}/answers/{person}"))
        .flex()
        .items_center()
        .gap(design::space::SM)
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .child(div().text_type(ladder.body).child(name))
                .child(mono(format!("account {person}"), theme, ladder)),
        )
        .child(
            design::button(
                format!("settings/agents/{number}/answers/{person}/remove"),
                "Remove",
                ladder,
                theme,
                remove,
            )
            .kind(ButtonKind::Quiet)
            .enabled(!form.busy),
        )
}

/// The Create an agent sheet: its name, Cancel, Create agent.
fn create_agent(
    view: &Settings,
    cx: &mut Context<Settings>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let form = &view.create_agent;
    let input = Input::new("settings/agents/create/name", &form.text, "Agent name")
        .placeholder("Name the new agent")
        .disabled(form.busy)
        .autocapitalize(Autocapitalize::Words)
        .when(!form.busy, |input| {
            input
                .on_submit(cx.listener(|v: &mut Settings, _: &(), _, cx| v.submit_create_agent(cx)))
        });
    let create = cx.listener(|v: &mut Settings, _: &ClickEvent, _, cx| v.submit_create_agent(cx));
    let cancel = cx.listener(|v: &mut Settings, _: &ClickEvent, _, cx| v.close_sheet(cx));
    add_sheet(
        "settings/agents/create",
        "Create an agent",
        labelled(
            "Agent name",
            design::field(input, ladder, theme),
            problem_of(form, "Enter an agent name."),
            theme,
            ladder,
        ),
        verbs(
            "settings/agents/create/submit",
            "Create agent",
            form.busy.then_some("Creating…"),
            create,
            cancel,
            theme,
            ladder,
        ),
        theme,
        ladder,
    )
}

/// The Add a key sheet: the pasted key request, Cancel, Add key. The
/// request is many lines in the data face, and Return is a new line, so
/// it is a `Textarea` styled as the SDK's `design::field` styles an
/// `Input` (Expo `src/kit/Field.tsx:79`, `multiline`).
fn add_key(
    view: &Settings,
    cx: &mut Context<Settings>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let form = &view.agent_key;
    // hex: no correction, no capitals, no suggestions
    let request = Textarea::new(
        "settings/agents/key/request",
        &form.text,
        "Agent key request",
    )
    .placeholder("Paste an agent's key request")
    .read_only(form.busy)
    .autocorrect(false)
    .autocapitalize(Autocapitalize::None)
    .suggestions(false)
    .min_h(ladder.control)
    .px(design::space::LG)
    .py(design::space::MD)
    .border_1()
    .border_color(theme.border_strong)
    .bg(theme.background)
    .text_color(theme.foreground)
    .font_family(design::fonts::FAMILY_MONO)
    .text_type(ladder.secondary);
    let add = cx.listener(|v: &mut Settings, _: &ClickEvent, _, cx| v.submit_agent_key(cx));
    let cancel = cx.listener(|v: &mut Settings, _: &ClickEvent, _, cx| v.close_sheet(cx));
    add_sheet(
        "settings/agents/key",
        "Add a key to an agent",
        labelled(
            "Agent key request",
            request,
            problem_of(form, ""),
            theme,
            ladder,
        ),
        verbs(
            "settings/agents/key/submit",
            "Add key",
            form.busy.then_some("Adding…"),
            add,
            cancel,
            theme,
            ladder,
        ),
        theme,
        ladder,
    )
}

/// An add sheet: its title in the sheet's head, then its field and verbs.
fn add_sheet(
    id: &str,
    title: &str,
    field: Div,
    verbs: Div,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .child(
            design::sheet_head(format!("{id}/head"), theme).child(
                div()
                    .text_type(ladder.title)
                    .font_weight(FontWeight::MEDIUM)
                    .child(title.to_owned()),
            ),
        )
        .child(
            column(format!("{id}/form"))
                .gap(design::space::LG)
                .p(PAD)
                .child(field)
                .child(verbs),
        )
        .into_any_element()
}

/// A sheet form's buttons: Cancel, then its verb taking the rest; the verb
/// says `busy` and takes no press while its submit is in flight.
fn verbs(
    id: &str,
    verb: &str,
    busy: Option<&str>,
    pressed: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cancel: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    theme: &Theme,
    ladder: &Ladder,
) -> Div {
    div()
        .flex()
        .gap(design::space::SM)
        .child(
            design::button(format!("{id}-cancel"), "Cancel", ladder, theme, cancel)
                .kind(ButtonKind::Quiet),
        )
        .child(
            div().flex_1().flex().flex_col().child(
                design::button(id.to_owned(), busy.unwrap_or(verb), ladder, theme, pressed)
                    .kind(ButtonKind::Primary)
                    .enabled(busy.is_none()),
            ),
        )
}

/// The muted line under a row's title.
fn caption(text: impl Into<SharedString>, theme: &Theme, ladder: &Ladder) -> Div {
    div()
        .truncate()
        .text_type(ladder.caption)
        .text_color(theme.muted)
        .child(text.into())
}

/// A sheet row's value at its right, never squeezed (the Expo kit's
/// `SheetRow` hint, `src/kit/Sheet.tsx:168`).
fn hint(text: &str, theme: &Theme, ladder: &Ladder) -> Div {
    div()
        .flex_none()
        .text_type(ladder.caption)
        .text_color(theme.muted)
        .child(text.to_owned())
}

/// A row that opens something (the Expo kit's `Row` `chevron`).
fn chevron(theme: &Theme, ladder: &Ladder) -> Svg {
    design::icon("chevron-right", ladder.icon_row).text_color(theme.faint)
}
