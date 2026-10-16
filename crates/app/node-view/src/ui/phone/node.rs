//! One member's page, Expo `app/(tabs)/nodes/[key].tsx`: its short key in
//! the title row over its seat; its key, address and operator; for a
//! validator its newest finalize vote, how far that is behind, and its
//! status; the blocks it led of the last 64; and for a reader who votes,
//! the motions on it (Demote and Remove a validator, Promote a resident).
//! A key that holds no seat says so.
//!
//! Reads the members, the node's status, the network's votes as this node
//! heard them, the recent blocks (`Nodes::recent`) and the vote. Writes
//! `Nodes::open` (the page's Back) and a vote through `Nodes::press`.
use abi::hex;
use std::ops::RangeInclusive;
use view::design::{Page, space};
use view::prelude::*;
use view::{Div, Stateful};

use super::LADDER;
use super::parts::{empty, last_answer, outcome, press, quiet, scroller, small_button, tint};
use crate::queries::Node;
use crate::recent::Recent;
use crate::row;
use crate::{Nodes, voting, words};

// SDK gap: the kit's `KeyValue` (with `key_value`); the SDK builds it later
/// How wide a key's label runs (Expo `src/kit/Facts.tsx:64`).
const KEY_LABEL_W: Pixels = px(96.);
/// How tall a block's mark is in the strip (Expo `[key].tsx:80`).
const MARK: Pixels = px(14.);
/// How far the tip's frame sits from its mark (Expo `[key].tsx:79`).
const TIP_FRAME: Pixels = px(1.);

/// The page of the member `key`, or why there is none.
pub(super) fn render(view: &Nodes, key: &[u8], cx: &mut Context<Nodes>, theme: &Theme) -> Page {
    let back = cx.listener(|view: &mut Nodes, _: &ClickEvent, _, cx| {
        view.close_page();
        cx.notify();
    });
    let nodes = view.nodes.ready();
    let Some(node) = nodes.and_then(|nodes| nodes.iter().find(|node| node.key == key)) else {
        let page = design::page(
            "nodes-node",
            "Node",
            design::Nav::back(back),
            &LADDER,
            theme,
        );
        return page.child(missing(view, theme));
    };
    let body = div()
        .child(identity(view, node, theme))
        .children(node.validator.then(|| vote_facts(view, node, theme)))
        .child(proposed_head(view, theme))
        .children(proposed(view, node, theme))
        .children(presses(view, node, cx, theme))
        .children(last_answer(view, "nodes-node-last-answer", theme));
    design::page(
        "nodes-node",
        design::short_hex(&hex(&node.key)),
        design::Nav::back(back),
        &LADDER,
        theme,
    )
    .caption(seat(node))
    .children(outcome(view, cx, theme))
    .child(scroller("nodes-node-scroll", body))
}

/// A key the page cannot show: one the members, once read, do not hold;
/// or the members still being read.
fn missing(view: &Nodes, theme: &Theme) -> AnyElement {
    match view.nodes.ready() {
        Some(_) => empty(
            "nodes-node-none",
            "server-off",
            "Not a member",
            "This key holds no seat on the network.",
            theme,
        )
        .into_any_element(),
        None => quiet("nodes-node-loading", words::READING_MEMBERS, theme).into_any_element(),
    }
}

/// The member's seat under its key: `validator 3`, or `resident`.
fn seat(node: &Node) -> String {
    match (node.validator, node.seat) {
        (true, Some(seat)) => format!("validator {}", seat + 1),
        (true, None) => "validator".to_owned(),
        (false, _) => "resident".to_owned(),
    }
}

/// Who the member is: its key, its address (`this node` on the one this
/// app talks to), and who operates it.
fn identity(view: &Nodes, node: &Node, theme: &Theme) -> Div {
    let address = view.address_line(node);
    div()
        .child(key_value("key", words::KEY, mono(hex(&node.key)), theme))
        .child(key_value("address", words::ADDRESS, mono(address), theme))
        .children(
            node.operator
                .clone()
                .map(|operator| key_value("operator", "Operator", plain(operator), theme)),
        )
}

