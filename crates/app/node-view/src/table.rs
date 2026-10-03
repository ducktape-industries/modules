//! The members table: one row per key, validators then residents, each
//! with the blocks it led of the strip and its Height, Behind and Status
//! cells (`row.rs`), then a line on where those come from.
use abi::hex;
use ducktape_view_guest::design;
use ducktape_view_guest::prelude::*;
use ducktape_view_guest::{Div, Loadable, Stateful};

use crate::queries::Node;
use crate::recent::{Recent, WINDOW};
use crate::row::{self, QUIET, Row, Status};
use crate::{Nodes, ui};

const KEY_W: Pixels = px(104.);
const ADDRESS_W: Pixels = px(196.);
const HEIGHT_W: Pixels = px(112.);
const BEHIND_W: Pixels = px(60.);
const STATUS_W: Pixels = px(136.);
/// Key, Height, Behind and Status, and the row's padding: the columns every
/// width keeps. The table never squeezes a cell.
const WORDS_W: f32 = 104. + 112. + 60. + 136. + 16.;
/// The sheet's inset either side, in pixels so the columns count against it.
pub(crate) const INSET: f32 = 16.;
/// A mark's width, and the gaps between marks, widest first: the strip
/// narrows a whole pixel at a time, so every mark keeps its pixels.
const MARK_W: f32 = 3.;
const GAPS: [f32; 3] = [3., 2., 1.];
/// One character's advance in [`design::fonts::FAMILY_MONO`], in ems: the
/// font's published metric, since no text measurement reaches a view. The
/// strip's column is kept as wide as its name.
const MONO_EM: f32 = 0.6;
/// What a node without `chain.network` shows over the table.
pub(crate) const NO_NETWORK: &str =
    "This node doesn't report its validators' votes. Update the node.";

/// The width beside the columns every width keeps, in the sheet's width
/// less its inset and the scroller's gutter; before the first measure, room
/// for every column.
fn spare(view: &Nodes) -> f32 {
    let gutter = f32::from(design::size::SCROLLBAR);
    view.width
        .map_or(f32::INFINITY, |width| width - 2. * INSET - gutter - WORDS_W)
}

/// Whether the Address column fits: from 672 px. The app lays a view out at
/// 480 at the least, where the kept columns fit.
fn address_fits(view: &Nodes) -> bool {
    spare(view) >= f32::from(ADDRESS_W)
}

/// The strip's width, its marks `gap` apart and the tip framed.
fn marks_w(gap: f32) -> f32 {
    WINDOW as f32 * MARK_W + (WINDOW - 1) as f32 * gap + 4.
}

/// The strip's gap where its column fits beside the other columns: the
/// widest that leaves room for the marks and the column's name, `label`
/// wide. `None` where not even the narrowest does: the strip then goes on
/// a line of its own under each row's cells, at the widest gap, rather than
/// scrolling Status out of sight. With Address, and a four-digit height,
/// 3 px from 1,057, 2 from 994, 1 from 943. Address gives way only at 672:
/// the strip moves under its row and loses nothing, Address would be lost.
fn gap_beside(view: &Nodes, label: f32) -> Option<f32> {
    let address = if address_fits(view) {
        f32::from(ADDRESS_W)
    } else {
        0.
    };
    let room = spare(view) - address;
    GAPS.into_iter()
        .find(|gap| marks_w(*gap).max(label) <= room)
}

pub(crate) fn table(view: &Nodes, nodes: &[Node], theme: &Theme) -> Stateful<Div> {
    let head = view.status.ready().map_or(0, |status| status.height);
    let this = view.status.ready().map(|status| status.identity.as_slice());
    let validators = nodes.iter().filter(|node| node.validator).count() as u64;
    // the votes, where this node hears them: only a node seated as a
    // validator does, its own among them; without its own, it hears none (a
    // resident, one not seated yet, one catching up after a restart)
    let heard = view.network.ready().filter(|network| {
        network
            .members
            .iter()
            .any(|peer| this == Some(peer.key.as_slice()) && peer.signed.is_some())
    });
    let answering = view.answering();
    let span = Recent::span(head);
    let label = format!(
        "Proposed · last {} · {} → {}",
        design::plural(span.end() - span.start() + 1, "block", "blocks"),
        design::grouped(*span.start()),
        design::grouped(*span.end()),
    );
    let label_w =
        (label.chars().count() as f32 * f32::from(design::text::CAPTION) * MONO_EM).ceil();
    let (address, gap) = (address_fits(view), gap_beside(view, label_w));
    // the strip's column where it fits beside the other columns
    let column = gap.map(|gap| px(marks_w(gap).max(label_w)));
    let rows = |validator: bool| {
        nodes
            .iter()
            .enumerate()
            .filter(move |(_, node)| node.validator == validator)
            .map(move |(index, node)| {
                let this = this == Some(node.key.as_slice());
                let cells = heard.map_or(row::BLANK, |network| row::synced(node, network));
                let marks = strip(node, head, gap.unwrap_or(GAPS[0]), &view.recent, theme);
                line(index, node, this, cells, place(marks, column), view, theme)
            })
    };
    let residents = nodes.len() as u64 - validators;
    // why the validators' cells are empty: this node hears no votes, or
    // does not report them (before its first answer, no line)
    let no_votes = match &view.network {
        Loadable::Ready(_) | Loadable::Reloading(_, _) if heard.is_none() => Some((
            "nodes-no-votes",
            "This node isn't voting right now, so it can't see the validators' votes.",
        )),
        Loadable::Failed(_) => Some(("nodes-no-network", NO_NETWORK)),
        _ => None,
    }
    .map(|(id, text)| note(id, text.into(), theme));
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
            + column.map_or(0., f32::from)))
        .flex()
        .flex_col()
        .children(no_votes)
        .children(silent)
        .child(columns(label, address, column, theme))
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
        .children(heard.is_some().then(|| {
            let footnote = format!(
                "Height: the newest block the validator voted to finalize. Quiet: none for {}.",
                design::plural(QUIET, "block", "blocks")
            );
            design::quiet(footnote, theme)
                .id("nodes-footnote")
                .pt_2()
                .px_2()
        }))
}

