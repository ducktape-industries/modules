//! Forge's root (Expo `app/(tabs)/forge/index.tsx`, board 60): "Forge"
//! over the network and the reader, New repository in the title row, the
//! filter, the changes that need my judgment across every repository,
//! then every repository; and the New repository sheet.
//!
//! Reads `Query::Repos`, `Query::Judgment` (for me) and the filter box;
//! writes a new repository (`create_repo`).
use view::design::{Kind, Ladder, Nav, TextType};
use view::prelude::*;
use view::wire;
use view::{Div, Stateful};

use super::parts::{self, Row};
use crate::id;
use crate::queries::PAGE;
use crate::state::NewRepo;
use crate::words;
use crate::{Forge, Stage};
use forge::{Judgment, Query, Reply, RepoInfo};

pub(super) fn render(
    forge: &Forge,
    nav: Nav,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    let new = cx.listener(|forge, _: &ClickEvent, window, cx| forge.start_repo(window, cx));
    let page = design::page("forge-page-repos", "Forge", nav, ladder, theme)
        .caption(place(forge))
        .trailing(design::icon_button(
            id("forge-new-repo"),
            design::icon("plus", ladder.icon),
            words::NEW_REPOSITORY,
            ladder,
            theme,
            new,
        ));
    let body = div()
        .id(id("forge-repos-scroll"))
        .flex_1()
        .min_h(px(0.))
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .child(filter(forge, ladder, theme))
        .children(parts::pending(forge, "repos", ladder, theme))
        .children(judgment(forge, cx, ladder, theme))
        .children(repositories(forge, cx, ladder, theme));
    page.children(parts::notices(forge, ladder, theme))
        .child(body)
        .into_any_element()
}

