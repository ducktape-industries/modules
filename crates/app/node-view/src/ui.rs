//! The Nodes screen: a header with the counts, the connected node's
//! status, then the set in one of its four states. `render` reads the
//! state and changes nothing.
use abi::hex;
use ducktape_view_guest::Loadable;
use ducktape_view_guest::design;
use ducktape_view_guest::prelude::*;
use ducktape_view_guest::{Div, FontWeight, Stateful};
use valset::{Membership, Role as Standing};

use crate::Nodes;

/// A row's label column.
const LABEL_W: Pixels = px(180.);
/// The widest the rows run: status, validators and members alike.
const ROWS_W: Pixels = px(720.);

pub(crate) fn render(view: &Nodes, cx: &mut Context<Nodes>) -> impl IntoElement {
    let theme = *cx.global::<Theme>();
    div()
        .id("nodes")
        .flex()
        .flex_col()
        .gap_3()
        .p_5()
        .size_full()
        .bg(theme.background)
        .text_color(theme.foreground)
        .text_size(design::text::BODY)
        .child(
            div()
                .id("nodes-head")
                .flex()
                .items_center()
                .gap_2()
                .child(design::heading("nodes-title", "Nodes", 1, &theme).flex_1())
                .child(
                    div()
                        .text_size(design::text::CAPTION)
                        .text_color(theme.muted)
                        .child(count(view)),
                ),
        )
        .child(
            div()
                .id("nodes-scroll")
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_2()
                .child(section("nodes-status-header", "Node", &theme))
                .child(status(view, cx, &theme))
                .child(body(view, cx, &theme)),
        )
}

/// The connected node's own numbers, one row each, or why they are not
/// here.
fn status(view: &Nodes, cx: &mut Context<Nodes>, theme: &Theme) -> AnyElement {
    match &view.status {
        Loadable::Ready(s) => div()
            .id("nodes-status")
            .max_w(ROWS_W)
            .flex()
            .flex_col()
            .children([
                fact("network", "Network", s.chain_id.clone(), false, theme),
                fact("height", "Height", design::grouped(s.height), true, theme),
                fact("epoch", "Epoch", design::grouped(s.epoch), true, theme),
                fact(
                    "block-time",
                    "Block time",
                    format!("{} ms", design::grouped(s.block_time_ms)),
                    true,
                    theme,
                ),
                fact("tip", "Tip", hex(&s.tip), true, theme),
                fact(
                    "identity",
                    "Node identity",
                    design::short_hex(&hex(&s.identity)),
                    true,
                    theme,
                ),
                fact(
                    "contract",
                    "Contract version",
                    s.contract.to_string(),
                    true,
                    theme,
                ),
            ])
            .into_any_element(),
        Loadable::Failed(refusal) => {
            let retry = cx.listener(|view: &mut Nodes, _: &ClickEvent, _, cx| view.read_status(cx));
            design::refused("nodes-status", refusal.message.clone(), theme, retry)
                .into_any_element()
        }
        Loadable::Idle | Loadable::Loading(_) => design::quiet("Reading node status…", theme)
            .id("nodes-status-loading")
            .pl_2()
            .into_any_element(),
    }
}

/// One status row: the label, and the value (mono for numbers and hashes).
fn fact(key: &str, label: &str, value: String, mono: bool, theme: &Theme) -> AnyElement {
    let value = match mono {
        true => design::mono(value).truncate().into_any_element(),
        false => div().child(value).into_any_element(),
    };
    row(
        format!("nodes-status-{key}").into(),
        div().text_color(theme.muted).child(label.to_owned()),
        value,
        theme,
    )
    .into_any_element()
}

/// The one row every block of this screen uses: a label column, the value
/// beside it, a hairline under.
fn row(
    id: ElementId,
    label: impl IntoElement,
    value: impl IntoElement,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_2()
        .min_h(design::size::CONTROL + design::space::SM)
        .pl_2()
        .pr_2()
        .border_b_1()
        .border_color(theme.border)
        .child(div().w(LABEL_W).flex_none().min_w(px(0.)).child(label))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .items_center()
                .gap_2()
                .child(value),
        )
}