/// A validator's vote: its newest finalize vote, how far that is behind
/// this node's tip, and its status as a badge.
fn vote_facts(view: &Nodes, node: &Node, theme: &Theme) -> Div {
    let row::Row {
        signed,
        behind,
        status,
    } = view.cells(node);
    let height = signed.map_or(words::NO_VALUE.to_owned(), |signed| {
        format!("{} {}", design::grouped(signed), words::VOTED)
    });
    let behind = behind.map_or(words::NO_VALUE.to_owned(), design::grouped);
    let (tone, ground) = tint(&status, view.answering(), theme);
    let badge = design::badge(
        "nodes-node-status-word",
        status.word(),
        &LADDER,
        tone,
        ground,
    );
    div()
        .child(key_value("height", words::HEIGHT, mono(height), theme))
        .child(key_value("behind", words::BEHIND, mono(behind), theme))
        // the badge fits its word, centred in the value's column (Expo
        // `Badge.tsx:20` `alignSelf: 'center'` in `Facts.tsx:67`'s column)
        .child(key_value(
            "status",
            words::STATUS,
            div().flex().justify_center().child(badge),
            theme,
        ))
}

/// The kit's `KeyValue` (Expo `src/kit/Facts.tsx:50-75`): at least a
/// control tall, its label muted in a column of its own, its value taking
/// the rest, a hairline under it.
fn key_value(key: &str, label: &str, value: impl IntoElement, theme: &Theme) -> Stateful<Div> {
    div()
        .id(ElementId::Name(format!("nodes-node-{key}").into()))
        .min_h(LADDER.control)
        .flex()
        .items_start()
        .gap(space::LG)
        .py(space::MD)
        .px(space::BLOCK)
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .w(KEY_LABEL_W)
                .flex_none()
                .text_type(LADDER.small)
                .text_color(theme.muted)
                .child(label.to_owned()),
        )
        .child(div().flex_1().min_w_0().child(value))
}

/// A value in small type, wrapping.
fn plain(value: String) -> Div {
    div().text_type(LADDER.small).child(value)
}

/// A value in small type in the data face, wrapping.
fn mono(value: String) -> Div {
    plain(value).font_family(design::fonts::FAMILY_MONO)
}

/// The blocks the strip spans: the last 64 to the tip, once the node
/// says where its tip is.
fn span(view: &Nodes) -> Option<RangeInclusive<u64>> {
    view.status
        .ready()
        .map(|status| Recent::span(status.height))
}

/// `Proposed`, and at its right which blocks the strip spans.
fn proposed_head(view: &Nodes, theme: &Theme) -> impl IntoElement {
    let range = span(view).map(|span| words::span(&span));
    design::section_head("nodes-node-proposed", words::PROPOSED, &LADDER, theme).children(range)
}

/// What the member proposed: a validator's strip once the tip is known; a
/// resident proposes none.
fn proposed(view: &Nodes, node: &Node, theme: &Theme) -> Option<AnyElement> {
    match node.validator {
        true => span(view).map(|span| strip(node, span, &view.recent, theme).into_any_element()),
        false => {
            let none = quiet("nodes-node-no-blocks", words::DOESNT_PROPOSE, theme);
            Some(none.into_any_element())
        }
    }
}

/// The blocks of `span`, one mark each sharing the width, oldest first:
/// ink where this key led, grey where another did, the surface where the
/// node names no proposer; the tip framed.
fn strip(node: &Node, span: RangeInclusive<u64>, recent: &Recent, theme: &Theme) -> Stateful<Div> {
    let tip = *span.end();
    let marks = span.map(|height| {
        let fill = match recent.led(&node.key, height) {
            Some(true) => theme.foreground,
            Some(false) => theme.border,
            None => theme.surface,
        };
        let mark = div().flex_1().child(div().h(MARK).bg(fill));
        match height == tip {
            true => mark.p(TIP_FRAME).border_1().border_color(theme.foreground),
            false => mark,
        }
    });
    div()
        .id("nodes-node-strip")
        .flex()
        .items_center()
        .gap(space::HAIR)
        .px(space::BLOCK)
        .py(space::MD)
        .children(marks)
}

/// For a reader who votes, the motions on this member: Demote and Remove
/// a validator, Promote a resident.
fn presses(view: &Nodes, node: &Node, cx: &Context<Nodes>, theme: &Theme) -> Option<Div> {
    if !view.voting.votes() {
        return None;
    }
    let pressable = view.voting.pressable();
    let buttons = voting::motions_on(node)
        .into_iter()
        .map(|(name, label, motion)| {
            small_button(
                ElementId::Name(format!("nodes-node-{name}").into()),
                label,
                pressable,
                theme,
                press(cx, motion),
            )
        });
    Some(
        div()
            .flex()
            .gap(space::SM)
            .p(space::BLOCK)
            .children(buttons),
    )
}