/// The header's caption: the network, then the reader (`network ·
/// account`).
fn place(forge: &Forge) -> String {
    let chain = forge.session.chain_id.as_str();
    let network = chain.split_once('#').map_or(chain, |(label, _)| label);
    let me = forge.me_principal().map(|me| forge.principal_name(&me));
    [Some(network.to_owned()), me]
        .into_iter()
        .flatten()
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// "Filter by name", 16 in, 8 under.
fn filter(forge: &Forge, ladder: &Ladder, theme: &Theme) -> Div {
    let input = Input::new(
        id("forge-repos-search"),
        &forge.search,
        words::FILTER_BY_NAME,
    )
    .autocorrect(false)
    .autocapitalize(Autocapitalize::None)
    .virtual_keyboard(VirtualKeyboard::Search)
    .placeholder(words::FILTER_BY_NAME);
    div()
        .px(design::space::BLOCK)
        .pb(design::space::SM)
        .child(parts::search_field(input, "search", ladder, theme))
}

/// "Needs my judgment" and its changes, each opening its change; nothing
/// while none waits. A refused read says so in its place.
fn judgment(
    forge: &Forge,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> Vec<AnyElement> {
    let Some(query) = forge.judgment_query() else {
        return Vec::new();
    };
    // while it is read nothing shows here: only a refusal says so
    if matches!(forge.stage(&query), Stage::Loading) {
        return Vec::new();
    }
    let items = match parts::stage(forge, &query, "forge-judgment", "", cx, (ladder, theme)) {
        Ok(Reply::Judgment { page, .. }) => &page.items,
        Ok(_) => return Vec::new(),
        Err(refused) => return vec![refused],
    };
    if items.is_empty() {
        return Vec::new();
    }
    let head = design::section_head(
        id("forge-judgment-head"),
        "Needs my judgment",
        ladder,
        theme,
    )
    .child(items.len().to_string());
    vec![
        head.into_any_element(),
        judgment_list(items, forge, cx, ladder, theme).into_any_element(),
    ]
}

fn judgment_list(
    items: &[Judgment],
    forge: &Forge,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    let routes: Vec<String> = items
        .iter()
        .map(|item| format!("{}/{}", item.change.repo, item.change.n))
        .collect();
    let rows = items
        .iter()
        .zip(&routes)
        .map(|(item, route)| {
            let row = parts::change_row(&item.change, Some(item), true, forge, ladder, theme);
            let route = route.clone();
            let open: parts::Press = Box::new(
                cx.listener(move |forge, _: &ClickEvent, _, cx| forge.open_route(&route, cx)),
            );
            (row, open)
        })
        .collect();
    parts::rows_list(
        "forge-judgment-list",
        "Needs my judgment",
        rows,
        move |forge, index, _, cx| forge.open_route(&routes[index], cx),
        forge,
        cx,
        (ladder, theme),
    )
}

/// "Repositories" and every one the filter keeps; until the read lands
/// its quiet line, and once it has, why the list is empty when it is.
fn repositories(
    forge: &Forge,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> Vec<AnyElement> {
    let query = Query::Repos { page: PAGE };
    let all = match parts::stage(
        forge,
        &query,
        "forge-repos-list",
        words::READING_REPOSITORIES,
        cx,
        (ladder, theme),
    ) {
        Ok(Reply::Repos { page, .. }) => &page.items,
        Ok(_) => return Vec::new(),
        Err(state) => return vec![section(0, ladder, theme), state],
    };
    let needle = forge.search.text().trim().to_lowercase();
    let shown: Vec<&RepoInfo> = all
        .iter()
        .filter(|info| needle.is_empty() || info.name.to_lowercase().contains(&needle))
        .collect();
    let body = match (shown.is_empty(), all.is_empty()) {
        (true, true) => parts::empty_state(
            id("forge-repos-empty"),
            "hammer",
            words::NO_REPOSITORIES_YET,
            words::CREATE_ONE,
            ladder,
            theme,
        )
        .into_any_element(),
        (true, false) => parts::empty_state(
            id("forge-repos-empty"),
            "search",
            words::NOTHING_MATCHES,
            words::NO_REPOSITORY_MATCHES,
            ladder,
            theme,
        )
        .into_any_element(),
        (false, _) => repo_list(&shown, forge, cx, ladder, theme).into_any_element(),
    };
    vec![section(shown.len(), ladder, theme), body]
}

/// "Repositories" and how many show (no count at none).
fn section(count: usize, ladder: &Ladder, theme: &Theme) -> AnyElement {
    design::section_head(id("forge-repos-head"), "Repositories", ladder, theme)
        .children((count > 0).then(|| count.to_string()))
        .into_any_element()
}

fn repo_list(
    shown: &[&RepoInfo],
    forge: &Forge,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    let names: Vec<String> = shown.iter().map(|info| info.name.clone()).collect();
    let rows = shown
        .iter()
        .map(|info| {
            let name = info.name.clone();
            let open: parts::Press =
                Box::new(cx.listener(move |forge, _: &ClickEvent, _, cx| {
                    forge.open_repo(name.clone(), cx)
                }));
            (repo_row(info, forge, ladder, theme), open)
        })
        .collect();
    parts::rows_list(
        "forge-repos-list",
        "Repositories",
        rows,
        move |forge, index, _, cx| forge.open_repo(names[index].clone(), cx),
        forge,
        cx,
        (ladder, theme),
    )
}

/// A repository (`RepoRow`): its name, `owner <name> · N refs · active
/// block <height>`, the height as it is, not grouped.
fn repo_row(info: &RepoInfo, forge: &Forge, ladder: &Ladder, theme: &Theme) -> Row {
    let owner = forge.principal_name(&info.repo.owner);
    let facts = format!(
        "owner {owner} · {} · active block {}",
        design::plural(info.repo.refs_count, "ref", "refs"),
        info.repo.last_activity
    );
    Row::titled(
        id(format!("forge-repo-{}", info.name)),
        info.name.clone(),
        ladder,
    )
    .lead(parts::lead_icon("folder", ladder, theme))
    .caption(parts::caption(facts, ladder, theme))
    .chevron()
}

// ------------------------------------------------------------------ New repository

/// The New repository sheet (`NewRepository`): a name, the object format
/// (fixed once created), Cancel and Create.
pub(super) fn new_repository(
    form: &NewRepo,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    let close = cx.listener(|forge, _: &(), _, cx| forge.cancel_repo(cx));
    let body = div()
        .id(id("forge-new-repo-body"))
        .flex()
        .flex_col()
        .gap(design::space::BLOCK)
        .p(design::space::BLOCK)
        .child(name_field(form, cx, ladder, theme))
        .child(format_choice(form, cx, ladder, theme))
        .child(new_repository_actions(cx, ladder, theme));
    design::sheet(
        id("forge-new-repo-sheet"),
        words::NEW_REPOSITORY,
        ladder,
        theme,
    )
    .on_dismiss(close)
    .child(parts::sheet_head(
        "forge-new-repo-title",
        words::NEW_REPOSITORY,
        ladder,
        theme,
    ))
    .child(body)
    .into_any_element()
}

/// "Name": the field, its rule, and why a name was refused.
fn name_field(form: &NewRepo, cx: &mut Context<Forge>, ladder: &Ladder, theme: &Theme) -> Div {
    // a name typed again takes the last refusal with it
    let typed = cx.listener(|forge, _: &wire::TextChange, _, cx| {
        if let Some(form) = &mut forge.new_repo {
            form.error.clear();
        }
        cx.notify();
    });
    let create = cx.listener(|forge, _: &(), _, cx| forge.create_repo(cx));
    let input = Input::new(id("forge-new-repo-name"), &form.name, "Name")
        .autocorrect(false)
        .autocapitalize(Autocapitalize::None)
        .placeholder(words::REPOSITORY_NAME)
        .on_change(typed)
        .on_submit(create);
    parts::input_field(
        Some("Name"),
        input,
        (&form.error, Some(words::NAME_RULE)),
        ladder,
        theme,
    )
}

/// "Object format": SHA-1 | SHA-256, and what picking SHA-1 is for.
fn format_choice(form: &NewRepo, cx: &mut Context<Forge>, ladder: &Ladder, theme: &Theme) -> Div {
    let pick = cx.processor(|forge, picked: usize, _, cx| {
        if let Some(form) = &mut forge.new_repo {
            form.sha1 = picked == 0;
        }
        cx.notify();
    });
    let formats = design::segmented(
        id("forge-new-repo-format"),
        words::OBJECT_FORMAT,
        ladder,
        theme,
        Some(usize::from(!form.sha1)),
        [
            (id("forge-new-repo-sha1"), "SHA-1".into()),
            (id("forge-new-repo-sha256"), "SHA-256".into()),
        ],
        pick,
    );
    let line = |text: &'static str| {
        div()
            .text_type(ladder.caption)
            .text_color(theme.muted)
            .child(text)
    };
    div()
        .flex()
        .flex_col()
        .gap(design::space::XS)
        .child(line(words::OBJECT_FORMAT))
        .child(formats)
        .child(line(words::FORMAT_RULE))
}

/// Cancel and Create, at the right.
fn new_repository_actions(cx: &mut Context<Forge>, ladder: &Ladder, theme: &Theme) -> Div {
    let cancel = cx.listener(|forge, _: &ClickEvent, _, cx| forge.cancel_repo(cx));
    let create = cx.listener(|forge, _: &ClickEvent, _, cx| forge.create_repo(cx));
    div()
        .flex()
        .justify_end()
        .gap(design::space::SM)
        .child(
            design::button(id("forge-new-repo-cancel"), "Cancel", ladder, theme, cancel)
                .kind(Kind::Quiet),
        )
        .child(
            design::button(id("forge-new-repo-create"), "Create", ladder, theme, create)
                .kind(Kind::Primary),
        )
}
