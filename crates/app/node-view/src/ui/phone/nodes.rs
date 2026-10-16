//! The Nodes list, Expo `app/(tabs)/nodes/index.tsx`: the network's name
//! and this node's pulse in the title row; the node's numbers (its height
//! and block time as two facts, the epoch under them); every member as a
//! row, validators then residents, each opening its page; and for a reader
//! who votes, the motions that wait, the scheduled changes, and the door's
//! `⋯`, which raises the door as a sheet.
//!
//! Reads the node's status, the members, the network's votes as this node
//! heard them, and the vote (`Nodes::voting`). Writes `Nodes::open` (a row
//! opens its member's page), `Nodes::door` (the `⋯` and the sheet), the
//! read a Retry asks again, and a vote through `Nodes::press`.
use abi::hex;
use programs::Pending as Scheduled;
use valset::{Motion, Pending};
use view::design::{Page, space};
use view::prelude::*;
use view::{Div, FontWeight, Stateful};

use super::LADDER;
use super::parts::{empty, last_answer, outcome, press, quiet, refused, scroller, small_button};
use crate::queries::{Motions, Node, Schedule};
use crate::row::Status;
use crate::words::{self, EpochPlace};
use crate::{Nodes, voting};

// SDK gap: the kit's tall row (`ROW_TALL`, `row_lines`), swap when
// feat/phone-kit lands

/// A list row with a caption under its title (Expo `src/kit/Row.tsx:44`
/// `size.rowTall`).
const ROW_TALL: Pixels = px(design::height::touch::ROW_TALL as f32);
/// The gap between a row's title and its caption (Expo `Row.tsx:58`).
const ROW_LINES_GAP: Pixels = px(1.);
/// A member's dot (Expo `index.tsx:114`).
const DOT: Pixels = px(8.);
/// How tall the epoch's bar is (Expo `index.tsx:76`).
const EPOCH_BAR: Pixels = px(6.);

/// The list page: its title row, the vote's outcome strip, and what
/// scrolls under them.
pub(super) fn render(view: &Nodes, cx: &mut Context<Nodes>, theme: &Theme) -> Page {
    // SDK gap: a root page without a typed `None` Back (`Page::root`),
    // swap when #406 lands
    let page = design::page(
        "nodes-page",
        title(view),
        None::<fn(&ClickEvent, &mut Window, &mut App)>,
        &LADDER,
        theme,
    );
    let page = match pulse(view) {
        Some(pulse) => page.caption(pulse),
        None => page,
    };
    let page = match view.voting.votes() {
        true => page.trailing(door_button(cx, theme)),
        false => page,
    };
    let body = div()
        .child(numbers(view, cx, theme))
        .child(members(view, cx, theme))
        .children(vote(view, cx, theme));
    page.children(outcome(view, cx, theme))
        .child(scroller("nodes-scroll", body))
}

/// The network's name, once the node says which it is on; Nodes before.
fn title(view: &Nodes) -> String {
    view.status
        .ready()
        .map_or(words::NODES, |status| words::network(&status.chain_id))
        .to_owned()
}

/// Whether this node answers, and which node it is.
// SDK gap: a caption that takes a node (Expo draws the word in its tone
// and the key in the data face); the SDK builds it later
fn pulse(view: &Nodes) -> Option<String> {
    let status = view.status.ready()?;
    Some(format!(
        "● {} · {} {}",
        words::pulse(view.answering()),
        words::THIS_NODE,
        design::short_hex(&hex(&status.identity))
    ))
}

/// The `⋯` that raises the door's sheet.
fn door_button(cx: &mut Context<Nodes>, theme: &Theme) -> impl IntoElement {
    let open = cx.listener(|view: &mut Nodes, _: &ClickEvent, _, cx| {
        view.door = true;
        cx.notify();
    });
    design::icon_button(
        "nodes-door",
        design::icon("ellipsis", LADDER.icon),
        "The door",
        &LADDER,
        theme,
        open,
    )
}

