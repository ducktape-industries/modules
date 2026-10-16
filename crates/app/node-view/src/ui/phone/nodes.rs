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
use crate::row::{self, Status};
use crate::{Nodes, motions, table, voting};

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

/// The network's name before its salt (`<label>#<salt>`), once the node
/// says which it is on; Nodes before.
fn title(view: &Nodes) -> String {
    let Some(status) = view.status.ready() else {
        return "Nodes".to_owned();
    };
    let chain_id = status.chain_id.as_str();
    chain_id
        .split_once('#')
        .map_or(chain_id, |(label, _)| label)
        .to_owned()
}

/// Whether this node answers, and which node it is. Expo draws the word in
/// its tone and the key in the data face; the SDK's caption takes text in
/// one tone (a SHARED row for the chief).
fn pulse(view: &Nodes) -> Option<String> {
    let status = view.status.ready()?;
    let word = match view.answering() {
        true => "In sync",
        false => "Not answering",
    };
    Some(format!(
        "● {word} · this node {}",
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
        let retry = cx.listener(|view: &mut Nodes, _: &ClickEvent, _, cx| {
            view.status = Loadable::Idle;
            view.read_status(cx);
            cx.notify();
        });
        return refused("nodes-status", &refusal.message, retry, theme).into_any_element();
    }
    let Some(status) = view.status.ready() else {
        return quiet("nodes-status-loading", "Reading node status…", theme).into_any_element();
    };
    let last = match view.ticks - view.moved {
        0 => "last block just now".to_owned(),
        seconds => format!("last block {} ago", design::ago(seconds * 1000, 0)),
    };
    let height = Fact {
        key: "height",
        label: "Height",
        value: design::grouped(status.height),
        mono: true,
        caption: Some(last),
    };
    let block_time = Fact {
        key: "block-time",
        label: "Block time",
        value: format!("{} ms", design::grouped(status.block_time_ms)),
        mono: false,
        caption: None,
    };
    div()
        .child(facts([height, block_time], theme))
        .children(epoch(status, theme))
        .into_any_element()
}

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
            .map(|(index, fact)| fact_cell(fact, index % 2 == 0, theme)),
    )
}

/// One fact's cell: the eyebrow (the data face, medium, muted, in
/// capitals), the value in heading type, the caption muted; `first` of
/// its row, it draws the hairline between the two.
fn fact_cell(fact: Fact, first: bool, theme: &Theme) -> Stateful<Div> {
    div()
        .id(ElementId::Name(format!("nodes-status-{}", fact.key).into()))
        .w_1_2()
        .flex()
        .flex_col()
        .gap(space::HAIR)
        .py(space::LG)
        .px(space::BLOCK)
        .border_b_1()
        .when(first, |cell| cell.border_r_1())
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
/// next one starts. A block closes epoch `e` when `(height + 1)` reaches
/// `(e + 1)` epochs, so the tip is `(height + 1) % length` blocks into its
/// epoch. Nothing on a network without epochs.
fn epoch(status: &NodeStatus, theme: &Theme) -> Option<Stateful<Div>> {
    let length = status.epoch_length;
    if length == 0 {
        return None;
    }
    let into = (status.height + 1) % length;
    let place = format!(
        "Epoch {} · {into} / {length}",
        design::grouped(status.epoch)
    );
    let next = format!(
        "Next epoch in {}",
        design::plural(length - into, "block", "blocks")
    );
    Some(
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
                    .child(place),
            )
            .child(epoch_bar(into, length, theme))
            .child(
                div()
                    .id("nodes-status-next-epoch")
                    .text_type(LADDER.caption)
                    .text_color(theme.muted)
                    .child(next),
            ),
    )
}

/// The epoch's bar: ink for the blocks gone, the hairline's grey for those
/// to come, sharing its width as the epoch does.
fn epoch_bar(into: u64, length: u64, theme: &Theme) -> Div {
    let part = |blocks: u64, fill: Hsla| {
        let mut part = div().h_full().flex_basis(px(0.)).bg(fill);
        part.style().flex_grow = Some(blocks as f32);
        part
    };
    div()
        .flex()
        .h(EPOCH_BAR)
        .bg(theme.border)
        .child(part(into, theme.foreground))
        .child(part(length - into, theme.border))
}

