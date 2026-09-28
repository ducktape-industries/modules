//! The window: a repositories rail, one content column, one docked panel.
//! Narrow windows fold the rail and the dock into toggles rather than
//! squeezing three columns into one.
pub(crate) mod change;
pub(crate) mod changes;
pub(crate) mod code;
pub(crate) mod commits;
pub(crate) mod components;
pub(crate) mod conversation;
pub(crate) mod diff;
pub(crate) mod dock;
pub(crate) mod highlight;
pub(crate) mod markdown;
pub(crate) mod readme;
pub(crate) mod refs;
pub(crate) mod repos;
pub(crate) mod settings;

use ducktape_view_guest::design;
use ducktape_view_guest::prelude::*;
use ducktape_view_guest::{Div, FontWeight, Stateful};

use crate::Forge;
use crate::state::{Dock, Menu, Progress, RepoTab};
use components::{button, heading, id, quiet};
use ducktape_view_guest::MouseDownEvent;
use forge::Reply;

/// The About dock beside a repository.
const DOCK_W: Pixels = px(300.);
/// A page's side inset.
pub(crate) const PAGE_X: Pixels = px(20.);
/// A tab bar's height.
pub(crate) const TAB_BAR_H: Pixels = px(38.);
/// A fact's label column.
const FACT_LABEL_W: Pixels = px(120.);

pub(crate) fn render(forge: &mut Forge, cx: &mut Context<Forge>) -> impl IntoElement {
    let theme = *cx.global::<Theme>();
    let measured = |cx: &mut Context<Forge>| {
        cx.listener(|forge, size: &(Pixels, Pixels), _, cx| {
            forge.measured(f32::from(size.0), f32::from(size.1), cx)
        })
    };
    let mut columns = div()
        .id(id("forge-columns"))
        .flex()
        .size_full()
        .min_h(px(0.));
    // The rail switches between repositories; with none open, the list
    // itself is the screen, and a rail beside it would say it twice.
    if forge.layout.tree_visible() && forge.nav().repo.is_some() {
        columns = columns
            .child(repos::rail(forge, cx, &theme))
            .child(design::divider(
                id("forge-rail-resize"),
                "Resize the repository list",
                &theme,
                cx,
                |forge: &mut Forge, dx| {
                    forge.layout.tree += dx;
                    forge.layout.clamp();
                },
            ));
    }
    columns = columns.child(main(forge, cx, &theme));
    // a change carries its own details; the About dock is a repository's
    if let Some(dock) = forge
        .nav()
        .dock
        .filter(|_| forge.layout.dock_visible() && forge.nav().change.is_none())
    {
        columns = columns.child(panel(forge, dock, cx, &theme));
    }
    let root = div()
        .id(id("forge"))
        .flex()
        .flex_col()
        .size_full()
        .bg(theme.background)
        .text_color(theme.foreground)
        .text_size(design::text::BODY)
        .child(columns);
    ducktape_view_guest::sensor(id("forge-viewport"), root)
        .size_full()
        .on_show(measured(cx))
        .on_resize(measured(cx))
}

fn main(forge: &Forge, cx: &mut Context<Forge>, theme: &Theme) -> AnyElement {
    let mut column = div()
        .id(id("forge-main"))
        .flex_1()
        .min_w(px(0.))
        .flex()
        .flex_col()
        .min_h(px(0.));
    if forge.layout.narrow() {
        column = column.child(narrow_bar(forge, cx, theme));
    }
    if !forge.notice.is_empty() {
        column = column.child(
            div()
                .id(id("forge-notice"))
                .m_2()
                .p_2()
                .bg(theme.danger_soft)
                .text_color(theme.foreground)
                .text_size(design::text::SECONDARY)
                .child(forge.notice.clone()),
        );
    }
    if forge.session.connected && forge.me_principal().is_none() {
        column = column.child(no_account(theme));
    }
    let body: AnyElement = match (forge.nav().repo.clone(), forge.nav().change) {
        (None, _) => repos::overview(forge, cx, theme),
        (Some(_), Some(_)) => change::render(forge, cx, theme),
        (Some(_), None) => repo(forge, cx, theme),
    };
    column.child(body).into_any_element()
}

/// Why every write control is off: the seated key holds no account. The
/// same rule and wording pattern as chat's `NoAccount`.
fn no_account(theme: &Theme) -> AnyElement {
    div()
        .id(id("forge-no-account"))
        .m_2()
        .p_3()
        .bg(theme.warning_soft)
        .text_color(theme.muted)
        .text_size(design::text::SECONDARY)
        .child(
            "To create repositories, push, open changes or review, create or join an account in \
             Account. You can read every repository without an account.",
        )
        .into_any_element()
}

