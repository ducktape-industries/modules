//! Expo `app/(tabs)/explorer/block/[height].tsx`: one block over the root.
//! Its hash, parent, time, proposer and epoch, the steps to the blocks
//! either side, its transactions. Reads the block from the window, or the
//! one the node was asked for (`opened`), the validators and the head;
//! writes the route (a step, a row, a link).
use super::*;
use crate::decode::short;

pub(super) fn block(view: &Explorer, height: u64, cx: Cx, k: &Kit) -> AnyElement {
    let copy = copy_link(view, &Route::Block(height), cx, k);
    let page = over("explorer-block", words::block(height), view, cx, k);
    let page = match copy {
        Some(copy) => page.trailing(copy),
        None => page,
    };
    let body = match view.block(height) {
        Some((block, txs)) => found(view, &block, &txs, cx, k),
        None => missing(view, height, cx, k),
    };
    page.child(body).into_any_element()
}

/// A block not in hand: being read, refused, or not on this node (below
/// where its archive starts, or never finalized).
fn missing(view: &Explorer, height: u64, cx: Cx, k: &Kit) -> AnyElement {
    match &view.opened {
        Loadable::Ready(None) | Loadable::Reloading(None, _) => {
            let unlisted = view.chain.unlisted_below().filter(|from| height < *from);
            let why = match unlisted {
                Some(from) => unindexed(from),
                None => words::NO_BLOCK_HERE.into(),
            };
            let title = words::no_block(height);
            empty("explorer-no-block", "blocks", title, why, k)
        }
        Loadable::Failed(refusal) => refused(&refusal.message, cx, k),
        Loadable::Idle
        | Loadable::Loading(_)
        | Loadable::Ready(Some(_))
        | Loadable::Reloading(Some(_), _) => {
            quiet("explorer-block-loading", words::READING_BLOCK, k)
        }
    }
}

fn found(view: &Explorer, block: &BlockRow, txs: &[&TxRow], cx: Cx, k: &Kit) -> AnyElement {
    let now = view.chain.now();
    let list = if txs.is_empty() {
        quiet("explorer-block-empty", words::NO_BLOCK_TRANSACTIONS, k)
    } else {
        column(
            "explorer-block-txs",
            txs.iter().map(|tx| tx_line(view, tx, cx, k)).collect(),
        )
        .into_any_element()
    };
    scroller("explorer-page")
        .child(key_value("explorer-field-hash", "Hash", data(abi::hex(&block.id)), k))
        .child(key_value("explorer-field-parent", "Parent", parent(block, cx, k), k))
        .child(key_value("explorer-field-time", "Time", time_and_age(block.time, now, k), k))
        .children(proposer(view, block, k))
        .child(key_value("explorer-field-epoch", "Epoch", data(grouped(block.epoch)), k))
        .child(steps(view, block.height, cx, k))
        .child(section(
            "explorer-block-txs-heading",
            "Transactions",
            Some(aside(grouped(txs.len() as u64))),
            k,
        ))
        .child(list)
        .into_any_element()
}

/// The parent's height, a link, then its short id; the first block's
/// parent is its id alone.
fn parent(block: &BlockRow, cx: Cx, k: &Kit) -> Div {
    let id = short(&block.parent);
    let Some(previous) = block.height.checked_sub(1) else {
        return data(id);
    };
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .child(
            linked(
                "explorer-parent",
                grouped(previous),
                Route::Block(previous),
                cx,
                k,
            )
            .font_family(design::fonts::FAMILY_MONO),
        )
        .child(data(format!(" {id}")).text_color(k.theme.muted))
}

/// Who proposed the block: its place in the validator set, then its key,
/// faint.
fn proposer(view: &Explorer, block: &BlockRow, k: &Kit) -> Option<Stateful<Div>> {
    let key = block.proposer.as_ref()?;
    let place = view
        .validators
        .ready()
        .and_then(|keys| keys.iter().position(|seated| seated == key));
    let value = div()
        .flex()
        .flex_wrap()
        .child(data(format!("{} ", words::validator(place))))
        .child(data(words::validator_key(key)).text_color(k.theme.faint));
    Some(key_value("explorer-field-proposer", "Proposer", value, k))
}

/// The steps to the blocks either side (Expo's two small outline buttons):
/// each opens its block in this page's place; off at the first block and
/// at the head.
fn steps(view: &Explorer, height: u64, cx: Cx, k: &Kit) -> Div {
    let head = view.status.ready().map(|status| status.height);
    let previous = height.checked_sub(1);
    let next = head.filter(|head| height < *head).map(|_| height + 1);
    let step = |id: &'static str, label: String, to: Option<u64>| {
        let press = to.map(|to| -> Press {
            Box::new(
                cx.listener(move |view: &mut Explorer, _: &ClickEvent, _, cx| {
                    view.go(Route::Block(to), cx)
                }),
            )
        });
        small(id, label, design::Kind::Outline, press, k)
    };
    let back_label = words::step_back(previous);
    let on_label = words::step_on(height + 1);
    div()
        .flex()
        .gap(space::SM)
        .p(space::BLOCK)
        .child(step("explorer-previous", back_label, previous))
        .child(step("explorer-next", on_label, next))
}
