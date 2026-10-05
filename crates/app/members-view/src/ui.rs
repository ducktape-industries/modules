//! The screen: the list on the left, the chosen account on the right.
//! Colour is kept for what it says: the agent tint on an agent's avatar, and
//! the badges of a standing. Everything else is the window, grey captions
//! and hairlines.
use ducktape_view_guest::design::{size, space, text};
use ducktape_view_guest::prelude::*;
use ducktape_view_guest::{Div, FontWeight, Stateful};

use crate::activity::WINDOW;
use crate::{Group, Members, Row};

/// The list pane's width until it is dragged.
const LIST: f32 = 400.;
/// The list's width bounds when docked; the detail keeps its [`DETAIL_MIN`].
const LIST_W: (f32, f32) = (320., 560.);
/// The detail's narrowest beside the list: the head and a device row with
/// its [`KEY_COLUMN`]. Narrower, it covers the whole screen.
const DETAIL_MIN: Pixels = px(440.);
/// A detail row's first column: a device's label, an agent's name. It
/// gives way first when the pane is narrow.
const KEY_COLUMN: Pixels = px(180.);

pub(crate) fn render(view: &Members, cx: &mut Context<Members>) -> impl IntoElement {
    let theme = *cx.global::<Theme>();
    // too narrow for the detail beside the list: the list takes the pane
    // and the chosen account floats over it
    let list = list_width(view);
    let docked = design::docks(view.width, list, DETAIL_MIN.into());
    let screen = div()
        .id("members")
        .relative()
        .flex()
        .size_full()
        .bg(theme.background)
        .text_color(theme.foreground)
        .text_size(text::BODY)
        .child(list_pane(view, docked.then_some(list), cx, &theme));
    match (docked, view.selected_row().is_some()) {
        (true, _) => screen
            .child(design::divider(
                "members-list-resize",
                "Resize the member list",
                &theme,
                cx,
                |view, dx| {
                    view.list = Some(list_width(view) + dx);
                    view.list = Some(list_width(view));
                },
            ))
            .child(detail_pane(view, false, cx, &theme).flex_1()),
        (false, true) => screen.child(design::over(
            "members-detail-over",
            detail_pane(view, true, cx, &theme),
            &theme,
        )),
        (false, false) => screen,
    }
}

/// The list's width: dragged or [`LIST`], within [`LIST_W`] and never
/// taking the detail's [`DETAIL_MIN`].
fn list_width(view: &Members) -> f32 {
    let (lo, hi) = LIST_W;
    let room = view.width - f32::from(DETAIL_MIN);
    view.list.unwrap_or(LIST).clamp(lo, room.clamp(lo, hi))
}

/// The chosen account, scrolling on its own; `over` the list it carries
/// its close.
fn detail_pane(
    view: &Members,
    over: bool,
    cx: &mut Context<Members>,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id("members-detail")
        .min_w(px(0.))
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .child(detail(view, over, cx, theme))
}

fn list_pane(
    view: &Members,
    width: Option<f32>,
    cx: &mut Context<Members>,
    theme: &Theme,
) -> impl IntoElement {
    let count = match view.rows.ready() {
        Some(rows) => design::plural(rows.len() as u64, "account", "accounts"),
        None => String::new(),
    };
    div()
        .id("members-list-pane")
        // docked at its `width`, else the list alone takes the screen
        .map(|pane| match width {
            Some(list) => pane.w(px(list)).flex_shrink_0(),
            None => pane.flex_1().min_w(px(0.)),
        })
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .items_center()
                .gap(space::SM)
                .px(space::BLOCK)
                .pt(px(14.))
                .pb(space::MD)
                .child(design::heading("members-title", "Members", 1, theme).flex_1())
                .child(
                    div()
                        .text_size(text::CAPTION)
                        .text_color(theme.muted)
                        .child(count),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(space::SM)
                .px(space::BLOCK)
                .pb(space::MD)
                .child(
                    Input::new("members-filter", &view.filter, "Filter members")
                        .h(size::CONTROL)
                        .w_full()
                        .px_2()
                        .py_1()
                        .border_1()
                        .border_color(theme.border_strong)
                        .bg(theme.surface)
                        .text_color(theme.foreground)
                        .placeholder("Filter by name or number"),
                )
                .child(chips(view, cx, theme)),
        )
        .child(rows(view, cx, theme))
}