/// The members, or why they are not here: still being read, refused with
/// Retry, none at all, or waiting on the node's status their rows read
/// (the numbers above say why).
fn members(view: &Nodes, cx: &mut Context<Nodes>, theme: &Theme) -> AnyElement {
    if let Some(refusal) = view.nodes.failed() {
        let retry = cx.listener(|view: &mut Nodes, _: &ClickEvent, _, cx| view.read(cx));
        return refused("nodes", &refusal.message, retry, theme).into_any_element();
    }
    let Some(nodes) = view.nodes.ready() else {
        return quiet("nodes-loading", "Reading the members…", theme).into_any_element();
    };
    if nodes.is_empty() {
        let none = "The validator set of this network is empty.";
        return empty("nodes-empty", "server", "No members", none, theme).into_any_element();
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
    let heard = table::heard(view);
    let rows = |validators: bool| -> Vec<Stateful<Div>> {
        nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.validator == validators)
            .map(|(index, node)| member_row(view, index, node, cx, theme))
            .collect()
    };
    let (seats, residents) = (rows(true), rows(false));
    let footnote = format!(
        "Height: the newest block the validator voted to finalize. Quiet: none for {}.",
        design::plural(row::QUIET, "block", "blocks")
    );
    div()
        .id("nodes-members")
        .children(table::no_votes(view, heard).map(|(id, why)| quiet(id, why, theme)))
        .children(last_answer(view, "nodes-last-answer", theme))
        .child(count_head(
            "nodes-validators",
            "Validators",
            seats.len(),
            theme,
        ))
        .children(seats)
        .child(count_head(
            "nodes-residents",
            "Residents",
            residents.len(),
            theme,
        ))
        .children(residents)
        .children(
            heard
                .is_some()
                .then(|| quiet("nodes-footnote", footnote, theme)),
        )
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
    let status = table::heard(view)
        .map_or(row::BLANK, |network| row::synced(node, network))
        .status;
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
    let address = node.address.clone().unwrap_or_else(|| "—".to_owned());
    let this = view
        .status
        .ready()
        .is_some_and(|status| status.identity == node.key);
    let words = match this {
        true => format!("{address} · this node"),
        false => address,
    };
    div()
        .font_family(design::fonts::FAMILY_MONO)
        .text_type(LADDER.caption)
        .text_color(theme.muted)
        .truncate()
        .child(words)
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
    let motions_head = design::section_head("nodes-motions", "Motions", None, &LADDER, theme);
    let scheduled_head =
        design::section_head("nodes-scheduled", "Scheduled changes", None, &LADDER, theme);
    let elsewhere = "Change a program in Explorer, Programs.";
    Some(
        div()
            .child(motions_head)
            .children(waiting(view, cx, theme))
            .child(scheduled_head)
            .children(scheduled(view, cx, theme))
            .child(quiet("nodes-change-elsewhere", elsewhere, theme)),
    )
}

/// The motions that wait for votes, or why there are none to show:
/// refused with Retry, still being read, or none waiting.
fn waiting(view: &Nodes, cx: &mut Context<Nodes>, theme: &Theme) -> Vec<AnyElement> {
    let voting = &view.voting;
    if let Some(refusal) = voting.motions.failed() {
        let retry = cx.listener(|view: &mut Nodes, _: &ClickEvent, _, cx| view.read_motions(cx));
        return vec![refused("nodes-motions", &refusal.message, retry, theme).into_any_element()];
    }
    let Some(motions) = voting.motions.ready() else {
        let reading = quiet("nodes-motions-loading", "Reading the motions…", theme);
        return vec![reading.into_any_element()];
    };
    if motions.pending.is_empty() {
        let none = quiet("nodes-motions-none", "No motion waits for votes.", theme);
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
            "Vote",
            voting.pressable(),
            theme,
            press(cx, pending.motion.clone()),
        )
        .into_any_element(),
        false => design::badge(
            ElementId::Name(format!("nodes-motion-voted-{index}").into()),
            "voted",
            &LADDER,
            theme.muted,
            theme.surface_raised,
        )
        .into_any_element(),
    };
    line(
        ElementId::Name(format!("nodes-motion-{index}").into()),
        voting.words(&pending.motion),
        motions::pending_caption(voting, pending, motions.needed),
        tail,
        theme,
    )
}

/// The changes scheduled past the tip, or why there are none to show:
/// still being read, or none scheduled. A refused read says its refusal
/// (kept from the desktop: Expo reads it as still being read).
fn scheduled(view: &Nodes, cx: &mut Context<Nodes>, theme: &Theme) -> Vec<AnyElement> {
    let voting = &view.voting;
    if let Some(refusal) = voting.scheduled.failed() {
        let why = quiet("nodes-scheduled-refused", refusal.message.clone(), theme);
        return vec![why.into_any_element()];
    }
    let Some(schedule) = voting.scheduled.ready() else {
        let reading = quiet("nodes-scheduled-loading", "Reading the schedule…", theme);
        return vec![reading.into_any_element()];
    };
    let tip = view.status.ready().map_or(0, |status| status.height);
    let ahead: Vec<&Scheduled> = schedule
        .changes
        .iter()
        .filter(|change| change.height > tip)
        .collect();
    if ahead.is_empty() {
        let none = quiet("nodes-scheduled-none", "No change is scheduled.", theme);
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
    let roster = voting.roster.ready().map_or(&[][..], Vec::as_slice);
    let cancel = small_button(
        ElementId::Name(format!("nodes-scheduled-cancel-{index}").into()),
        "Propose cancel",
        voting.pressable(),
        theme,
        press(cx, voting::cancel(change)),
    );
    line(
        ElementId::Name(format!("nodes-scheduled-{index}").into()),
        voting::scheduled_words(change.height, &change.change, roster),
        motions::scheduled_caption(change, schedule, view.status.ready()),
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
    let open = door_row(
        "nodes-door-open",
        "Open the door",
        "anyone enrolls without an invite",
        Motion::Door { open: true },
        cx,
        theme,
    );
    let shut = door_row(
        "nodes-door-close",
        "Close the door",
        "enrolling takes an invite",
        Motion::Door { open: false },
        cx,
        theme,
    );
    Some(
        design::sheet("nodes-door-sheet", "The door", &LADDER, theme)
            .on_dismiss(close)
            .child(div().flex().flex_col().child(open).child(shut))
            .into_any_element(),
    )
}

/// One of the door's sheet rows: its label, and its hint (the kit's
/// `SheetRow` hint: caption, muted, at the right).
fn door_row(
    id: &'static str,
    label: &'static str,
    hint: &'static str,
    motion: Motion,
    cx: &Context<Nodes>,
    theme: &Theme,
) -> impl IntoElement {
    let vote = cx.listener(move |view: &mut Nodes, _: &ClickEvent, _, cx| {
        view.door = false;
        view.press(motion.clone(), cx);
    });
    design::sheet_row(id, None, label, false, &LADDER, theme, vote).child(
        div()
            .flex_none()
            .text_type(LADDER.caption)
            .text_color(theme.muted)
            .child(hint),
    )
}
