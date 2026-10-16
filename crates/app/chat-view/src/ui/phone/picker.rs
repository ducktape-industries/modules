//! The full reaction picker (Expo `src/chat/Actions.tsx` `Picker`, design
//! §4.5, board 42): a sheet with the search field, Frequently used, the
//! eight category tabs and the chosen one's grid; or, while the field
//! holds a search, its results. A pick reacts and closes. Reads the open
//! menu, the picker's query and tab and the reader's recent reactions;
//! writes the query and the tab, and reacts.
use view::Div;
use view::prelude::*;

use crate::words;
use crate::{Chat, Mode, emoji};
use view::FontWeight;
use view::design::Ladder;

/// One emoji cell, each way (mobile `src/chat/Actions.tsx:24`), the emoji
/// 24 on a 30 line (`:33`).
pub(super) const CELL: Pixels = px(44.);
const CELL_EMOJI: design::Type = design::Type::lined(24., 30.);
/// Where the sheet rests (mobile `src/chat/Actions.tsx:153`).
const DETENTS: [f32; 2] = [0.6, 1.0];
/// The field's magnifier (mobile `src/chat/Actions.tsx:157`).
const SEARCH_ICON: Pixels = px(20.);
/// A category tab: 36 wide, the glyph 20 on a 26 line, a 2 rule under the
/// chosen one (mobile `src/chat/Actions.tsx:195` and `:197`).
const TAB_WIDTH: Pixels = px(36.);
const TAB_GLYPH: design::Type = design::Type::lined(20., 26.);

/// One emoji cell as a sheet draws it: `mine`, the reader's own reaction
/// on the message, wears the accent.
pub(super) struct Cell {
    pub id: ElementId,
    pub emoji: String,
    pub mine: bool,
}

/// The picker, while a message's is open.
pub(super) fn sheet(
    chat: &Chat,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> Option<AnyElement> {
    let menu = chat
        .menu
        .as_ref()
        .filter(|menu| menu.mode == Mode::Reactions)?;
    let close = cx.listener(|chat, _: &(), _window, cx| {
        chat.close_menu();
        cx.notify();
    });
    let query = chat.picker.query.text();
    let body = match query.trim().is_empty() {
        true => browse(chat, menu.seq, cx, theme, ladder),
        false => results(chat, &query, menu.seq, cx, theme, ladder),
    };
    let picker = div()
        .id("chat-reaction-picker")
        .size_full()
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .child(
            div()
                .px(design::space::BLOCK)
                .pb(design::space::SM)
                .child(field(chat, theme, ladder)),
        )
        .children(body);
    Some(
        design::sheet("chat-picker-sheet", words::ADD_REACTION, ladder, theme)
            .detents(DETENTS)
            .on_dismiss(close)
            .child(picker)
            .into_any_element(),
    )
}

/// The search field: a hairline box, its magnifier, the words.
fn field(chat: &Chat, theme: &Theme, ladder: &Ladder) -> Div {
    let input = Input::new(
        "chat-reaction-search",
        &chat.picker.query,
        words::SEARCH_EMOJI,
    )
    .autocorrect(false)
    .autocapitalize(Autocapitalize::None)
    .flex_1()
    .min_w(px(0.))
    .text_type(ladder.body)
    .text_color(theme.foreground)
    .placeholder(words::SEARCH_EMOJI);
    div()
        .min_h(ladder.control)
        .flex()
        .items_center()
        .gap(design::space::SM)
        .px(design::space::LG)
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.background)
        .child(design::icon("search", SEARCH_ICON))
        .child(input)
}

