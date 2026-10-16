//! Expo `src/network/explorer/pages.tsx`: the root's pages under the strip,
//! Overview, Blocks, Transactions and Accounts (Programs is
//! [`programs`]), and the Transactions page one program's page opens
//! ([`txs`]). Reads the window (`chain`), the status, the validators, the
//! accounts and what activity recorded for each; writes nothing but the
//! route a row opens.
use super::*;

/// The rows each Overview section draws (Expo `pages.tsx:20` `LATEST`).
const LATEST: usize = 12;
/// The most lines a list page draws (Expo `src/network/window.ts`
/// `LIST_ROWS`).
const LIST_ROWS: usize = 50;

/// The root page's list under the strip.
pub(super) fn body(view: &Explorer, cx: Cx, k: &Kit) -> AnyElement {
    match view.root {
        Route::Blocks => chain_gate(view, cx, k).unwrap_or_else(|| blocks(view, cx, k)),
        Route::Transactions(_) => {
            chain_gate(view, cx, k).unwrap_or_else(|| transactions(view, None, cx, k))
        }
        Route::Accounts => accounts(view, cx, k),
        Route::Programs => programs::programs(view, cx, k),
        // the root is always a page's own list (`Explorer::go`)
        Route::Overview
        | Route::Block(_)
        | Route::Tx { .. }
        | Route::Account(_)
        | Route::Program(_) => chain_gate(view, cx, k).unwrap_or_else(|| overview(view, cx, k)),
    }
}

/// A window page before the window's first page lands (Expo `ChainGate`):
/// "Reading the chain…", or its refusal with Retry; `None` once it holds
/// a block.
pub(super) fn chain_gate(view: &Explorer, cx: Cx, k: &Kit) -> Option<AnyElement> {
    if !view.chain.blocks.is_empty() {
        return None;
    }
    Some(match &view.chain.failed {
        Some(sentence) => refused(sentence, cx, k),
        None => quiet("explorer-loading", "Reading the chain…", k),
    })
}

/// "the last 1,000 blocks": how far back the window reaches.
fn last_blocks(view: &Explorer) -> String {
    plural(view.chain.blocks.len() as u64, "block", "blocks")
}

/// The figures, the latest blocks and the latest transactions (Expo
/// `Overview`).
fn overview(view: &Explorer, cx: Cx, k: &Kit) -> AnyElement {
    let txs = match view.chain.txs.is_empty() {
        true => quiet(
            "explorer-no-txs",
            format!("No transactions in the last {}.", last_blocks(view)),
            k,
        ),
        false => column(
            "explorer-latest-txs-list",
            view.chain
                .txs
                .iter()
                .take(LATEST)
                .map(|tx| tx_line(view, tx, cx, k))
                .collect(),
        )
        .into_any_element(),
    };
    let all_blocks = more_link("explorer-all-blocks", "All blocks →", Route::Blocks, cx, k);
    let all_txs = more_link(
        "explorer-all-txs",
        "All transactions →",
        Route::Transactions(None),
        cx,
        k,
    );
    scroller("explorer-page")
        .child(facts("explorer-stats", figures(view), k))
        .child(section(
            "explorer-latest-blocks-heading",
            "Latest activity",
            Some(all_blocks),
            k,
        ))
        .child(column(
            "explorer-latest-blocks-list",
            block_lines(view, LATEST, cx, k),
        ))
        .child(section(
            "explorer-latest-txs-heading",
            "Latest transactions",
            Some(all_txs),
            k,
        ))
        .child(txs)
        .into_any_element()
}

/// The Overview's four figures: the height and the block time, the epoch
/// and the blocks to the next, the validators, the accounts; a dash for
/// one not read yet.
fn figures(view: &Explorer) -> Vec<Fact> {
    let status = view.status.ready();
    let dash = || "—".to_owned();
    let left = status
        .filter(|status| status.epoch_length > 0)
        .map(|status| ((status.epoch + 1) * status.epoch_length - 1).saturating_sub(status.height));
    vec![
        Fact {
            key: "height",
            label: "Height",
            value: status.map_or_else(dash, |status| grouped(status.height)),
            caption: status
                .map(|status| format!("Block every {:.1} s", status.block_time_ms as f64 / 1000.)),
        },
        Fact {
            key: "epoch",
            label: "Epoch",
            value: status
                .filter(|_| left.is_some())
                .map_or_else(dash, |status| grouped(status.epoch)),
            caption: left.map(|left| format!("Next in {}", plural(left, "block", "blocks"))),
        },
        Fact {
            key: "validators",
            label: "Validators",
            value: view
                .validators
                .ready()
                .map_or_else(dash, |keys| grouped(keys.len() as u64)),
            caption: None,
        },
        Fact {
            key: "accounts",
            label: "Accounts",
            value: view
                .accounts
                .ready()
                .map_or_else(dash, |accounts| grouped(accounts.list.len() as u64)),
            caption: None,
        },
    ]
}