/// One plain line over the table: why its rows read as they do.
fn note(id: &'static str, text: String, theme: &Theme) -> impl IntoElement {
    design::quiet(text, theme).id(id).pb_2().px_2()
}

/// The column names; the strip's names its span, over the strips, on a
/// line of its own where they go under their rows.
fn columns(
    label: String,
    address: bool,
    column: Option<Pixels>,
    theme: &Theme,
) -> impl IntoElement {
    let name = |text: String, width: Pixels, right: bool| cell(width, right).child(text);
    let (beside, below) = place(label, column);
    let names = div()
        .flex()
        .items_center()
        .h(design::size::CONTROL)
        .child(name("Key".into(), KEY_W, false))
        .children(address.then(|| name("Address".into(), ADDRESS_W, false)))
        .child(stretch(beside))
        .child(name("Height".into(), HEIGHT_W, true))
        .child(name("Behind".into(), BEHIND_W, true))
        .child(name("Status".into(), STATUS_W, false).pl_4());
    ruled("nodes-columns", names, below, theme)
        .font_family(design::fonts::FAMILY_MONO)
        .text_size(design::text::CAPTION)
        .text_color(theme.muted)
}

/// The strip's column's content and its width, where it fits beside the
/// other cells; else a line of its own under them.
fn place<E: IntoElement>(content: E, column: Option<Pixels>) -> (Option<(Pixels, E)>, Option<Div>) {
    match column {
        Some(width) => (Some((width, content)), None),
        None => (None, Some(div().pb_2().child(content))),
    }
}

/// A line of the table, the column names or a member's: its cells, and
/// under them the strip's, where it does not fit beside them.
fn ruled(id: impl Into<ElementId>, cells: Div, below: Option<Div>, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_col()
        .px_2()
        .border_b_1()
        .border_color(theme.border)
        .child(cells)
        .children(below)
}

/// The strip's column, `width` wide at the least: the strip, and the
/// table's spare width after it; only the spare width where the strip goes
/// under its row.
fn stretch(content: Option<(Pixels, impl IntoElement)>) -> Div {
    match content {
        Some((width, content)) => cell(width, false).flex_1().min_w(width).child(content),
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
    (beside, below): (Option<(Pixels, Div)>, Option<Div>),
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
    let cells = div()
        .flex()
        .items_center()
        .min_h(design::size::CONTROL + design::space::SM)
        .child(key)
        .children(address)
        .child(stretch(beside))
        .child(height)
        .child(cell(BEHIND_W, true).child(behind))
        .child(cell(STATUS_W, false).pl_4().child(status(
            index,
            &cells.status,
            view.answering(),
            theme,
        )));
    let id = ElementId::Name(format!("nodes-row-{index}").into());
    ruled(id, cells, below, theme).text_size(design::text::SECONDARY)
}

/// The last blocks, one mark each `gap` apart, oldest first: ink where
/// this key led, grey where another did, faint where the node names no
/// proposer; the tip framed. A resident leads none.
fn strip(node: &Node, head: u64, gap: f32, recent: &Recent, theme: &Theme) -> Div {
    if !node.validator {
        return div().text_color(theme.muted).child("Doesn't propose");
    }
    let marks = Recent::span(head).map(|height| {
        let fill = match recent.led(&node.key, height) {
            Some(true) => theme.foreground,
            Some(false) => theme.border,
            None => theme.surface,
        };
        let mark = div().w(px(MARK_W)).h(px(14.)).bg(fill);
        match height == head {
            true => div()
                .p(px(1.))
                .border_1()
                .border_color(theme.foreground)
                .child(mark),
            false => mark,
        }
    });
    div().flex().items_center().gap(px(gap)).children(marks)
}

/// The status as one word in its colours, grey while the node is silent
/// (the word is what it said then); where a Quiet validator stopped, as a
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
        Status::Quiet { since } => {
            let since = since.map(|since| caption(format!("since {}", design::grouped(since))));
            [badge(theme.warning, theme.warning_soft)]
                .into_iter()
                .chain(since)
                .collect()
        }
    };
    div().flex().items_center().gap_1().children(parts)
}
