//! Expo `app/(tabs)/explorer/index.tsx`, board `09-explorer`: Explorer's
//! root. The title row says "Explorer" over the network and the reader,
//! with Search (and More on the Programs page); Search turns the row into
//! the field. Under it the five pages on a strip, the notice, and the
//! page's list ([`pages::body`]). Reads the session, the root page and the
//! search field; writes the root page (`go`), the search (`search`) and
//! whether the field is open.
use super::*;

/// The page strip's band (Expo `src/design/theme.ts:81` `size.strip`).
const STRIP_H: Pixels = px(44.);
/// The chosen page's underline (Expo `src/kit/Choice.tsx:138`).
const UNDERLINE_H: Pixels = px(2.);

/// The five pages, in the strip's order: each one's key, label and list.
const PAGES: [(&str, &str, Route); 5] = [
    ("overview", "Overview", Route::Overview),
    ("blocks", "Blocks", Route::Blocks),
    ("transactions", "Transactions", Route::Transactions(None)),
    ("accounts", "Accounts", Route::Accounts),
    ("programs", "Programs", Route::Programs),
];

pub(super) fn root(view: &Explorer, cx: Cx, k: &Kit) -> AnyElement {
    let strip = strip(view, cx, k);
    let note = note(view, cx, k);
    let page = pages::body(view, cx, k);
    match view.searching {
        true => searching(view, cx, k)
            .child(strip)
            .children(note)
            .child(page)
            .into_any_element(),
        false => titled(view, cx, k)
            .child(strip)
            .children(note)
            .child(page)
            .into_any_element(),
    }
}

/// The network the session is on, by its label (`<label>#<salt>`), and the
/// reader (Expo's root caption, `testkit · Dev`).
fn network_and_me(view: &Explorer) -> Option<String> {
    let chain = view.session_chain.as_str();
    let network = chain.split_once('#').map_or(chain, |(label, _)| label);
    if network.is_empty() {
        return None;
    }
    let me = view
        .acting
        .account
        .and_then(|number| view.account(number))
        .map(|account| account.card.name.clone());
    Some(match me {
        Some(me) => format!("{network} · {me}"),
        None => network.to_owned(),
    })
}

/// The title row: "Explorer", its caption, Search, and on the Programs
/// page More ([`programs::more`]).
fn titled(view: &Explorer, cx: Cx, k: &Kit) -> design::Page {
    let open = cx.listener(|view: &mut Explorer, _: &ClickEvent, window, cx| {
        view.searching = true;
        window.focus("explorer-search");
        cx.notify();
    });
    let no_back = None::<fn(&ClickEvent, &mut Window, &mut App)>;
    let page = design::page("explorer-root", "Explorer", no_back, &k.ladder, &k.theme).trailing(
        design::icon_button(
            "explorer-search-open",
            design::icon("search", k.ladder.icon),
            "Search",
            &k.ladder,
            &k.theme,
            open,
        ),
    );
    let page = match network_and_me(view) {
        Some(caption) => page.caption(caption),
        None => page,
    };
    match view.root {
        Route::Programs => programs::more(view, page, cx, k),
        _ => page,
    }
}

/// The root while searching (Expo `Header center`): the field in the
/// title's place and Close search, the page under it.
fn searching(view: &Explorer, cx: Cx, k: &Kit) -> Stateful<Div> {
    let close = cx.listener(|view: &mut Explorer, _: &ClickEvent, _, cx| {
        view.searching = false;
        view.note = None;
        view.search.reset("");
        cx.notify();
    });
    let row = div()
        .id("explorer-root-title")
        .flex_none()
        .min_h(k.ladder.header)
        .flex()
        .items_center()
        .gap(space::XXS)
        .px(space::SM)
        .pr(CAPSULE_CLEARANCE)
        .bg(k.theme.background)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .px(space::XS)
                .child(field(view, cx, k)),
        )
        .child(design::icon_button(
            "explorer-search-close",
            design::icon("x", k.ladder.icon),
            "Close search",
            &k.ladder,
            &k.theme,
            close,
        ));
    div()
        .id("explorer-root")
        .size_full()
        .flex()
        .flex_col()
        .child(row)
}

