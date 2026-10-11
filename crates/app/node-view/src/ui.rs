//! The Nodes sheet: the network's name and this node's pulse, its numbers
//! (height, last block, block time, the epoch), then the members table
//! (`table.rs`) in one of its four states. `render` reads the state and
//! changes nothing.
use abi::hex;
use ducktape_view_guest::prelude::*;
use ducktape_view_guest::{Div, FontWeight};

use crate::{Nodes, table};

pub(crate) fn render(view: &Nodes, cx: &mut Context<Nodes>) -> impl IntoElement {
    let theme = *cx.global::<Theme>();
    div()
        .id("nodes")
        .flex()
        .flex_col()
        .gap_3()
        .p(px(table::INSET))
        .size_full()
        .bg(theme.background)
        .text_color(theme.foreground)
        .text_size(design::text::BODY)
        .child(head(view, &theme))
        .child(numbers(view, cx, &theme))
        .child(
            div()
                .id("nodes-scroll")
                .flex_1()
                .min_h(px(0.))
                .overflow_scroll()
                .child(body(view, cx, &theme)),
        )
}

/// The network's name, and whether this node answers, with its key; the
/// latter on a row of its own where both do not fit.
fn head(view: &Nodes, theme: &Theme) -> impl IntoElement {
    let status = view.status.ready();
    let title = status.map_or("Nodes".into(), |status| status.chain_id.clone());
    div()
        .id("nodes-head")
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .child(design::heading("nodes-title", title, 1, theme).flex_1())
        .children(status.map(|status| pulse(view.answering(), status, theme)))
}

/// A dot and the word: In sync or Not answering, then this node's key.
fn pulse(answering: bool, status: &NodeStatus, theme: &Theme) -> impl IntoElement {
    let (word, color) = match answering {
        true => ("In sync", theme.success),
        false => ("Not answering", theme.danger),
    };
    div()
        .id("nodes-pulse")
        .flex()
        .items_center()
        .gap_2()
        .text_size(design::text::SECONDARY)
        .child(div().size_2().rounded_full().bg(color))
        .child(word)
        .child(div().text_color(theme.muted).child("· this node"))
        .child(design::mono(design::short_hex(&hex(&status.identity))).text_color(theme.muted))
}

/// This node's numbers, the epoch's progress, and when the next one starts,
/// wrapping onto a second row where they do not fit; or why they are not
/// here.
fn numbers(view: &Nodes, cx: &mut Context<Nodes>, theme: &Theme) -> AnyElement {
    let status = match &view.status {
        Loadable::Ready(status) | Loadable::Reloading(status, _) => status,
        Loadable::Failed(refusal) => {
            let retry = cx.listener(|view: &mut Nodes, _: &ClickEvent, _, cx| {
                view.status = Loadable::Idle;
                view.read_status(cx);
                cx.notify();
            });
            return design::refused("nodes-status", refusal.message.clone(), theme, retry)
                .into_any_element();
        }
        Loadable::Idle | Loadable::Loading(_) => {
            return design::quiet("Reading node status…", theme)
                .id("nodes-status-loading")
                .into_any_element();
        }
    };
    let age = view.ticks - view.moved;
    let last = match age {
        0 => "just now".to_owned(),
        seconds => format!("{} ago", design::ago(seconds * 1000, 0)),
    };
    let block_time = format!("{:.1} s", status.block_time_ms as f64 / 1000.);
    div()
        .id("nodes-status")
        .flex()
        .flex_wrap()
        .items_end()
        .gap_x_6()
        .gap_y_3()
        .pb_3()
        .border_b_1()
        .border_color(theme.border)
        .child(fact(
            "height",
            "Height",
            design::grouped(status.height),
            true,
            theme,
        ))
        .child(fact("last-block", "Last block", last, false, theme))
        .child(fact("block-time", "Block time", block_time, false, theme))
        .children(epoch(status, theme).into_iter().flatten())
        .into_any_element()
}

/// One of the node's numbers: its label over its value.
fn fact(key: &str, label: &str, value: String, mono: bool, theme: &Theme) -> impl IntoElement {
    let value = match mono {
        true => design::mono(value).text_size(design::text::SECTION),
        false => div().text_size(design::text::SECTION).child(value),
    };
    div()
        .id(ElementId::Name(format!("nodes-status-{key}").into()))
        .flex()
        .flex_col()
        .gap_1()
        .child(quiet_label(label, theme))
        .child(value)
}

fn quiet_label(label: impl Into<SharedString>, theme: &Theme) -> Div {
    div()
        .text_size(design::text::SECONDARY)
        .text_color(theme.muted)
        .child(label.into())
}

/// `Epoch 67 · 8 / 64` over a bar, and `Next epoch in 56 blocks` at the
/// right. A block closes epoch `e` when `(height + 1)` reaches `(e + 1)`
/// epochs, so the tip is `(height + 1) % length` blocks into its epoch.
fn epoch(status: &NodeStatus, theme: &Theme) -> Option<[AnyElement; 2]> {
    let length = status.epoch_length;
    if length == 0 {
        return None;
    }
    let into = (status.height + 1) % length;
    let share = into as f32 / length as f32;
    const BAR: f32 = 180.;
    let label = format!(
        "Epoch {} · {into} / {length}",
        design::grouped(status.epoch)
    );
    let progress = div()
        .id("nodes-status-epoch")
        .flex()
        .flex_col()
        .gap_2()
        .child(quiet_label(label, theme))
        .child(
            div()
                .w(px(BAR))
                .h(px(2.))
                .bg(theme.border)
                .child(div().h_full().w(px(BAR * share)).bg(theme.foreground)),
        )
        .into_any_element();
    let next = quiet_label(
        format!(
            "Next epoch in {}",
            design::plural(length - into, "block", "blocks")
        ),
        theme,
    )
    .id("nodes-status-next-epoch")
    .ml_auto()
    .into_any_element();
    Some([progress, next])
}

/// The four states of the members: loading, refused, empty, ready.
fn body(view: &Nodes, cx: &mut Context<Nodes>, theme: &Theme) -> AnyElement {
    match &view.nodes {
        Loadable::Idle | Loadable::Loading(_) => design::quiet("Reading the members…", theme)
            .id("nodes-loading")
            .into_any_element(),
        Loadable::Failed(refusal) => {
            let retry = cx.listener(|view: &mut Nodes, _: &ClickEvent, _, cx| view.read(cx));
            design::refused("nodes", refusal.message.clone(), theme, retry).into_any_element()
        }
        Loadable::Ready(nodes) | Loadable::Reloading(nodes, _) if nodes.is_empty() => {
            design::empty_state(
                "nodes-empty",
                "No members",
                "The validator set of this network is empty.",
                theme,
            )
            .into_any_element()
        }
        // the table reads the node's height: nothing until it answers,
        // and the numbers above say why
        Loadable::Ready(_) | Loadable::Reloading(_, _) if view.status.ready().is_none() => {
            div().into_any_element()
        }
        Loadable::Ready(nodes) | Loadable::Reloading(nodes, _) => {
            table::table(view, nodes, theme).into_any_element()
        }
    }
}

/// A group's or a section's heading, quieter than the title.
pub(crate) fn section(id: &'static str, label: String, theme: &Theme) -> impl IntoElement {
    div()
        .id(id)
        .role(Role::Heading)
        .aria_level(2)
        .h(design::size::CONTROL)
        .flex()
        .items_end()
        .pb_1()
        .px_2()
        .border_b_1()
        .border_color(theme.border)
        .text_size(design::text::SECONDARY)
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.muted)
        .child(label)
}