/// The repository header: its name, what it is and its clone address,
/// then the ref picker and the tabs on one bar.
fn repo(forge: &Forge, cx: &mut Context<Forge>, theme: &Theme) -> AnyElement {
    let name = forge.repo_name();
    let head = forge.head_name();
    let mut tabs = div()
        .id(id("forge-tabs"))
        .h(TAB_BAR_H)
        .flex()
        .items_center()
        .gap(design::space::XL)
        .px(PAGE_X)
        .border_b_1()
        .border_color(theme.border)
        .child(ref_picker(forge, cx, theme));
    let mut list = div()
        .id(id("forge-tab-list"))
        .h_full()
        .flex()
        .items_center()
        .gap(design::space::XL)
        .role(Role::TabList)
        .aria_label("Repository");
    for tab in RepoTab::ALL {
        let open = cx.listener(move |forge, _: &ClickEvent, _, cx| forge.open_tab(tab, cx));
        let count = (tab == RepoTab::Changes)
            .then(|| forge.open_changes())
            .flatten();
        list = list.child(components::tab(
            id(format!("forge-tab-{}", tab.slug())),
            tab.label(),
            count,
            forge.nav().tab == tab,
            theme,
            open,
        ));
    }
    let about = cx.listener(|forge, _: &ClickEvent, _, cx| forge.toggle_dock(Dock::About, cx));
    tabs = tabs.child(list).child(div().flex_1()).child(
        button(id("forge-dock-about"), "About", theme, about)
            .selected(forge.nav().dock == Some(Dock::About))
            .kind(design::Kind::Quiet),
    );
    let body: AnyElement = match forge.nav().tab {
        RepoTab::Readme => readme::render(forge, cx, theme),
        RepoTab::Code => code::render(forge, cx, theme),
        RepoTab::Commits => commits::render(forge, cx, theme),
        RepoTab::Changes => changes::render(forge, cx, theme),
        RepoTab::Refs => refs::render(forge, cx, theme, &head),
        RepoTab::Settings => settings::render(forge, cx, theme),
    };
    div()
        .id(id("forge-repo"))
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.))
        .child(title(forge, &name, cx, theme))
        .child(tabs)
        .child(body)
        .into_any_element()
}

/// The repository's name, one mono line of what it is (owner, refs, the
/// block it was last active at), and its address with Copy.
fn title(forge: &Forge, name: &str, cx: &mut Context<Forge>, theme: &Theme) -> AnyElement {
    let url = repo_link(forge, name);
    let copy = cx.listener({
        let (name, url) = (name.to_owned(), url.clone());
        move |forge, _: &ClickEvent, _, cx| {
            cx.host()
                .notify::<ducktape_view_guest::methods::ClipboardWrite>(url.clone());
            forge.copied = Some(name.clone());
            cx.notify();
        }
    });
    let copied = forge.copied.as_deref() == Some(name);
    let mut title = div()
        .id(id("forge-repo-header"))
        .flex()
        .flex_wrap()
        .items_center()
        .gap(design::space::MD)
        .px(PAGE_X)
        .pt(design::space::BLOCK)
        .pb(design::space::XS)
        .child(heading(id("forge-repo-name"), name.to_owned(), 1, theme));
    if let Some((info, _, _)) = forge.repo() {
        let owner = forge.principal_name(&info.repo.owner);
        title = title.child(
            div()
                .id(id("forge-repo-owner"))
                .flex()
                .items_center()
                .gap(design::space::XS)
                .font_family(design::fonts::FAMILY_MONO)
                .text_size(design::text::CAPTION)
                .text_color(theme.muted)
                .child(format!(
                    "owner {owner} · {} · active",
                    design::plural(info.repo.refs_count, "ref", "refs")
                ))
                .child(design::block_link(
                    id("forge-repo-activity"),
                    info.repo.last_activity,
                    theme,
                )),
        );
    }
    title
        .child(div().flex_1())
        .child(
            div()
                .min_w(px(0.))
                .truncate()
                .font_family(design::fonts::FAMILY_MONO)
                .text_size(design::text::CAPTION)
                .text_color(theme.faint)
                .child(url),
        )
        .child(
            button(
                id("forge-repo-copy"),
                if copied { "Copied" } else { "Copy" },
                theme,
                copy,
            )
            .kind(design::Kind::Quiet),
        )
        .into_any_element()
}