/// The search field (Expo `Field` with a magnifier lead): a height, a hash,
/// `#3` or a name; no capitals, no corrections, Return searches.
fn field(view: &Explorer, cx: Cx, k: &Kit) -> Div {
    let submit = cx.listener(|view: &mut Explorer, _: &(), _, cx| view.search(cx));
    let input = Input::new("explorer-search", &view.search, "Search the chain")
        .autocorrect(false)
        .autocapitalize(Autocapitalize::None)
        .virtual_keyboard(VirtualKeyboard::Search)
        .input_action(TextInputAction::Search)
        .flex_1()
        .min_w(px(0.))
        .text_type(k.ladder.body)
        .placeholder("Search by height, hash, account or program")
        .on_submit(submit);
    div()
        .min_h(k.ladder.control)
        .flex()
        .items_center()
        .gap(space::SM)
        .px(space::LG)
        .border_1()
        .border_color(k.theme.border_strong)
        .bg(k.theme.background)
        .child(design::icon("search", k.ladder.icon_row).text_color(k.theme.muted))
        .child(input)
}

/// The five pages on a strip that scrolls sideways (Expo `Strip`): one Tab
/// stop, ← → move through the pages, a press opens a page's list.
fn strip(view: &Explorer, cx: Cx, k: &Kit) -> Stateful<Div> {
    let shown = view.root.tab();
    let routes = || PAGES.map(|(_, _, route)| route);
    let (moved, pressed) = (routes(), routes());
    let pages: Vec<Stateful<Div>> = PAGES
        .into_iter()
        .map(|(key, label, route)| page_tab(key, label, route, shown, cx, k))
        .collect();
    design::composite("explorer-tabs", Role::TabList, "Pages")
        .orientation(design::Orientation::Horizontal)
        .wrap()
        .active(shown, PAGES.len())
        .on_move(
            cx.processor(move |view: &mut Explorer, index: usize, _, cx| {
                view.go(moved[index].clone(), cx)
            }),
        )
        .on_press(
            cx.processor(move |view: &mut Explorer, index: usize, _, cx| {
                view.go(pressed[index].clone(), cx)
            }),
        )
        .build()
        .flex_none()
        .h(STRIP_H)
        .flex()
        .px(space::SM)
        .overflow_x_scroll()
        .border_b_1()
        .border_color(k.theme.border)
        .children(pages)
}

/// One page on the strip: its label in small, the chosen one foreground
/// and medium over a 2 px underline, the rest muted.
fn page_tab(
    key: &str,
    label: &'static str,
    route: Route,
    shown: usize,
    cx: Cx,
    k: &Kit,
) -> Stateful<Div> {
    let chosen = shown == route.tab();
    let go =
        cx.listener(move |view: &mut Explorer, _: &ClickEvent, _, cx| view.go(route.clone(), cx));
    let underline = div()
        .absolute()
        .bottom_0()
        .left(space::MD)
        .right(space::MD)
        .h(UNDERLINE_H)
        .bg(k.theme.foreground);
    let tab = div()
        .id(SharedString::from(format!("explorer-tab-{key}")))
        .relative()
        .flex_none()
        .h_full()
        .flex()
        .items_center()
        .px(space::MD)
        .text_type(k.ladder.small)
        .text_color(match chosen {
            true => k.theme.foreground,
            false => k.theme.muted,
        })
        .when(chosen, |tab| tab.font_weight(FontWeight::MEDIUM))
        .whitespace_nowrap()
        .aria_selected(chosen)
        .on_click(go)
        .child(label)
        .when(chosen, |tab| tab.child(underline));
    design::item(tab, Role::Tab, chosen)
}