/// The node's numbers, or why they are not here: refused with Retry, or
/// still being read.
fn numbers(view: &Nodes, cx: &mut Context<Nodes>, theme: &Theme) -> AnyElement {
    if let Some(refusal) = view.status.failed() {
        let retry = cx.listener(|view: &mut Nodes, _: &ClickEvent, _, cx| view.retry_status(cx));
        return refused("nodes-status", &refusal.message, theme, retry).into_any_element();
    }
    let Some(status) = view.status.ready() else {
        return quiet("nodes-status-loading", words::READING_STATUS, theme).into_any_element();
    };
    let last = format!("last block {}", words::block_age(view.ticks - view.moved));
    let height = Fact {
        key: "height",
        label: words::HEIGHT,
        value: design::grouped(status.height),
        mono: true,
        caption: Some(last),
    };
    let block_time = Fact {
        key: "block-time",
        label: words::BLOCK_TIME,
        value: format!("{} ms", design::grouped(status.block_time_ms)),
        mono: false,
        caption: None,
    };
    div()
        .child(facts([height, block_time], theme))
        .children(EpochPlace::of(status).map(|place| epoch(&place, theme)))
        .into_any_element()
}

// SDK gap: the kit's `Facts` (`Fact`, `facts`, `fact_cell`); the SDK builds it later

/// One of the kit's `Facts` (Expo `src/kit/Facts.tsx`): an eyebrow over a
/// value over a caption.
struct Fact {
    /// names the cell: `nodes-status-{key}`
    key: &'static str,
    label: &'static str,
    value: String,
    /// the value in the data face
    mono: bool,
    caption: Option<String>,
}

/// The kit's `Facts`: two to a row, each half the width, a hairline under
/// every one and between the two of a row.
fn facts(facts: impl IntoIterator<Item = Fact>, theme: &Theme) -> Stateful<Div> {
    div().id("nodes-status").flex().flex_wrap().children(
        facts
            .into_iter()
            .enumerate()
            .map(|(index, fact)| fact_cell(fact, index, theme)),
    )
}

/// The `index`th fact's cell: the eyebrow (the data face, medium, muted,
/// in capitals), the value in heading type, the caption muted. The first
/// of a row draws the hairline between the two (Expo `Facts.tsx:24`).
fn fact_cell(fact: Fact, index: usize, theme: &Theme) -> Stateful<Div> {
    div()
        .id(ElementId::Name(format!("nodes-status-{}", fact.key).into()))
        .w_1_2()
        .flex()
        .flex_col()
        .gap(space::HAIR)
        .py(space::LG)
        .px(space::BLOCK)
        .border_b_1()
        .when(index.is_multiple_of(2), |cell| cell.border_r_1())
        .border_color(theme.border)
        .child(
            div()
                .mb(space::XXS)
                .font_family(design::fonts::FAMILY_MONO)
                .text_type(LADDER.tiny)
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.muted)
                .child(fact.label.to_uppercase()),
        )
        .child(
            div()
                .text_type(LADDER.heading)
                .font_weight(FontWeight::MEDIUM)
                .when(fact.mono, |value| {
                    value.font_family(design::fonts::FAMILY_MONO)
                })
                .child(fact.value),
        )
        .children(fact.caption.map(|caption| {
            div()
                .text_type(LADDER.caption)
                .text_color(theme.muted)
                .child(caption)
        }))
}

/// `Epoch 131 · 29 / 64` over a bar the width of the screen, and when the
/// next one starts.
fn epoch(place: &EpochPlace, theme: &Theme) -> Stateful<Div> {
    div()
        .id("nodes-status-epoch")
        .flex()
        .flex_col()
        .gap(space::XS)
        .px(space::BLOCK)
        .py(space::LG)
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .text_type(LADDER.secondary)
                .text_color(theme.muted)
                .child(place.place()),
        )
        .child(epoch_bar(place, theme))
        .child(
            div()
                .id("nodes-status-next-epoch")
                .text_type(LADDER.caption)
                .text_color(theme.muted)
                .child(place.next()),
        )
}

/// The epoch's bar: ink for the blocks gone, the hairline's grey for those
/// to come, sharing its width as the epoch does.
fn epoch_bar(place: &EpochPlace, theme: &Theme) -> Div {
    let part = |blocks: u64, fill: Hsla| {
        let mut part = div().h_full().flex_basis(px(0.)).bg(fill);
        part.style().flex_grow = Some(blocks as f32);
        part
    };
    div()
        .flex()
        .h(EPOCH_BAR)
        .bg(theme.border)
        .child(part(place.into, theme.foreground))
        .child(part(place.length - place.into, theme.border))
}