/// `duck://<chain>/forge/<name>`: the repository's clone address and link,
/// minted by ducklink from the session's chain id (`<label>#<salt>`); a
/// placeholder `duck://<network>/forge/<name>` while no chain is known.
pub(crate) fn repo_link(forge: &Forge, name: &str) -> String {
    ducklink::mint(&forge.session.chain_id, forge::MODULE, &[name])
        .unwrap_or_else(|| format!("duck://<network>/forge/{name}"))
}

/// The picked ref as a dropdown at the head of the tab bar: branches, the
/// default first, then tags. The pick steers every tab.
fn ref_picker(forge: &Forge, cx: &mut Context<Forge>, theme: &Theme) -> AnyElement {
    let head = forge.head_name();
    let default = forge.default_head();
    let open = forge.menu == Some(Menu::Ref);
    let mut items = Vec::new();
    if open {
        let mut ordered: Vec<&forge::RefInfo> = forge.refs().unwrap_or_default().iter().collect();
        ordered.sort_by_key(|info| (info.name != default, info.name.clone()));
        let (tags, branches): (Vec<_>, Vec<_>) = ordered
            .into_iter()
            .partition(|info| info.name.starts_with(b"refs/tags/"));
        for (label, group) in [("Branches", branches), ("Tags", tags)] {
            if group.is_empty() {
                continue;
            }
            items.push(components::menu_label(label, theme));
            for info in group {
                let name = info.name.clone();
                let pick = cx.listener({
                    let name = name.clone();
                    move |forge, _: &ClickEvent, _, cx| forge.pick_ref(name.clone(), cx)
                });
                items.push(components::menu_item(
                    id(format!("forge-ref-{}", components::path_text(&name))),
                    components::ref_label(&name),
                    (name == default).then_some("default"),
                    name == head,
                    theme,
                    pick,
                ));
            }
        }
        if items.is_empty() {
            items.push(components::menu_label("Reading refs…", theme));
        }
    }
    let label = match forge.refs() {
        Some([]) => "no refs".to_owned(),
        _ => components::ref_label(&head),
    };
    components::dropdown(
        "forge-ref-picker",
        label,
        open,
        items,
        theme,
        cx.listener(|forge, _: &ClickEvent, _, cx| forge.open_menu(Some(Menu::Ref), cx)),
        cx.listener(|forge, _: &MouseDownEvent, _, cx| forge.open_menu(None, cx)),
    )
}

/// On a narrow window the rail and a change's details fold into toggles.
fn narrow_bar(forge: &Forge, cx: &mut Context<Forge>, theme: &Theme) -> AnyElement {
    let tree = cx.listener(|forge, _: &ClickEvent, _, cx| {
        forge.layout.tree_open = !forge.layout.tree_open;
        cx.notify();
    });
    let details = cx.listener(|forge, _: &ClickEvent, _, cx| {
        forge.layout.dock_open = !forge.layout.dock_open;
        cx.notify();
    });
    div()
        .id(id("forge-narrow-bar"))
        .flex()
        .gap_1()
        .px_2()
        .py_1()
        .border_b_1()
        .border_color(theme.border)
        .child(
            button(id("forge-toggle-rail"), "Repositories", theme, tree)
                .kind(design::Kind::Outline)
                .selected(forge.layout.tree_open),
        )
        .when(forge.nav().change.is_some(), |bar| {
            bar.child(
                button(id("forge-toggle-dock"), "Details", theme, details)
                    .kind(design::Kind::Outline)
                    .selected(forge.layout.dock_open),
            )
        })
        .into_any_element()
}

/// The one docked panel a screen shows at a time.
fn panel(forge: &Forge, dock: Dock, cx: &mut Context<Forge>, theme: &Theme) -> AnyElement {
    let close = cx.listener(move |forge, _: &ClickEvent, _, cx| forge.toggle_dock(dock, cx));
    let body: AnyElement = match dock {
        Dock::About => about(forge, theme),
    };
    div()
        .id(id("forge-dock"))
        .w(DOCK_W)
        .flex()
        .flex_col()
        .min_h(px(0.))
        .border_l_1()
        .border_color(theme.border)
        .bg(theme.surface)
        .child(
            div()
                .id(id("forge-dock-header"))
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .child(heading(id("forge-dock-title"), dock.label(), 2, theme))
                .child(div().flex_1())
                .child(button(id("forge-dock-close"), "Close", theme, close)),
        )
        .child(
            div()
                .id(id("forge-dock-body"))
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scroll()
                .px_3()
                .pb_3()
                .child(body),
        )
        .into_any_element()
}