/// All, People, Agents, Modules, each with how many the network holds.
fn chips(view: &Members, cx: &mut Context<Members>, theme: &Theme) -> impl IntoElement {
    let rows = view.rows.ready().map_or(&[][..], Vec::as_slice);
    let count = |group: Option<Group>| {
        rows.iter()
            .filter(|row| group.is_none_or(|group| row.group() == group))
            .count()
    };
    let choices = [
        (None, "All"),
        (Some(Group::People), "People"),
        (Some(Group::Agents), "Agents"),
        (Some(Group::Modules), "Modules"),
    ];
    div()
        .flex()
        .gap(space::XS)
        .children(
            choices
                .into_iter()
                .enumerate()
                .map(|(index, (group, label))| {
                    let on = view.only == group;
                    div()
                        .id(format!("members-chip-{}", index))
                        .flex()
                        .gap(space::XXS)
                        .px(space::SM)
                        .py(space::HAIR)
                        .border_1()
                        .border_color(if on { theme.foreground } else { theme.border })
                        .text_size(text::CAPTION)
                        .text_color(if on { theme.foreground } else { theme.muted })
                        .cursor_pointer()
                        .role(Role::Button)
                        .aria_toggled(on.into())
                        .focusable()
                        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                            view.only = group;
                            cx.notify();
                        }))
                        .child(label)
                        .child(
                            div()
                                .text_color(theme.faint)
                                .child(count(group).to_string()),
                        )
                }),
        )
}

/// The four states of the roster: loading, refused, empty, ready.
fn rows(view: &Members, cx: &mut Context<Members>, theme: &Theme) -> AnyElement {
    let all = match &view.rows {
        Loadable::Idle | Loadable::Loading(_) => {
            return pad(design::quiet("Reading the roster…", theme))
                .id("members-loading")
                .into_any_element();
        }
        Loadable::Failed(refusal) => {
            let retry = cx.listener(|view, _: &ClickEvent, _, cx| view.read(cx));
            return pad(design::refused(
                "members",
                refusal.message.clone(),
                theme,
                retry,
            ))
            .into_any_element();
        }
        Loadable::Ready(rows) | Loadable::Reloading(rows, _) => rows,
    };
    if all.is_empty() {
        return design::empty_state(
            "members-empty",
            "No accounts",
            "The identity program of this network holds no accounts yet.",
            theme,
        )
        .into_any_element();
    }
    let shown = view.shown();
    if shown.is_empty() {
        // the chip alone, or the filter (with or without a chip)
        let detail = match (view.filter.text().trim(), view.only) {
            ("", Some(group)) => {
                format!("No {} on this network yet.", group.label().to_lowercase())
            }
            (needle, _) => format!("No account reads like “{needle}”."),
        };
        return design::empty_state("members-no-match", "Nothing matches", detail, theme)
            .into_any_element();
    }
    // one Tab stop; the arrows select as they move, and the active row is
    // the selected one, else the first shown, which Enter selects
    let numbers: Vec<u64> = shown.iter().map(|row| row.number).collect();
    let active = view
        .selected
        .and_then(|number| numbers.iter().position(|shown| *shown == number))
        .unwrap_or(0);
    let active_number = numbers[active];
    let pressed = numbers.clone();
    let mut list = design::composite("members-list", Role::ListBox, "Members")
        .active(active, numbers.len())
        .on_move(cx.processor(move |view, index: usize, _, cx| view.select(numbers[index], cx)))
        .on_press(cx.processor(move |view, index: usize, _, cx| view.select(pressed[index], cx)))
        .build()
        .flex_1()
        .min_h(px(0.))
        .overflow_y_scroll()
        .flex()
        .flex_col();
    for (index, group) in Group::ALL.into_iter().enumerate() {
        let members: Vec<&Row> = shown
            .iter()
            .copied()
            .filter(|row| row.group() == group)
            .collect();
        if members.is_empty() {
            continue;
        }
        list = list.child(
            div()
                .flex()
                .justify_between()
                .px(space::BLOCK)
                .pt(space::MD)
                .pb(space::XXS)
                .text_size(text::CAPTION)
                .text_color(theme.muted)
                .when(index > 0, |head| {
                    head.border_t_1().border_color(theme.border)
                })
                .child(group.label())
                .child(design::mono(members.len().to_string()).text_size(text::CAPTION)),
        );
        for row in members {
            list = list.child(member_row(
                view,
                row,
                row.number == active_number,
                all,
                cx,
                theme,
            ));
        }
    }
    list.into_any_element()
}