/// The members, or why they are not here: still being read, refused with
/// Retry, none at all, or waiting on the node's status their rows read
/// (the numbers above say why).
fn members(view: &Nodes, cx: &mut Context<Nodes>, theme: &Theme) -> AnyElement {
    if let Some(refusal) = view.nodes.failed() {
        let retry = cx.listener(|view: &mut Nodes, _: &ClickEvent, _, cx| view.read(cx));
        return refused("nodes", &refusal.message, theme, retry).into_any_element();
    }
    let Some(nodes) = view.nodes.ready() else {
        return quiet("nodes-loading", words::READING_MEMBERS, theme).into_any_element();
    };
    if nodes.is_empty() {
        let (title, why) = (words::NO_MEMBERS, words::NO_MEMBERS_WHY);
        return empty("nodes-empty", "server", title, why, theme).into_any_element();
    }
    match view.status.ready() {
        Some(_) => roster(view, nodes, cx, theme).into_any_element(),
        None => div().into_any_element(),
    }
}

/// The members once read: why no validator's state shows, how long the
/// node has been silent, the validators then the residents under their
/// counts, and what a validator's cells mean.
fn roster(view: &Nodes, nodes: &[Node], cx: &Context<Nodes>, theme: &Theme) -> Stateful<Div> {
    let rows = |validators: bool| -> Vec<Stateful<Div>> {
        nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.validator == validators)
            .map(|(index, node)| member_row(view, index, node, cx, theme))
            .collect()
    };
    let (seats, residents) = (rows(true), rows(false));
    let unheard = view.unheard().map(words::unheard);
    let footnote = view.heard().map(|_| words::footnote());
    div()
        .id("nodes-members")
        .children(unheard.map(|why| quiet("nodes-unheard", why, theme)))
        .children(last_answer(view, "nodes-last-answer", theme))
        .child(count_head(
            "nodes-validators",
            words::VALIDATORS,
            seats.len(),
            theme,
        ))
        .children(seats)
        .child(count_head(
            "nodes-residents",
            words::RESIDENTS,
            residents.len(),
            theme,
        ))
        .children(residents)
        .children(footnote.map(|footnote| quiet("nodes-footnote", footnote, theme)))
}

/// A section head with how many rows it heads at its right.
fn count_head(
    id: &'static str,
    label: &'static str,
    count: usize,
    theme: &Theme,
) -> impl IntoElement {
    design::section_head(id, label, Some(count), &LADDER, theme)
}

/// A member as the kit's tall `Row`: its dot, its key (and who operates
/// it) over its address, a validator's status word, a chevron. The row
/// opens the member's page.
fn member_row(
    view: &Nodes,
    index: usize,
    node: &Node,
    cx: &Context<Nodes>,
    theme: &Theme,
) -> Stateful<Div> {
    let status = view.cells(node).status;
    let (dot, tone) = tones(node, &status, view.answering(), theme);
    let key = node.key.clone();
    let open = cx.listener(move |view: &mut Nodes, _: &ClickEvent, _, cx| {
        view.open = Some(key.clone());
        cx.notify();
    });
    design::row(
        ElementId::Name(format!("nodes-member-{index}").into()),
        &LADDER,
        theme,
    )
    .min_h(ROW_TALL)
    .role(Role::Button)
    .focusable()
    .on_click(open)
    .child(design::lead(div().size(DOT).rounded_full().bg(dot)))
    .child(row_lines(
        member_title(node, theme),
        member_address(view, node, theme),
    ))
    .children(node.validator.then(|| {
        div()
            .flex_none()
            .text_type(LADDER.secondary)
            .text_color(tone)
            .child(status.word())
    }))
    // SDK gap: the row's chevron (`design::chevron`), swap when feat/phone-kit lands
    .child(design::icon("chevron-right", LADDER.icon_row).text_color(theme.faint))
}

/// A member's tones: its dot's, and its status word's. A resident's state
/// is not reported; a silent node's are all muted.
fn tones(node: &Node, status: &Status, live: bool, theme: &Theme) -> (Hsla, Hsla) {
    if !node.validator {
        return (theme.faint, theme.muted);
    }
    if !live {
        return (theme.muted, theme.muted);
    }
    match status {
        Status::InSync => (theme.success, theme.success),
        Status::Behind(_) => (theme.warning, theme.warning),
        Status::Quiet { .. } => (theme.danger, theme.danger),
        Status::Blank => (theme.faint, theme.muted),
    }
}

