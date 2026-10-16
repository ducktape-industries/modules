//! Refs: branches and tags, how far each one is from the default head, and
//! the Change a comparison can become.
use std::rc::Rc;

use view::prelude::*;

use crate::Forge;
use crate::ui::components::{
    badge, button, empty_state, id, name_cell, quiet, ref_label, row, short_hex, whole_name,
};
use crate::ui::diff::Route;
use crate::ui::{pending, staged};
use crate::words;
use forge::{Query, RefInfo, Reply, Revision};
use view::design::Ladder;

pub(crate) fn render(
    forge: &Forge,
    cx: &mut Context<Forge>,
    theme: &Theme,
    ladder: &Ladder,
    head: &[u8],
) -> AnyElement {
    let ladder = *ladder;
    let mut column = div()
        .id(id("forge-refs"))
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.))
        .child(pending(forge, "changes", theme));
    if let Some(form) = forge.change_form() {
        column = column.child(crate::ui::changes::form(form, forge, cx, theme, &ladder));
    }
    let reply = match staged(
        forge,
        &forge.refs_query(),
        "forge-refs-list",
        words::READING_REFS,
        cx,
        theme,
        &ladder,
    ) {
        Ok(reply) => reply,
        Err(state) => return column.child(state).into_any_element(),
    };
    let Reply::Refs { page, .. } = reply else {
        return column.into_any_element();
    };
    if page.items.is_empty() {
        return column
            .child(empty_state(
                id("forge-refs-empty"),
                words::NO_REFS,
                words::UNBORN,
                theme,
            ))
            .into_any_element();
    }
    // one Tab stop: ↑ ↓ walk the refs, ← → a row's cells (the ref, its
    // Compare when the reader may write), Enter presses the active cell. A
    // virtual list: a render draws the refs on screen, not every branch
    let Some(shared) = forge.shared(&forge.refs_query()) else {
        return column.into_any_element();
    };
    let names: Vec<Vec<u8>> = page.items.iter().map(|info| info.name.clone()).collect();
    let count = names.len();
    let (at, cell) = crate::ui::components::cursor(forge, "forge-refs-list");
    let at = at.min(count.saturating_sub(1));
    let compares = |name: &[u8]| name != forge.default_head() && !name.starts_with(b"refs/tags/");
    let cells =
        1 + usize::from(forge.may_write() && names.get(at).is_some_and(|name| compares(name)));
    // what each row shows, read once a render for the rows the list draws
    let marks = Marks {
        default: forge.default_head(),
        browsing: forge.head_name(),
        may_write: forge.may_write(),
        walked: crate::ui::components::walked(forge, "forge-refs-list"),
        active: (at, cell),
        standings: standings(forge, &names, head),
    };
    let list = crate::ui::components::grid(
        "forge-refs-list",
        "Refs",
        count,
        cells,
        Some(&forge.refs_scroll),
        forge,
        cx,
        move |forge, index, cell, window, cx| match cell {
            0 => forge.pick_ref(names[index].clone(), cx),
            _ if forge.may_write() => forge.start_change(names[index].clone(), window, cx),
            _ => {}
        },
    )
    .p_2();
    let pick: Route<Vec<u8>> =
        Rc::new(cx.listener(|forge, name: &Vec<u8>, _, cx| forge.pick_ref(name.clone(), cx)));
    let start: Route<Vec<u8>> = Rc::new(cx.listener(|forge, name: &Vec<u8>, window, cx| {
        forge.start_change(name.clone(), window, cx)
    }));
    // what the settings forbid closes the list, after its last ref
    let forbidden = forbidden(forge);
    let theme = *theme;
    let rows = crate::ui::components::rows(
        "forge-refs-rows",
        count + usize::from(forbidden.is_some()),
        None,
        Some(&forge.refs_scroll),
        move |index| {
            let Reply::Refs { page, .. } = &*shared else {
                return div().into_any_element();
            };
            match page.items.get(index) {
                Some(info) => ref_row(info, index, &marks, &pick, &start, &theme, &ladder),
                None => div()
                    .id(id("forge-refs-forbidden"))
                    .min_h(design::size::CONTROL)
                    .flex()
                    .items_center()
                    .child(quiet(forbidden.clone().unwrap_or_default(), &theme))
                    .into_any_element(),
            }
        },
    );
    column.child(list.child(rows)).into_any_element()
}