fn member_row(
    view: &Members,
    row: &Row,
    active: bool,
    all: &[Row],
    cx: &mut Context<Members>,
    theme: &Theme,
) -> impl IntoElement {
    let theme = *theme;
    let number = row.number;
    let selected = view.selected == Some(number);
    let dim = row.kind.note().is_some();
    let name_of = |manager| name(all, manager);
    let kind = match &row.kind {
        // a module named for its program says so once
        identity::Kind::Module(program) if *program == row.name => "Module".into(),
        kind => identity::view::kind(kind, name_of),
    };
    let me = view.me == Some(number);
    let row = div()
        .id(format!("members-row-{}", number))
        .flex()
        .items_center()
        .gap(space::MD)
        .h(size::CONTROL)
        .flex_shrink_0()
        .px(space::BLOCK)
        .cursor_pointer()
        .when(selected, |row| row.bg(theme.surface_raised))
        .when(!selected, |row| {
            row.hover(move |style| style.bg(theme.surface))
        })
        // the name, not the avatar's initial drawn before it
        .aria_label(row.name.clone())
        .aria_description(if me {
            format!("you · {kind}")
        } else {
            kind.clone()
        })
        .aria_selected(selected)
        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| view.select(number, cx)))
        .child(avatar(row, size::AVATAR, dim, &theme))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .items_center()
                .gap(space::XS)
                .child(
                    div()
                        .truncate()
                        .when(dim, |name| name.text_color(theme.faint))
                        .child(row.name.clone()),
                )
                .when(me, |line| {
                    line.child(
                        div()
                            .text_size(text::CAPTION)
                            .text_color(theme.faint)
                            .child("you"),
                    )
                }),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_size(text::CAPTION)
                .text_color(theme.muted)
                .child(kind),
        );
    // the list holds focus and the arrows; the active row is the one
    // assistive technology is told is active
    design::item(row, Role::ListBoxOption, active)
}

/// An account's initial: round for a person, tinted for an agent, square
/// and monospaced for a module; faint when it no longer acts.
fn avatar(row: &Row, size: Pixels, dim: bool, theme: &Theme) -> Div {
    let avatar = design::avatar(&row.name, size, theme);
    match row.group() {
        _ if dim => avatar.bg(theme.background).text_color(theme.faint),
        Group::People => avatar,
        Group::Agents => avatar.bg(theme.agent_soft).text_color(theme.agent),
        Group::Modules => avatar
            .rounded_none()
            .font_family(design::fonts::FAMILY_MONO),
    }
}

fn name(rows: &[Row], number: u64) -> Option<String> {
    rows.iter()
        .find(|row| row.number == number)
        .map(|row| row.name.clone())
}

fn pad(inner: impl IntoElement) -> Div {
    div().px(space::BLOCK).py(space::SM).child(inner)
}

