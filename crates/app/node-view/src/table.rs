//! The members table: one row per key, validators then residents, each
//! with the blocks it led of the strip and its Height, Behind and Status
//! cells (`row.rs`), then a line on where those come from.
use abi::hex;
use ducktape_view_guest::design;
use ducktape_view_guest::prelude::*;
use ducktape_view_guest::{Div, Loadable, Stateful};

use crate::queries::Node;
use crate::recent::Recent;
use crate::row::{self, QUIET_PER_VALIDATOR, Row, Status};
use crate::{Nodes, ui};

const KEY_W: Pixels = px(112.);
const ADDRESS_W: Pixels = px(196.);
const STRIP_W: Pixels = px(392.);
const HEIGHT_W: Pixels = px(112.);
const BEHIND_W: Pixels = px(60.);
const STATUS_W: Pixels = px(136.);
/// Every column but the strip, and the row's padding: the table never
/// squeezes a cell.
const WORDS_W: f32 = 112. + 196. + 112. + 60. + 136. + 16.;
/// The sheet's padding either side (`p_5`).
const INSET: f32 = 20.;

/// Whether the strip fits beside the other columns. A narrower sheet (the
/// desk opens a window at 60% of its width; 680) keeps every other column
/// and leaves the strip out, rather than scrolling Status out of sight.
fn strip_fits(view: &Nodes) -> bool {
    view.width
        .is_none_or(|width| width - 2. * INSET >= WORDS_W + f32::from(STRIP_W))
}

pub(crate) fn table(view: &Nodes, nodes: &[Node], theme: &Theme) -> Stateful<Div> {
    let head = view.status.ready().map_or(0, |status| status.height);
    let this = view.status.ready().map(|status| status.identity.as_slice());
    let validators = nodes.iter().filter(|node| node.validator).count() as u64;
    let strip = strip_fits(view);
    let rows = |validator: bool| {
        nodes
            .iter()
            .enumerate()
            .filter(move |(_, node)| node.validator == validator)
            .map(move |(index, node)| {
                let this = this == Some(node.key.as_slice());
                let cells = match &view.network {
                    Loadable::Ready(network) => row::synced(node, validators, network),
                    _ => row::unsynced(node, head, validators, &view.recent),
                };
                let marks = strip.then(|| self::strip(node, head, &view.recent, theme));
                line(index, node, this, cells, marks, theme)
            })
    };
    let residents = nodes.len() as u64 - validators;
    div()
        .id("nodes-table")
        .w_full()
        .min_w(px(WORDS_W + if strip { f32::from(STRIP_W) } else { 0. }))
        .flex()
        .flex_col()
        .child(columns(head, strip, theme))
        .child(ui::section(
            "nodes-validators",
            format!("Validators · {validators}"),
            theme,
        ))
        .children(rows(true))
        .child(ui::section(
            "nodes-residents",
            format!("Residents · {residents}"),
            theme,
        ))
        .children(rows(false))
        .child(
            design::quiet(footnote(view, validators), theme)
                .id("nodes-footnote")
                .pt_2()
                .px_2(),
        )
}

/// Where the Height column comes from, or what stands in for it.
fn footnote(view: &Nodes, validators: u64) -> String {
    let quiet = design::plural(validators * QUIET_PER_VALIDATOR, "block", "blocks");
    match view.network {
        Loadable::Ready(_) => format!(
            "Height: the last block a validator's signature finalized, as this node applied \
             it. Quiet: none for {quiet}."
        ),
        _ => format!(
            "This node does not report its validators' signatures. A validator reads by the \
             blocks it led, and Quiet after {quiet} without one."
        ),
    }
}

/// The column names; the strip's names its span.
fn columns(head: u64, strip: bool, theme: &Theme) -> impl IntoElement {
    let span = Recent::span(head);
    let label = format!(
        "Proposed · last {} · {} → {}",
        design::plural(span.end() - span.start() + 1, "block", "blocks"),
        design::grouped(*span.start()),
        design::grouped(*span.end()),
    );
    let name = |text: String, width: Pixels, right: bool| cell(width, right).child(text);
    div()
        .id("nodes-columns")
        .flex()
        .items_center()
        .h(design::size::CONTROL)
        .px_2()
        .border_b_1()
        .border_color(theme.border)
        .font_family(design::fonts::FAMILY_MONO)
        .text_size(design::text::CAPTION)
        .text_color(theme.muted)
        .child(name("Key".into(), KEY_W, false))
        .child(name("Address".into(), ADDRESS_W, false))
        .child(stretch(strip.then_some(label)))
        .child(name("Height".into(), HEIGHT_W, true))
        .child(name("Behind".into(), BEHIND_W, true))
        .child(name("Status".into(), STATUS_W, false).pl_4())
}