/// The ref name and target columns.
const NAME_W: Pixels = px(200.);
const TARGET_W: Pixels = px(100.);

/// What marks the refs' rows, read once a render: the default head and the
/// ref browsed, whether the reader may write, where the arrows are, and
/// each ref's standing against the head.
struct Marks {
    default: Vec<u8>,
    browsing: Vec<u8>,
    may_write: bool,
    walked: bool,
    /// the active row and its cell
    active: (usize, usize),
    standings: Vec<Option<String>>,
}

/// One ref: its name, kind, target and standing, and Compare for a branch
/// other than the default head. Spaced from the next as the list draws
/// every row one height.
fn ref_row(
    info: &RefInfo,
    index: usize,
    marks: &Marks,
    pick: &Route<Vec<u8>>,
    start: &Route<Vec<u8>>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let name = info.name.clone();
    let label = ref_label(&name);
    let tag = name.starts_with(b"refs/tags/");
    let active = (index == marks.active.0).then_some(marks.active.1);
    let press = |route: &Route<Vec<u8>>| {
        let (route, name) = (route.clone(), name.clone());
        move |_: &ClickEvent, window: &mut Window, app: &mut App| route(&name, window, app)
    };
    let mut line = row(format!("forge-ref-row-{label}"), theme)
        .in_grid()
        .on_click(press(pick))
        .active(active)
        .walked(marks.walked)
        .selected(name == marks.browsing)
        .cell(whole_name(
            name_cell(label.clone())
                .id(id(format!("forge-ref-name-{label}")))
                .w(NAME_W)
                .font_weight(view::FontWeight::MEDIUM),
            label.clone(),
            theme,
        ))
        .cell(badge(
            id(format!("forge-ref-kind-{label}")),
            if tag { "tag" } else { "branch" },
            ladder,
            theme.muted,
            theme.surface_raised,
        ))
        .cell(
            div()
                .w(TARGET_W)
                .whitespace_nowrap()
                .font_family(design::fonts::FAMILY_MONO)
                .text_size(design::text::SECONDARY)
                .text_color(theme.muted)
                .child(short_hex(&info.target)),
        )
        .cell(match &marks.standings[index] {
            Some(standing) => quiet(standing.clone(), theme),
            None => div().into_any_element(),
        })
        .cell(div().flex_1());
    if name != marks.default && !tag {
        line = line.control(
            button(
                id(format!("forge-compare-{label}")),
                "Compare →",
                ladder,
                theme,
                press(start),
            )
            .enabled(marks.may_write)
            .item(active == Some(1)),
        );
    }
    div()
        .id(id(format!("forge-ref-slot-{label}")))
        .pb_1()
        .child(line)
        .into_any_element()
}

/// What this repository's settings forbid, if anything.
fn forbidden(forge: &Forge) -> Option<String> {
    let allow = forge
        .repo()
        .map(|(info, _, _)| {
            (
                info.repo.settings.allow_force,
                info.repo.settings.allow_delete,
            )
        })
        .unwrap_or((false, false));
    words::forbids(allow.0, allow.1)
}

fn standings(forge: &Forge, names: &[Vec<u8>], head: &[u8]) -> Vec<Option<String>> {
    names
        .iter()
        .map(|name| {
            if name == head {
                return Some(words::BROWSING.to_owned());
            }
            let branch = name.starts_with(b"refs/heads/");
            if !branch {
                return None;
            }
            let query = Query::Compare {
                repo: forge.repo_name(),
                from: Revision::Ref(name.clone()),
                into: Revision::Ref(head.to_vec()),
            };
            Some(match forge.ready(&query) {
                Some(Reply::Compare { comparison, .. }) => words::compared(comparison),
                _ => words::COMPARING.to_owned(),
            })
        })
        .collect()
}