fn detail(view: &Members, over: bool, cx: &mut Context<Members>, theme: &Theme) -> AnyElement {
    let (Some(rows), Some(row)) = (view.rows.ready(), view.selected_row()) else {
        return div()
            .id("members-none")
            .px(px(20.))
            .py(space::BLOCK)
            .child(design::quiet("Choose a member", theme))
            .into_any_element();
    };
    let mut detail = div()
        .flex()
        .flex_col()
        .child(head(view, row, rows, over, cx, theme))
        .child(devices(row, theme));
    let managed: Vec<&Row> = rows
        .iter()
        .filter(|other| other.manager() == Some(row.number))
        .collect();
    if !managed.is_empty() {
        detail = detail.child(manages(&managed, cx, theme));
    }
    if !row.devices.is_empty() {
        detail = detail.child(activity(view, theme));
    }
    if row.group() == Group::Agents {
        detail = detail.child(
            div()
                .id("members-managed-in-settings")
                .px(px(20.))
                .pt(space::LG)
                .text_size(text::SECONDARY)
                .text_color(theme.muted)
                .child(
                    "Suspend, revoke, rename and keys live in Account → Agents, for the manager.",
                ),
        );
    }
    detail.into_any_element()
}

/// The avatar, the name, one caption line of what the account is, its bio,
/// and the two ways out of this screen.
fn head(
    view: &Members,
    row: &Row,
    rows: &[Row],
    over: bool,
    cx: &mut Context<Members>,
    theme: &Theme,
) -> impl IntoElement {
    let text = |text: String| div().child(text).into_any_element();
    // each part keeps its words (and the manager its link) on one line;
    // a narrow pane wraps between parts
    let mut caption: Vec<Vec<AnyElement>> = vec![vec![text(format!("account {}", row.number))]];
    match &row.kind {
        identity::Kind::Person => caption.push(vec![text("Person".into())]),
        identity::Kind::Module(program) if *program == row.name => {
            caption.push(vec![text("Module".into())])
        }
        identity::Kind::Module(_) => caption.push(vec![text(
            row.kind.badge(|_| String::new()).unwrap_or_default(),
        )]),
        identity::Kind::Managed {
            manager, standing, ..
        } => {
            // the badge with the manager's name left out, and the name as
            // a link that chooses the manager here
            let what = row.kind.badge(|_| String::new()).unwrap_or_default();
            let manager = *manager;
            let label = name(rows, manager).unwrap_or_else(|| format!("#{manager}"));
            let link = div()
                .id("members-manager")
                .text_color(theme.foreground)
                .text_decoration_1()
                .cursor_pointer()
                .role(Role::Link)
                .focusable()
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| view.select(manager, cx)))
                .child(label)
                .into_any_element();
            caption.push(vec![text(what.trim_end().to_owned()), link]);
            caption.push(vec![
                standing_badge("members-standing", &row.kind, *standing, theme).into_any_element(),
            ]);
        }
    }
    if row.group() != Group::Modules {
        let devices = design::plural(row.devices.len() as u64, "device", "devices");
        caption.push(vec![text(devices)]);
    }
    if let Some(standing) = &row.standing {
        // a validator in the success colours, a resident quiet: Nodes'
        // memberships drew them so
        let (foreground, background) = match standing.as_str() {
            "Validator" => (theme.success, theme.success_soft),
            _ => (theme.muted, theme.surface_raised),
        };
        caption.push(vec![
            design::badge(
                "members-validator",
                standing.clone(),
                foreground,
                background,
            )
            .into_any_element(),
        ]);
    }
    let last = caption.len() - 1;
    let line = caption.into_iter().enumerate().map(|(index, part)| {
        div()
            .flex()
            .items_center()
            .gap(space::XS)
            .whitespace_nowrap()
            .children(part)
            .when(index < last, |part| part.child("·"))
    });
    let number = row.number;
    let explorer = design::explorer::link(&design::explorer::account_path(number));
    let dm = match (view.me, &row.kind) {
        (Some(me), identity::Kind::Person | identity::Kind::Managed { .. }) if me != number => {
            ducklink::mint(
                &view.chain,
                chat::MODULE,
                &[&chat::dm_channel_id(me, number)],
            )
        }
        _ => None,
    };
    div()
        .id("members-detail-head")
        .flex()
        .items_start()
        .gap(space::LG)
        .px(px(20.))
        .pt(space::BLOCK)
        .pb(px(14.))
        .border_b_1()
        .border_color(theme.border)
        .child(avatar(row, px(32.), row.kind.note().is_some(), theme))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .child(
                    div()
                        .id("members-detail-title")
                        // one line: a long name ends in an ellipsis
                        .truncate()
                        .text_size(text::TITLE)
                        .font_weight(FontWeight::SEMIBOLD)
                        .role(Role::Heading)
                        .aria_level(2)
                        .child(row.name.clone()),
                )
                .child(
                    div()
                        .id("members-caption")
                        .mt(px(3.))
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_x(space::XS)
                        .gap_y(space::HAIR)
                        .font_family(design::fonts::FAMILY_MONO)
                        .text_size(text::CAPTION)
                        .text_color(theme.muted)
                        .children(line),
                )
                .when_some(
                    row.bio.clone().filter(|bio| !bio.trim().is_empty()),
                    |who, bio| {
                        who.child(
                            div()
                                .id("members-bio")
                                .mt(space::SM)
                                .max_w(px(460.))
                                .child(bio),
                        )
                    },
                ),
        )
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .gap(space::XS)
                .when_some(dm, |buttons, link| {
                    buttons.child(outline_button("members-open-dm", "Open DM", link, theme))
                })
                .child(outline_button(
                    "members-explorer",
                    "Explorer",
                    explorer,
                    theme,
                ))
                .when(over, |buttons| {
                    let close = cx.listener(|view, _: &ClickEvent, _, cx| {
                        view.selected = None;
                        view.activity = Loadable::Idle;
                        cx.notify();
                    });
                    buttons.child(design::button(
                        "members-detail-close",
                        "Close",
                        theme,
                        close,
                    ))
                }),
        )
}