/// A member's title: its short key in the data face, then who operates
/// it, muted at secondary size, cut short where it runs out of room.
fn member_title(node: &Node, theme: &Theme) -> Div {
    div()
        .flex()
        .items_baseline()
        .min_w(px(0.))
        .whitespace_nowrap()
        .overflow_hidden()
        .text_type(LADDER.body)
        // SDK gap: `design::mono` sized by the ladder
        // (`design::mono(text, ladder)`), swap when feat/phone-kit lands
        .child(
            design::mono(design::short_hex(&hex(&node.key)))
                .flex_none()
                .text_type(LADDER.body),
        )
        .children(node.operator.clone().map(|operator| {
            div()
                .min_w(px(0.))
                .truncate()
                .text_size(LADDER.secondary.size)
                .text_color(theme.muted)
                .child(format!(" · {operator}"))
        }))
}

/// A member's caption: its address in the data face, and `this node` on
/// the one this app talks to.
fn member_address(view: &Nodes, node: &Node, theme: &Theme) -> Div {
    // SDK gap: the row's caption (`design::row_caption`), swap when feat/phone-kit lands
    div()
        .font_family(design::fonts::FAMILY_MONO)
        .text_type(LADDER.caption)
        .text_color(theme.muted)
        .truncate()
        .child(view.address_line(node))
}

/// A row's title over its caption, taking the room the row leaves.
fn row_lines(title: impl IntoElement, caption: impl IntoElement) -> Div {
    div()
        .flex_1()
        .min_w(px(0.))
        .flex()
        .flex_col()
        .gap(ROW_LINES_GAP)
        .child(title)
        .child(caption)
}

/// The vote, for a reader who votes: the motions that wait, the scheduled
/// changes, and where a program is changed.
fn vote(view: &Nodes, cx: &mut Context<Nodes>, theme: &Theme) -> Option<Div> {
    if !view.voting.votes() {
        return None;
    }
    let motions_head = design::section_head("nodes-motions", words::MOTIONS, None, &LADDER, theme);
    let scheduled_head =
        design::section_head("nodes-scheduled", words::SCHEDULED, None, &LADDER, theme);
    let elsewhere = quiet("nodes-change-elsewhere", words::CHANGE_ELSEWHERE, theme);
    Some(
        div()
            .child(motions_head)
            .children(waiting(view, cx, theme))
            .child(scheduled_head)
            .children(scheduled(view, cx, theme))
            .child(elsewhere),
    )
}

/// The motions that wait for votes, or why there are none to show:
/// refused with Retry, still being read, or none waiting.
fn waiting(view: &Nodes, cx: &mut Context<Nodes>, theme: &Theme) -> Vec<AnyElement> {
    let voting = &view.voting;
    if let Some(refusal) = voting.motions.failed() {
        let retry = cx.listener(|view: &mut Nodes, _: &ClickEvent, _, cx| view.read_motions(cx));
        return vec![refused("nodes-motions", &refusal.message, theme, retry).into_any_element()];
    }
    let Some(motions) = voting.motions.ready() else {
        let reading = quiet("nodes-motions-loading", words::READING_MOTIONS, theme);
        return vec![reading.into_any_element()];
    };
    if motions.pending.is_empty() {
        let none = quiet("nodes-motions-none", words::NO_MOTION, theme);
        return vec![none.into_any_element()];
    }
    motions
        .pending
        .iter()
        .enumerate()
        .map(|(index, pending)| {
            motion_row(view, index, pending, motions, cx, theme).into_any_element()
        })
        .collect()
}

/// A motion that waits: its words, how many voted of how many it needs
/// and the reader's share, then Vote while the reader has a voice left on
/// it, else a `voted` badge.
fn motion_row(
    view: &Nodes,
    index: usize,
    pending: &Pending,
    motions: &Motions,
    cx: &Context<Nodes>,
    theme: &Theme,
) -> Stateful<Div> {
    let voting = &view.voting;
    let tail = match voting.uncast(&pending.motion) > 0 {
        true => small_button(
            ElementId::Name(format!("nodes-motion-vote-{index}").into()),
            words::VOTE,
            voting.pressable(),
            theme,
            press(cx, pending.motion.clone()),
        )
        .into_any_element(),
        false => design::badge(
            ElementId::Name(format!("nodes-motion-voted-{index}").into()),
            words::VOTED,
            &LADDER,
            theme.muted,
            theme.surface_raised,
        )
        .into_any_element(),
    };
    line(
        ElementId::Name(format!("nodes-motion-{index}").into()),
        voting.words(&pending.motion),
        words::pending_caption(voting, pending, motions.needed),
        tail,
        theme,
    )
}