/// The window's blocks, newest first, each run of empty ones folded (Expo
/// `Blocks`), and where the node's archive stops when it stops inside the
/// window.
fn blocks(view: &Explorer, cx: Cx, k: &Kit) -> AnyElement {
    let archive = view
        .chain
        .unlisted_below()
        .filter(|_| view.chain.complete)
        .map(|from| quiet("explorer-blocks-unindexed", unindexed(from), k));
    scroller("explorer-page")
        .child(section(
            "explorer-blocks-heading",
            "Blocks",
            Some(aside(format!("the last {}", last_blocks(view)))),
            k,
        ))
        .child(column(
            "explorer-blocks-list",
            block_lines(view, LIST_ROWS, cx, k),
        ))
        .children(archive)
        .into_any_element()
}

/// The window's transactions, newest first, or those sent to `program`
/// (Expo `Transactions`).
pub(super) fn transactions(view: &Explorer, program: Option<&str>, cx: Cx, k: &Kit) -> AnyElement {
    let matching: Vec<&TxRow> = view
        .chain
        .txs
        .iter()
        .filter(|tx| program.is_none_or(|program| tx.target == program))
        .collect();
    let title = match program {
        Some(program) => format!("Transactions · {program}"),
        None => "Transactions".to_owned(),
    };
    let count = format!(
        "{} in the last {}",
        plural(matching.len() as u64, "transaction", "transactions"),
        last_blocks(view)
    );
    let list = match matching.is_empty() {
        true => quiet(
            "explorer-no-txs",
            format!("No transactions in the last {}.", last_blocks(view)),
            k,
        ),
        false => column(
            "explorer-transactions-list",
            matching
                .into_iter()
                .take(LIST_ROWS)
                .map(|tx| tx_line(view, tx, cx, k))
                .collect(),
        )
        .into_any_element(),
    };
    scroller("explorer-page")
        .child(section(
            "explorer-transactions-heading",
            title,
            Some(aside(count)),
            k,
        ))
        .child(list)
        .into_any_element()
}

/// Every account identity lists (Expo `Accounts`, a `FlatList`): each with
/// its face, what it is and its devices, the transactions activity
/// recorded for it; only the rows on screen are drawn.
fn accounts(view: &Explorer, cx: Cx, k: &Kit) -> AnyElement {
    let listed = match &view.accounts {
        Loadable::Ready(listed) | Loadable::Reloading(listed, _) => listed,
        Loadable::Failed(refusal) => return refused(&refusal.message, cx, k),
        Loadable::Idle | Loadable::Loading(_) => {
            return quiet("explorer-accounts-loading", "Reading accounts…", k);
        }
    };
    let count = format!("{} · tx recorded", grouped(listed.list.len() as u64));
    let kit = *k;
    let rows = uniform_list(
        "explorer-accounts-list",
        listed.list.len(),
        cx.processor(move |view: &mut Explorer, range: Range<usize>, _, cx| {
            let Some(listed) = view.accounts.ready() else {
                return Vec::new();
            };
            range
                .filter_map(|index| listed.list.get(index))
                .map(|account| account_line(view, account, cx, &kit))
                .collect()
        }),
    )
    .flex_1()
    .min_h(px(0.));
    div()
        .id("explorer-page")
        .flex_1()
        .min_h(px(0.))
        .flex()
        .flex_col()
        .child(section(
            "explorer-accounts-heading",
            "Accounts",
            Some(aside(count)),
            k,
        ))
        .child(rows)
        .into_any_element()
}

/// An account (Expo `Accounts`' row): its face, its name, `#n · what it is
/// · its devices`, the transactions recorded for it.
fn account_line(view: &Explorer, account: &identity::Account, cx: Cx, k: &Kit) -> Stateful<Div> {
    let number = account.number;
    let sent = view
        .used_by(number)
        .map(|used| used.iter().map(|used| used.frames).sum::<u64>());
    let about = format!(
        "#{number} · {} · {}",
        crate::ui::accounts::kind(view, account),
        plural(account.keys().len() as u64, "device", "devices")
    );
    let row = list_row(SharedString::from(format!("explorer-account-{number}")), k);
    opens(row, account.card.name.clone(), Route::Account(number), cx)
        .child(design::lead(face(account, k.ladder.avatar, k)))
        .child(title_and_caption(
            div().truncate().child(account.card.name.clone()),
            caption(about, k),
        ))
        .child(tail(
            sent.map_or_else(|| "–".to_owned(), |sent| format!("{} tx", grouped(sent))),
            k,
        ))
        .child(chevron(k))
}