/// Active, suspended or revoked, in its colour.
fn standing_badge(
    id: &'static str,
    kind: &identity::Kind,
    standing: identity::Standing,
    theme: &Theme,
) -> Stateful<Div> {
    let (foreground, background) = match standing {
        identity::Standing::Active => (theme.success, theme.success_soft),
        identity::Standing::Suspended => (theme.warning, theme.warning_soft),
        identity::Standing::Revoked => (theme.danger, theme.danger_soft),
    };
    design::badge(id, kind.note().unwrap_or("active"), foreground, background)
}

/// A 1px-edged button that opens `link` through the host.
fn outline_button(
    id: &'static str,
    label: &'static str,
    link: String,
    theme: &Theme,
) -> Stateful<Div> {
    let theme = *theme;
    div()
        .id(id)
        .flex()
        .items_center()
        .h(size::CONTROL)
        .px(space::MD)
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.background)
        .text_size(text::SECONDARY)
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(move |style| style.bg(theme.surface))
        .role(Role::Button)
        .focusable()
        .on_click(move |_: &ClickEvent, _, cx| cx.host().open_link(&link))
        .child(label)
}

/// A detail section: its heading and count over its rows, a hairline under.
fn section(
    id: &'static str,
    title: &'static str,
    count: String,
    body: Vec<AnyElement>,
    theme: &Theme,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .flex_col()
        .pb(space::XS)
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .items_center()
                .px(px(20.))
                .pt(space::LG)
                .pb(space::XS)
                .child(
                    design::heading(SharedString::from(format!("{id}-heading")), title, 2, theme)
                        .flex_1(),
                )
                .child(
                    design::mono(count)
                        .text_size(text::CAPTION)
                        .text_color(theme.muted),
                ),
        )
        .children(body)
}

/// One line of a section.
fn line() -> Div {
    div()
        .flex()
        .items_center()
        .gap(space::MD)
        .h(size::ROW)
        .px(px(20.))
}

/// A section with nothing in it says so, quietly.
fn empty(text: &'static str, theme: &Theme) -> AnyElement {
    div()
        .px(px(20.))
        .pt(space::HAIR)
        .pb(space::SM)
        .text_size(text::SECONDARY)
        .text_color(theme.muted)
        .child(text)
        .into_any_element()
}