/// The changes scheduled past the tip, or why there are none to show:
/// refused with Retry, still being read, or none scheduled. (Expo reads a
/// refused schedule as still being read, forever: `index.tsx:197-198`.)
fn scheduled(view: &Nodes, cx: &mut Context<Nodes>, theme: &Theme) -> Vec<AnyElement> {
    let voting = &view.voting;
    if let Some(refusal) = voting.scheduled.failed() {
        let retry = cx.listener(|view: &mut Nodes, _: &ClickEvent, _, cx| view.read_programs(cx));
        return vec![refused("nodes-scheduled", &refusal.message, theme, retry).into_any_element()];
    }
    let (Some(schedule), Some(ahead)) = (voting.scheduled.ready(), view.scheduled_ahead()) else {
        let reading = quiet("nodes-scheduled-loading", words::READING_SCHEDULE, theme);
        return vec![reading.into_any_element()];
    };
    if ahead.is_empty() {
        let none = quiet("nodes-scheduled-none", words::NO_CHANGE, theme);
        return vec![none.into_any_element()];
    }
    ahead
        .into_iter()
        .enumerate()
        .map(|(index, change)| {
            scheduled_row(view, index, change, schedule, cx, theme).into_any_element()
        })
        .collect()
}

/// A scheduled change: what it does at which height, who made it and how
/// long it can still be cancelled, then Propose cancel.
fn scheduled_row(
    view: &Nodes,
    index: usize,
    change: &Scheduled,
    schedule: &Schedule,
    cx: &Context<Nodes>,
    theme: &Theme,
) -> Stateful<Div> {
    let voting = &view.voting;
    let cancel = small_button(
        ElementId::Name(format!("nodes-scheduled-cancel-{index}").into()),
        words::PROPOSE_CANCEL,
        voting.pressable(),
        theme,
        press(cx, voting::cancel(change)),
    );
    line(
        ElementId::Name(format!("nodes-scheduled-{index}").into()),
        voting::scheduled_words(change.height, &change.change, voting.entries()),
        words::scheduled_caption(change, schedule, view.status.ready()),
        cancel.into_any_element(),
        theme,
    )
}

/// A row of the vote (the kit's `Row` with a caption): the words, its
/// caption under them, its press at the right.
fn line(
    id: ElementId,
    words: String,
    caption: String,
    tail: AnyElement,
    theme: &Theme,
) -> Stateful<Div> {
    let words = div().text_type(LADDER.body).truncate().child(words);
    // SDK gap: the row's caption (`design::row_caption`), swap when feat/phone-kit lands
    let caption = div()
        .text_type(LADDER.caption)
        .text_color(theme.muted)
        .truncate()
        .child(caption);
    design::row(id, &LADDER, theme)
        .min_h(ROW_TALL)
        .child(row_lines(words, caption))
        .child(div().flex().items_center().gap(space::SM).child(tail))
}

/// The door, as a sheet over the list (Expo `index.tsx:224-241`): its two
/// motions, each with what it means. Either closes the sheet and votes.
pub(super) fn door(view: &Nodes, cx: &mut Context<Nodes>, theme: &Theme) -> Option<AnyElement> {
    if !view.door {
        return None;
    }
    let close = cx.listener(|view: &mut Nodes, _: &(), _, cx| {
        view.door = false;
        cx.notify();
    });
    let open = door_row("nodes-door-open", true, cx, theme);
    let shut = door_row("nodes-door-close", false, cx, theme);
    Some(
        design::sheet("nodes-door-sheet", "The door", &LADDER, theme)
            .on_dismiss(close)
            .child(div().flex().flex_col().child(open).child(shut))
            .into_any_element(),
    )
}

/// One of the door's sheet rows: the motion that opens or closes the
/// door, and what it means as the row's hint.
fn door_row(id: &'static str, open: bool, cx: &Context<Nodes>, theme: &Theme) -> impl IntoElement {
    let (label, meaning) = words::door(open);
    let vote = cx.listener(move |view: &mut Nodes, _: &ClickEvent, _, cx| {
        view.door = false;
        view.press(Motion::Door { open }, cx);
    });
    // SDK gap: the sheet row's hint (caption, muted, at the right), swap
    // when feat/phone-kit lands
    design::sheet_row(id, None, label, false, &LADDER, theme, vote).child(
        div()
            .flex_none()
            .text_type(LADDER.caption)
            .text_color(theme.muted)
            .child(meaning),
    )
}