/// No search: Frequently used, the tabs, the chosen tab's grid.
fn browse(
    chat: &Chat,
    seq: u64,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> Vec<AnyElement> {
    let tab = chat.picker.tab.min(emoji::CATEGORIES.len() - 1);
    let category = &emoji::CATEGORIES[tab];
    let frequent = emoji::frequent(&chat.recent_emoji);
    let chosen = category
        .emoji
        .iter()
        .take(emoji::PER_TAB)
        .map(|(emoji, _)| (*emoji).to_owned());
    vec![
        caption(words::FREQUENTLY_USED, theme, ladder),
        grid(chat, "frequent", frequent, seq, cx, theme).into_any_element(),
        tabs(tab, cx, theme, ladder),
        caption(category.name, theme, ladder),
        grid(chat, category.name, chosen.collect(), seq, cx, theme)
            .pb(design::space::XL)
            .into_any_element(),
    ]
}

/// A search: how many matched, then they.
fn results(
    chat: &Chat,
    query: &str,
    seq: u64,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> Vec<AnyElement> {
    let found: Vec<String> = emoji::search(query)
        .into_iter()
        .map(str::to_owned)
        .collect();
    let count = match found.len() {
        0 => words::NO_EMOJI_MATCH.to_owned(),
        found => format!("Results · {found}"),
    };
    vec![
        caption(&count, theme, ladder),
        grid(chat, "results", found, seq, cx, theme).into_any_element(),
    ]
}

/// A section's label: caption medium, muted, 16 in, 8 over and under.
fn caption(text: &str, theme: &Theme, ladder: &Ladder) -> AnyElement {
    div()
        .px(design::space::BLOCK)
        .py(design::space::SM)
        .text_type(ladder.caption)
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.muted)
        .child(text.to_owned())
        .into_any_element()
}

/// Cells in rows, 16 in from each side, the room left over shared between
/// them. Expo's gap makes a row exactly the width, which Yoga's float sum
/// overflows, so the Expo app draws one cell fewer a row (7 at 402 and at
/// 411 wide); so does this.
fn grid(
    chat: &Chat,
    section: &str,
    emoji: Vec<String>,
    seq: u64,
    cx: &mut Context<Chat>,
    theme: &Theme,
) -> Div {
    let room = chat.layout.viewport.0 - 2. * f32::from(design::space::BLOCK);
    let fits = ((room / f32::from(CELL)).floor() as usize).max(2);
    let gap = px((room - fits as f32 * f32::from(CELL)) / (fits - 1) as f32);
    let cells: Vec<AnyElement> = emoji
        .into_iter()
        .map(|emoji| {
            let pick = emoji.clone();
            // a pick the reader already reacted with only closes
            let press = cx.listener(move |chat, _: &ClickEvent, _window, cx| {
                cx.notify();
                match reacted(chat, seq, &pick) {
                    true => chat.close_menu(),
                    false => chat.react(seq, pick.clone(), true, cx),
                }
            });
            let id = format!("chat-reaction-{section}-{emoji}").into();
            cell(
                Cell {
                    id,
                    emoji,
                    mine: false,
                },
                press,
                theme,
            )
        })
        .collect();
    let mut rows = Vec::new();
    let mut cells = cells.into_iter().peekable();
    while cells.peek().is_some() {
        let row: Vec<AnyElement> = cells.by_ref().take(fits - 1).collect();
        let id = format!("chat-reaction-{section}-row-{}", rows.len());
        rows.push(div().id(id).flex().gap(gap).children(row));
    }
    div()
        .flex()
        .flex_col()
        .gap(design::space::XXS)
        .px(design::space::BLOCK)
        .children(rows)
}

/// Whether the reader reacted to the message at `seq` with `emoji`.
fn reacted(chat: &Chat, seq: u64, emoji: &str) -> bool {
    let Some(menu) = &chat.menu else {
        return false;
    };
    chat.messages(menu.pane)
        .iter()
        .any(|message| message.seq == seq && message.reacted(emoji))
}

/// The category tabs, spread across the sheet; the chosen one's glyph
/// over a 2 rule in the foreground.
fn tabs(chosen: usize, cx: &mut Context<Chat>, theme: &Theme, ladder: &Ladder) -> AnyElement {
    let tabs: Vec<AnyElement> = emoji::CATEGORIES
        .iter()
        .enumerate()
        .map(|(index, category)| {
            let open = cx.listener(move |chat, _: &ClickEvent, _window, cx| {
                chat.picker.tab = index;
                cx.notify();
            });
            div()
                .id(format!("chat-reaction-tab-{}", category.name))
                .w(TAB_WIDTH)
                .h(ladder.hit)
                .flex()
                .items_center()
                .justify_center()
                .border_b_2()
                .border_color(match index == chosen {
                    true => theme.foreground,
                    false => Hsla::transparent_black(),
                })
                .text_type(TAB_GLYPH)
                .role(Role::Tab)
                .aria_label(category.name)
                .aria_selected(index == chosen)
                .focusable()
                .on_click(open)
                .child(category.glyph)
                .into_any_element()
        })
        .collect();
    div()
        .id("chat-reaction-tabs")
        .flex()
        .justify_between()
        .px(design::space::BLOCK)
        .bg(theme.background)
        .border_b_1()
        .border_color(theme.border)
        .role(Role::TabList)
        .aria_label(words::EMOJI_CATEGORIES)
        .children(tabs)
        .into_any_element()
}

/// One 44 emoji cell (the Expo `Cell`); the reader's own reaction wears
/// the accent edge and ground.
pub(super) fn cell(
    cell: Cell,
    press: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    theme: &Theme,
) -> AnyElement {
    div()
        .id(cell.id)
        .size(CELL)
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(match cell.mine {
            true => theme.accent,
            false => Hsla::transparent_black(),
        })
        .when(cell.mine, |cell| cell.bg(theme.accent_soft))
        .text_type(CELL_EMOJI)
        .role(Role::Button)
        .aria_label(words::react_with(&cell.emoji))
        .focusable()
        .on_click(press)
        .child(cell.emoji)
        .into_any_element()
}