/// The strip's column: the strip, and the table's spare width after it;
/// only the spare width where the strip does not fit.
fn stretch(content: Option<impl IntoElement>) -> Div {
    match content {
        Some(content) => cell(STRIP_W, false).flex_1().min_w(STRIP_W).child(content),
        None => div().flex_1(),
    }
}

/// A fixed-width cell, its content at the left or the right.
fn cell(width: Pixels, right: bool) -> Div {
    let cell = div()
        .w(width)
        .flex_none()
        .min_w(px(0.))
        .flex()
        .items_center()
        .gap_1()
        .whitespace_nowrap()
        .overflow_hidden();
    match right {
        true => cell.justify_end(),
        false => cell,
    }
}

fn line(
    index: usize,
    node: &Node,
    this: bool,
    cells: Row,
    strip: Option<Div>,
    theme: &Theme,
) -> impl IntoElement {
    let muted = |text: String| div().text_color(theme.muted).child(text);
    let address = match node.address.is_empty() {
        true => "—".to_owned(),
        false => node.address.clone(),
    };
    let height = match cells.signed {
        Some(signed) => cell(HEIGHT_W, true)
            .child(design::mono(design::grouped(signed)))
            .child(muted("signed".into())),
        None => cell(HEIGHT_W, true).child(muted("—".into())),
    };
    // an empty cell reads as Height's and Status's do: a quiet dash
    let behind = match cells.behind {
        Some(behind) if behind < 0 => {
            design::mono(format!("−{}", design::grouped(behind.unsigned_abs())))
        }
        Some(behind) => design::mono(design::grouped(behind as u64)),
        None => muted("—".into()),
    };
    div()
        .id(ElementId::Name(format!("nodes-row-{index}").into()))
        .flex()
        .items_center()
        .min_h(design::size::CONTROL + design::space::SM)
        .px_2()
        .border_b_1()
        .border_color(theme.border)
        .text_size(design::text::SECONDARY)
        .child(cell(KEY_W, false).child(design::mono(design::short_hex(&hex(&node.key)))))
        .child(
            cell(ADDRESS_W, false)
                .child(design::mono(address).text_color(theme.muted).truncate())
                .children(this.then(|| muted("this node".into()))),
        )
        .child(stretch(strip))
        .child(height)
        .child(cell(BEHIND_W, true).child(behind))
        .child(
            cell(STATUS_W, false)
                .pl_4()
                .child(status(index, &cells.status, theme)),
        )
}

/// The last blocks, one mark each, oldest first: ink where this key led,
/// grey where another did, faint where the node names no proposer; the
/// tip framed. A resident leads none.
fn strip(node: &Node, head: u64, recent: &Recent, theme: &Theme) -> Div {
    if !node.validator {
        return div().text_color(theme.muted).child("Doesn't propose");
    }
    let marks = Recent::span(head).map(|height| {
        let fill = match recent.led(&node.key, height) {
            Some(true) => theme.foreground,
            Some(false) => theme.border,
            None => theme.surface,
        };
        let mark = div().w(px(3.)).h(px(14.)).bg(fill);
        match height == head {
            true => div()
                .p(px(1.))
                .border_1()
                .border_color(theme.foreground)
                .child(mark),
            false => mark,
        }
    });
    div().flex().items_center().gap(px(3.)).children(marks)
}

/// The status as one word in its colours; the counts without `chain.network`
/// as a quiet caption.
fn status(index: usize, status: &Status, theme: &Theme) -> impl IntoElement {
    let id = ElementId::named_usize("nodes-status-word", index);
    let badge = |foreground, background| {
        design::badge(id.clone(), status.word(), foreground, background).into_any_element()
    };
    let caption = |text: String| {
        div()
            .text_size(design::text::CAPTION)
            .text_color(theme.muted)
            .child(text)
            .into_any_element()
    };
    let parts: Vec<AnyElement> = match status {
        Status::InSync => vec![badge(theme.success, theme.success_soft)],
        Status::Behind(_) | Status::Ahead(_) => vec![badge(theme.warning, theme.warning_soft)],
        Status::Blank => vec![
            div()
                .text_color(theme.muted)
                .child(status.word())
                .into_any_element(),
        ],
        // no block of the strip names who led it (yet, or ever: a node
        // that state-synced past their certificates)
        Status::Led { of: 0, .. } => vec![caption("Not known".into())],
        Status::Led { .. } => vec![caption(status.word())],
        Status::Quiet { since, of } => {
            let mut parts = vec![badge(theme.warning, theme.warning_soft)];
            match (since, of) {
                (Some(since), _) => {
                    parts.push(caption(format!("since {}", design::grouped(*since))))
                }
                (None, 0) => {}
                (None, of) => {
                    parts.push(caption(format!("not in the last {}", design::grouped(*of))))
                }
            }
            parts
        }
    };
    div().flex().items_center().gap_1().children(parts)
}
