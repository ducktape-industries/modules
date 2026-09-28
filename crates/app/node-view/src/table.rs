//! The members table: one row per key, validators then residents, each
//! with the blocks it led of the strip and its Height, Behind and Status
//! cells (`row.rs`), then a line on where those come from.
use abi::hex;
use ducktape_view_guest::design;
use ducktape_view_guest::prelude::*;
use ducktape_view_guest::{Div, Loadable, Stateful};

use crate::queries::Node;
use crate::recent::Recent;
use crate::row::{self, QUIET, QUIET_PER_VALIDATOR, Row, Status};
use crate::{Nodes, ui};

const KEY_W: Pixels = px(104.);
const ADDRESS_W: Pixels = px(196.);
const STRIP_W: Pixels = px(392.);
const HEIGHT_W: Pixels = px(112.);
const BEHIND_W: Pixels = px(60.);
const STATUS_W: Pixels = px(136.);
/// Key, Height, Behind and Status, and the row's padding: the columns every
/// width keeps. The table never squeezes a cell.
const WORDS_W: f32 = 104. + 112. + 60. + 136. + 16.;
/// The sheet's inset either side, in pixels so the columns count against it.
pub(crate) const INSET: f32 = 16.;

/// Whether `extra` fits beside the columns every width keeps, in the
/// sheet's width less its inset and the scroller's gutter: Address from
/// 672 px, Address and the strip from 1,064. The app lays a view out at 480
/// at the least, where the kept columns fit; before the first measure,
/// every column.
fn fits(view: &Nodes, extra: f32) -> bool {
    let gutter = f32::from(design::size::SCROLLBAR);
    view.width
        .is_none_or(|width| width - 2. * INSET - gutter >= WORDS_W + extra)
}

/// Whether the Address column fits.
fn address_fits(view: &Nodes) -> bool {
    fits(view, f32::from(ADDRESS_W))
}

/// Whether the strip fits beside the other columns. A narrower sheet keeps
/// every other column and leaves the strip out, rather than scrolling Status
/// out of sight.
fn strip_fits(view: &Nodes) -> bool {
    fits(view, f32::from(ADDRESS_W) + f32::from(STRIP_W))
}

pub(crate) fn table(view: &Nodes, nodes: &[Node], theme: &Theme) -> Stateful<Div> {
    let head = view.status.ready().map_or(0, |status| status.height);
    let this = view.status.ready().map(|status| status.identity.as_slice());
    let validators = nodes.iter().filter(|node| node.validator).count() as u64;
    // only a node seated as a validator hears the votes, its own among
    // them: without its own, it hears none (a resident, one not seated yet,
    // one catching up after a restart)
    let deaf = view.network.ready().is_some_and(|network| {
        !network
            .members
            .iter()
            .any(|peer| this == Some(peer.key.as_slice()) && peer.signed.is_some())
    });
    let answering = view.answering();
    let (address, strip) = (address_fits(view), strip_fits(view));
    let rows = |validator: bool| {
        nodes
            .iter()
            .enumerate()
            .filter(move |(_, node)| node.validator == validator)
            .map(move |(index, node)| {
                let this = this == Some(node.key.as_slice());
                let cells = match &view.network {
                    Loadable::Ready(_) if deaf => row::BLANK,
                    Loadable::Ready(network) => row::synced(node, network),
                    _ => row::unsynced(node, head, validators, &view.recent),
                };
                let marks = strip.then(|| self::strip(node, head, &view.recent, theme));
                line(index, node, this, cells, marks, view, theme)
            })
    };
    let residents = nodes.len() as u64 - validators;
    let no_votes = deaf.then(|| {
        note(
            "nodes-no-votes",
            "This node isn't voting right now, so it can't see the validators' votes.".into(),
            theme,
        )
    });
    // the rows are what the node last said: how long ago, once it is silent
    let silent = (!answering).then(|| {
        let age = design::ago((view.ticks - view.answered) * 1000, 0);
        note("nodes-last-answer", format!("Last answer {age} ago"), theme)
    });
    div()
        .id("nodes-table")
        .w_full()
        .min_w(px(WORDS_W
            + if address { f32::from(ADDRESS_W) } else { 0. }
            + if strip { f32::from(STRIP_W) } else { 0. }))
        .flex()
        .flex_col()
        .children(no_votes)
        .children(silent)
        .child(columns(head, address, strip, theme))
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
        // with no votes to read, the line above says why the cells are empty
        .children((!deaf).then(|| {
            design::quiet(footnote(view, validators), theme)
                .id("nodes-footnote")
                .pt_2()
                .px_2()
        }))
}

/// One plain line over the table: why its rows read as they do.
fn note(id: &'static str, text: String, theme: &Theme) -> impl IntoElement {
    design::quiet(text, theme).id(id).pb_2().px_2()
}

/// Where the Height column comes from, or what stands in for it.
fn footnote(view: &Nodes, validators: u64) -> String {
    match view.network {
        Loadable::Ready(_) => format!(
            "Height: the newest block the validator voted to finalize. Quiet: none for {}.",
            design::plural(QUIET, "block", "blocks")
        ),
        _ => format!(
            "This node does not report its validators' signatures. A validator reads by the \
             blocks it led, and Quiet after {} without one.",
            design::plural(validators * QUIET_PER_VALIDATOR, "block", "blocks")
        ),
    }
}

/// The column names; the strip's names its span.
fn columns(head: u64, address: bool, strip: bool, theme: &Theme) -> impl IntoElement {
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
        .children(address.then(|| name("Address".into(), ADDRESS_W, false)))
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
    view: &Nodes,
    theme: &Theme,
) -> impl IntoElement {
    let muted = |text: String| div().text_color(theme.muted).child(text);
    let address = address_fits(view);
    // without the Address column, this node's mark goes under its key
    let key = design::mono(design::short_hex(&hex(&node.key)));
    let key = match this && !address {
        true => cell(KEY_W, false)
            .flex_col()
            .items_start()
            .gap_0()
            .child(key)
            .child(muted("this node".into()).text_size(design::text::CAPTION)),
        false => cell(KEY_W, false).child(key),
    };
    let address = address.then(|| {
        let address = match node.address.is_empty() {
            true => "—".to_owned(),
            false => node.address.clone(),
        };
        cell(ADDRESS_W, false)
            .child(design::mono(address).text_color(theme.muted).truncate())
            .children(this.then(|| muted("this node".into())))
    });
    let height = match cells.signed {
        Some(signed) => cell(HEIGHT_W, true)
            .child(design::mono(design::grouped(signed)))
            .child(muted("voted".into())),
        None => cell(HEIGHT_W, true).child(muted("—".into())),
    };
    // an empty cell reads as Height's and Status's do: a quiet dash
    let behind = match cells.behind {
        Some(behind) => design::mono(design::grouped(behind)),
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
        .child(key)
        .children(address)
        .child(stretch(strip))
        .child(height)
        .child(cell(BEHIND_W, true).child(behind))
        .child(cell(STATUS_W, false).pl_4().child(status(
            index,
            &cells.status,
            view.answering(),
            theme,
        )))
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

/// The status as one word in its colours, grey while the node is silent
/// (the word is what it said then); the counts without `chain.network` as a
/// quiet caption.
fn status(index: usize, status: &Status, answering: bool, theme: &Theme) -> impl IntoElement {
    let id = ElementId::named_usize("nodes-status-word", index);
    let badge = |foreground, background| {
        let (foreground, background) = match answering {
            true => (foreground, background),
            false => (theme.muted, theme.surface_raised),
        };
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
        Status::Behind(_) => vec![badge(theme.warning, theme.warning_soft)],
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