fn count(view: &Nodes) -> String {
    match view.set.ready() {
        Some(set) => format!(
            "{} · {}",
            design::plural(set.validators.len() as u64, "validator", "validators"),
            design::plural(set.members.len() as u64, "member", "members"),
        ),
        None => String::new(),
    }
}

/// The four states of the set: loading, refused, empty, ready.
fn body(view: &Nodes, cx: &mut Context<Nodes>, theme: &Theme) -> AnyElement {
    match &view.set {
        Loadable::Idle | Loadable::Loading(_) => design::quiet("Reading the validator set…", theme)
            .id("nodes-loading")
            .into_any_element(),
        Loadable::Failed(refusal) => {
            let retry = cx.listener(|view: &mut Nodes, _: &ClickEvent, _, cx| view.read(cx));
            design::refused("nodes", refusal.message.clone(), theme, retry).into_any_element()
        }
        Loadable::Ready(set) if set.members.is_empty() && set.validators.is_empty() => {
            design::empty_state(
                "nodes-empty",
                "No members",
                "The validator set of this network is empty.",
                theme,
            )
            .into_any_element()
        }
        Loadable::Ready(set) => div()
            .id("nodes-list")
            .flex()
            .flex_col()
            .gap_2()
            .child(section("nodes-set-header", "Validator set", theme))
            .child(validators(&set.validators, theme))
            .child(section("nodes-members-header", "Memberships", theme))
            .child(members(&set.members, theme))
            .into_any_element(),
    }
}

/// A section's heading, quieter than the title.
fn section(id: &'static str, label: &'static str, theme: &Theme) -> impl IntoElement {
    div()
        .id(id)
        .role(Role::Heading)
        .aria_level(2)
        .h(design::size::CONTROL)
        .flex()
        .items_center()
        .pl_2()
        .pr_1()
        .text_size(design::text::SECONDARY)
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.muted)
        .child(label)
}

fn validators(validators: &[Vec<u8>], theme: &Theme) -> AnyElement {
    if validators.is_empty() {
        return design::quiet("No key validates on this network.", theme)
            .id("nodes-no-validators")
            .into_any_element();
    }
    div()
        .id("nodes-validators")
        .max_w(ROWS_W)
        .flex()
        .flex_col()
        .children(validators.iter().enumerate().map(|(index, key)| {
            row(
                ElementId::named_usize("nodes-validator", index),
                div()
                    .text_color(theme.muted)
                    .child(format!("Validator {}", index + 1)),
                key_text(key),
                theme,
            )
        }))
        .into_any_element()
}

fn members(members: &[Membership], theme: &Theme) -> impl IntoElement {
    div()
        .id("nodes-members")
        .max_w(ROWS_W)
        .flex()
        .flex_col()
        .children(members.iter().enumerate().map(|(index, member)| {
            row(
                ElementId::named_usize("nodes-member", index),
                key_text(&member.key),
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        design::mono(member.address.clone())
                            .flex_1()
                            .min_w(px(0.))
                            .truncate(),
                    )
                    .child(standing(index, member.role, theme)),
                theme,
            )
        }))
}

/// A key, shortened: never raw.
fn key_text(key: &[u8]) -> Div {
    div()
        .font_family(design::fonts::FAMILY_MONO)
        .text_size(design::text::SECONDARY)
        .child(design::short_hex(&hex(key)))
}

/// Whether a member validates or only resides, as a tag.
fn standing(index: usize, role: Standing, theme: &Theme) -> impl IntoElement {
    let (label, foreground, background) = match role {
        Standing::Validator => ("Validator", theme.success, theme.success_soft),
        Standing::Resident => ("Resident", theme.muted, theme.surface_raised),
    };
    design::badge(
        ElementId::named_usize("nodes-standing", index),
        label,
        foreground,
        background,
    )
}