fn devices(row: &Row, theme: &Theme) -> impl IntoElement {
    let body = if row.devices.is_empty() {
        vec![empty("No devices", theme)]
    } else {
        row.devices
            .iter()
            .enumerate()
            .map(|(index, device)| {
                let label = device
                    .label
                    .clone()
                    .unwrap_or_else(|| format!("Device {}", index + 1));
                let hex: String = device
                    .key
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect();
                line()
                    .child(div().w(KEY_COLUMN).min_w(px(0.)).truncate().child(label))
                    .child(
                        design::mono(design::short_hex(&hex))
                            .text_size(text::CAPTION)
                            .text_color(theme.muted),
                    )
                    .child(
                        design::mono(format!("added {}", design::day(device.added_at)))
                            .ml_auto()
                            .text_size(text::CAPTION)
                            .text_color(theme.faint),
                    )
                    .into_any_element()
            })
            .collect()
    };
    section(
        "members-devices",
        "Devices",
        row.devices.len().to_string(),
        body,
        theme,
    )
}

/// The agents a person manages; pressing one chooses it here.
fn manages(managed: &[&Row], cx: &mut Context<Members>, theme: &Theme) -> impl IntoElement {
    let body = managed
        .iter()
        .filter_map(|agent| {
            let identity::Kind::Managed {
                category: identity::Category::Agent,
                standing,
                ..
            } = agent.kind
            else {
                return None;
            };
            let number = agent.number;
            let theme = *theme;
            let about = format!("account {number} · Agent");
            let row = line()
                .id(format!("members-manages-{}", number))
                .cursor_pointer()
                .hover(move |style| style.bg(theme.surface))
                .role(Role::Button)
                // the agent's name, not the avatar's initial drawn before it
                .aria_label(agent.name.clone())
                .aria_description(format!(
                    "{about} · {}",
                    agent.kind.note().unwrap_or("active")
                ))
                .focusable()
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| view.select(number, cx)))
                .child(avatar(
                    agent,
                    size::AVATAR_SM,
                    agent.kind.note().is_some(),
                    &theme,
                ))
                .child(
                    div()
                        .w(KEY_COLUMN)
                        .min_w(px(0.))
                        .truncate()
                        .child(agent.name.clone()),
                )
                .child(
                    design::mono(about)
                        .text_size(text::CAPTION)
                        .text_color(theme.muted),
                )
                .child(div().ml_auto().child(standing_badge(
                    "members-manages-standing",
                    &agent.kind,
                    standing,
                    &theme,
                )));
            Some(row.into_any_element())
        })
        .collect();
    section(
        "members-manages",
        "Manages",
        managed.len().to_string(),
        body,
        theme,
    )
}

fn activity(view: &Members, theme: &Theme) -> impl IntoElement {
    let window = design::plural(WINDOW, "block", "blocks");
    let body = match &view.activity {
        Loadable::Idle | Loadable::Loading(_) => vec![
            div()
                .id("members-activity-loading")
                .px(px(20.))
                .pb(space::SM)
                .child(design::quiet(format!("Reading the last {window}…"), theme))
                .into_any_element(),
        ],
        Loadable::Failed(refusal) => vec![
            div()
                .px(px(20.))
                .pb(space::SM)
                .child(design::quiet(refusal.message.clone(), theme))
                .into_any_element(),
        ],
        Loadable::Ready(recent) | Loadable::Reloading(recent, _) if recent.items.is_empty() => {
            vec![
                div()
                    .id("members-no-activity")
                    .px(px(20.))
                    .pb(space::SM)
                    .child(design::quiet(
                        format!("Nothing signed in the last {window}."),
                        theme,
                    ))
                    .into_any_element(),
            ]
        }
        Loadable::Ready(recent) | Loadable::Reloading(recent, _) => recent
            .items
            .iter()
            .enumerate()
            .map(|(index, signed)| {
                line()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .truncate()
                            .child(signed.title.clone()),
                    )
                    .child(design::block_link(
                        format!("members-block-{}", index),
                        signed.height,
                        theme,
                    ))
                    .child(
                        div()
                            .w(px(52.))
                            .flex()
                            .justify_end()
                            .text_size(text::CAPTION)
                            .text_color(theme.faint)
                            .child(design::ago(recent.now, signed.time)),
                    )
                    .into_any_element()
            })
            .collect(),
    };
    section(
        "members-activity",
        "Recent activity",
        format!("last {window}"),
        body,
        theme,
    )
}
