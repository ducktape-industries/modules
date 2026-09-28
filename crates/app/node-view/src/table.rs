//! The members table: one row per key, validators then residents, each
//! with the blocks it led of the strip and its Height, Behind, Heard and
//! Status cells (`row.rs`), then a line on where those come from.
use abi::hex;
use ducktape_view_guest::design;
use ducktape_view_guest::prelude::*;
use ducktape_view_guest::{Div, Loadable, Stateful};

use crate::queries::Node;
use crate::recent::Recent;
use crate::row::{self, QUIET_PER_VALIDATOR, Row, Status};
use crate::{Nodes, ui};

const KEY_W: Pixels = px(124.);
const ADDRESS_W: Pixels = px(140.);
const STRIP_W: Pixels = px(392.);
const HEIGHT_W: Pixels = px(112.);
const BEHIND_W: Pixels = px(60.);
const HEARD_W: Pixels = px(92.);
const STATUS_W: Pixels = px(112.);
/// Every column and the row's padding: the table never squeezes a cell.
const TABLE_W: Pixels = px(124. + 140. + 392. + 112. + 60. + 92. + 112. + 16.);

pub(crate) fn table(view: &Nodes, nodes: &[Node], theme: &Theme) -> Stateful<Div> {
    let head = view.status.ready().map_or(0, |status| status.height);
    let this = view.status.ready().map(|status| status.identity.as_slice());
    let validators = nodes.iter().filter(|node| node.validator).count() as u64;
    let rows = |validator: bool| {
        nodes
            .iter()
            .enumerate()
            .filter(move |(_, node)| node.validator == validator)
            .map(move |(index, node)| {
                let this = this == Some(node.key.as_slice());
                let cells = match &view.network {
                    Loadable::Ready(network) => {
                        row::shown(node, this, network, view.settled, view.earlier.as_ref())
                    }
                    _ => row::unsynced(node, this, head, validators, &view.recent),
                };
                line(index, node, cells, head, &view.recent, theme)
            })
    };
    let residents = nodes.len() as u64 - validators;
    div()
        .id("nodes-table")
        .min_w(TABLE_W)
        .max_w(TABLE_W)
        .flex()
        .flex_col()
        .child(columns(head, theme))
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

/// Where the Height and Heard columns come from, or why they are empty.
fn footnote(view: &Nodes, validators: u64) -> String {
    match view.network {
        Loadable::Ready(_) => "Height: for a validator, the last block its signature finalized; \
             for a resident, the height it reports. Heard: when it last answered this node, \
             which asks every second."
            .into(),
        _ => format!(
            "This node does not report its members' heights. A validator reads by the blocks \
             it led, and Quiet after {} without one; a resident reports no height to this node.",
            design::plural(validators * QUIET_PER_VALIDATOR, "block", "blocks")
        ),
    }
}

/// The column names; the strip's names its span.
fn columns(head: u64, theme: &Theme) -> impl IntoElement {
    let span = Recent::span(head);
    let strip = format!(
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
        .child(name(strip, STRIP_W, false))
        .child(name("Height".into(), HEIGHT_W, true))
        .child(name("Behind".into(), BEHIND_W, true))
        .child(name("Heard".into(), HEARD_W, true))
        .child(name("Status".into(), STATUS_W, false).pl_4())
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
    cells: Row,
    head: u64,
    recent: &Recent,
    theme: &Theme,
) -> impl IntoElement {
    let muted = |text: String| div().text_color(theme.muted).child(text);
    let address = match node.address.is_empty() {
        true => "—".to_owned(),
        false => node.address.clone(),
    };
    let height = match cells.height {
        Some((height, backed)) => cell(HEIGHT_W, true)
            .child(design::mono(design::grouped(height)))
            .children((!backed.is_empty()).then(|| muted(backed.into())))
            .text_size(design::text::SECONDARY),
        None => cell(HEIGHT_W, true).child(muted("—".into())),
    };
    let behind = match cells.behind {
        Some(behind) if behind < 0 => format!("−{}", design::grouped(behind.unsigned_abs())),
        Some(behind) => design::grouped(behind as u64),
        None => "—".into(),
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
            cell(ADDRESS_W, false).child(design::mono(address).text_color(theme.muted).truncate()),
        )
        .child(cell(STRIP_W, false).child(strip(node, head, recent, theme)))
        .child(height)
        .child(cell(BEHIND_W, true).child(design::mono(behind)))
        .child(cell(HEARD_W, true).child(muted(cells.heard)))
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
        Status::NotAnswering => vec![badge(theme.danger, theme.danger_soft)],
        Status::Withheld | Status::NotReported | Status::Checking => {
            vec![badge(theme.muted, theme.surface_raised)]
        }
        Status::Led { of: 0, .. } => vec![caption("—".into())],
        Status::Led { .. } => vec![caption(status.word())],
        Status::Quiet { since, of } => vec![
            badge(theme.warning, theme.warning_soft),
            caption(match since {
                Some(since) => format!("since {}", design::grouped(*since)),
                None => format!("not in the last {}", design::grouped(*of)),
            }),
        ],
    };
    div().flex().items_center().gap_1().children(parts)
}
