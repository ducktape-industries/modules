//! The Blocks tab and one block.
use super::*;
use view::design::Ladder;

pub(super) fn blocks(view: &Explorer, cx: Cx, theme: &Theme) -> AnyElement {
    let held = view.chain.blocks.len() as u64;
    let list = rows("explorer-blocks-list", "Blocks", view);
    let list = block_lines(list, view, view.chain.blocks.len());
    let unlisted = view
        .chain
        .unlisted_below()
        .filter(|_| view.chain.complete)
        .map(|from| quiet("explorer-blocks-unindexed", unindexed(from), theme));
    div()
        .id("explorer-blocks")
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .child(
            heading(
                "explorer-blocks-heading",
                "Blocks",
                Some(caption(words::last_blocks(held), theme)),
                theme,
            )
            .mr(design::size::SCROLLBAR),
        )
        .child(list.build(cx))
        .children(unlisted)
        .into_any_element()
}

pub(super) fn block(
    view: &Explorer,
    height: u64,
    cx: Cx,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let Some((block, txs)) = view.block(height) else {
        return match &view.opened {
            Loadable::Ready(None) | Loadable::Reloading(None, _) => {
                let unlisted = view.chain.unlisted_below().filter(|from| height < *from);
                let why = match unlisted {
                    Some(from) => unindexed(from),
                    None => words::NO_BLOCK_HERE.into(),
                };
                empty_state("explorer-no-block", words::no_block(height), why, theme)
                    .into_any_element()
            }
            Loadable::Failed(refusal) => failed(&refusal.message, cx, theme, ladder),
            Loadable::Idle
            | Loadable::Loading(_)
            | Loadable::Ready(Some(_))
            | Loadable::Reloading(Some(_), _) => {
                quiet("explorer-block-loading", words::READING_BLOCK, theme)
            }
        };
    };
    let now = view.chain.now();
    let count = txs.len() as u64;
    let empty = txs
        .is_empty()
        .then(|| quiet("explorer-block-empty", words::NO_BLOCK_TRANSACTIONS, theme));
    let list = rows("explorer-block-txs", "Transactions", view);
    let rows = (!txs.is_empty()).then(|| {
        tx_rows(list, view, move |view| {
            view.block(height).map_or_else(Vec::new, |(_, txs)| txs)
        })
        .whole(view, cx, theme)
        .build(cx)
    });
    div()
        .id("explorer-block")
        .child(title(view, height, cx, theme))
        .child(field("Hash", mono(abi::hex(&block.id)), theme))
        .child(field("Parent", parent(&block, cx, theme), theme))
        .child(field(
            "Time",
            div()
                .flex()
                .gap_2()
                .child(date(block.time))
                .child(mono(words::age(now, block.time)).text_color(theme.faint)),
            theme,
        ))
        .children(proposer(view, &block, theme))
        .child(field("Epoch", mono(grouped(block.epoch)), theme))
        .child(heading(
            "explorer-block-txs-heading",
            "Transactions",
            Some(caption(grouped(count), theme)),
            theme,
        ))
        .children(rows)
        .children(empty)
        .into_any_element()
}

/// The block's height, its link, and the steps to the blocks either side.
fn title(view: &Explorer, height: u64, cx: Cx, theme: &Theme) -> impl IntoElement {
    let head = view.status.ready().map_or(0, |status| status.height);
    let previous = height.checked_sub(1);
    let next = (height < head).then_some(height + 1);
    let back = words::step_back(previous);
    let on = words::step_on(height + 1);
    div()
        .flex()
        .items_center()
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .flex_1()
                .child(titled("Block", grouped(height), theme)),
        )
        .child(
            div()
                .flex()
                .gap_2()
                .px_5()
                .children(copy_button(view, &Route::Block(height), cx, theme))
                .child(step("explorer-previous", back, previous, cx, theme))
                .child(step("explorer-next", on, next, cx, theme)),
        )
}

/// A step to the block at `to`; none there, it stands disabled.
fn step(
    id: &'static str,
    text: String,
    to: Option<u64>,
    cx: Cx,
    theme: &Theme,
) -> impl IntoElement {
    let button = div()
        .id(id)
        .px_3()
        .h(design::size::CONTROL)
        .flex()
        .items_center()
        .border_1()
        .border_color(theme.border)
        .text_size(design::text::SECONDARY)
        .text_color(if to.is_some() {
            theme.foreground
        } else {
            theme.faint
        })
        .role(Role::Button)
        .child(text);
    match to {
        Some(to) => {
            let go = cx.listener(move |view: &mut Explorer, _: &ClickEvent, _, cx| {
                view.go(Route::Block(to), cx)
            });
            let hover = theme.hover;
            button.focusable().hover(move |s| s.bg(hover)).on_click(go)
        }
        None => button.aria_disabled(true),
    }
}

/// The parent's height, a link, then its hash.
fn parent(block: &BlockRow, cx: Cx, theme: &Theme) -> impl IntoElement {
    let hash = mono(short(&block.parent)).text_color(theme.muted);
    let Some(previous) = block.height.checked_sub(1) else {
        return div().child(hash);
    };
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            link(
                "explorer-parent".into(),
                grouped(previous),
                Route::Block(previous),
                cx,
                theme,
            )
            .font_family(design::fonts::FAMILY_MONO)
            .text_size(design::text::SECONDARY),
        )
        .child(hash)
}

/// Who proposed the block: its place in the validator set, then its key.
fn proposer(view: &Explorer, block: &BlockRow, theme: &Theme) -> Option<impl IntoElement> {
    let key = block.proposer.as_ref()?;
    let place = view
        .validators
        .ready()
        .and_then(|keys| keys.iter().position(|seated| seated == key));
    Some(field(
        "Proposer",
        div()
            .flex()
            .gap_2()
            .child(mono(words::validator(place)))
            .child(mono(words::validator_key(key)).text_color(theme.faint)),
        theme,
    ))
}