fn about(forge: &Forge, theme: &Theme) -> AnyElement {
    let Some((info, bounds, writers)) = forge.repo() else {
        return quiet("Reading this repository…", theme);
    };
    let mut column = div()
        .id(id("forge-about"))
        .flex()
        .flex_col()
        .gap_2()
        .child(fact(
            "Default head",
            components::ref_label(&info.repo.settings.head),
            theme,
        ))
        .child(fact(
            "Hash",
            match info.repo.hash {
                abi::HashKind::Sha1 => "sha1",
                abi::HashKind::Sha256 => "sha256",
            },
            theme,
        ))
        .child(fact("Owner", forge.principal_name(&info.repo.owner), theme))
        .child(fact("Page size", bounds.page_size.to_string(), theme))
        .child(fact(
            "Inline blob bound",
            format!("{} bytes", bounds.blob_bytes),
            theme,
        ))
        .child(heading(id("forge-about-access"), "Access", 3, theme));
    if writers.items.is_empty() {
        column = column.child(quiet("Only the owner writes here.", theme));
    }
    for key in &writers.items {
        column = column.child(quiet(forge.principal_name(key), theme));
    }
    column.into_any_element()
}

pub(crate) fn fact(label: &str, value: impl Into<String>, theme: &Theme) -> AnyElement {
    div()
        .flex()
        .gap_2()
        .text_size(design::text::SECONDARY)
        .child(
            div()
                .w(FACT_LABEL_W)
                .text_color(theme.muted)
                .child(label.to_owned()),
        )
        .child(div().flex_1().child(value.into()))
        .into_any_element()
}

/// The optimistic rows of a scope: what the reader issued, still in flight.
pub(crate) fn pending(forge: &Forge, scope: &str, theme: &Theme) -> AnyElement {
    let ops = forge.pending_in(scope);
    if ops.is_empty() {
        return div().into_any_element();
    }
    let mut column = div()
        .id(id(format!("forge-pending-{scope}")))
        .flex()
        .flex_col()
        .gap_1()
        .px_2()
        .py_1();
    for op in ops {
        let (tone, status) = match &op.progress {
            Progress::Submitting => (theme.surface_raised, "Submitting…".to_owned()),
            Progress::Accepted => (
                theme.surface_raised,
                "Waiting for the next block…".to_owned(),
            ),
            Progress::Refused(sentence) => (theme.danger_soft, format!("Refused: {sentence}")),
        };
        column = column.child(
            div()
                .id(id(format!("forge-pending-{}", op.id)))
                .flex()
                .items_center()
                .gap_2()
                .p_1()
                .bg(tone)
                .text_size(design::text::SECONDARY)
                .child(op.label.clone())
                .child(quiet(status, theme)),
        );
    }
    column.into_any_element()
}

/// The `Reply` a read landed, or the loading/refused state in its place.
pub(crate) fn staged<'a>(
    forge: &'a Forge,
    query: &forge::Query,
    element_id: &str,
    loading_text: &str,
    cx: &mut Context<Forge>,
    theme: &Theme,
) -> Result<&'a Reply, AnyElement> {
    match forge.stage(query) {
        crate::Stage::Ready(reply) => Ok(reply),
        crate::Stage::Loading => Err(components::loading(
            id(format!("{element_id}-loading")),
            loading_text,
            theme,
        )),
        crate::Stage::Failed(refusal) => {
            let query = query.clone();
            let retry =
                cx.listener(move |forge, _: &ClickEvent, _, cx| forge.retry(query.clone(), cx));
            Err(ducktape_view_guest::design::refused(
                element_id,
                refusal.message.clone(),
                theme,
                retry,
            )
            .m_2()
            .into_any_element())
        }
    }
}

/// A scrolling content column, the shape every screen body uses.
pub(crate) fn scroller(name: &str) -> Stateful<Div> {
    div()
        .id(id(name.to_owned()))
        .flex_1()
        .min_h(px(0.))
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap_1()
        .p_2()
}

pub(crate) fn bold(text: impl Into<String>) -> AnyElement {
    div()
        .font_weight(FontWeight::MEDIUM)
        .child(text.into())
        .into_any_element()
}
