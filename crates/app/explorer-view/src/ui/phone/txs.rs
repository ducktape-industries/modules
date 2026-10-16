//! Expo `app/(tabs)/explorer/txs/[name].tsx` (mobile dev 266633c): one
//! program's transactions in the window, over the root; the program as the
//! title, "Transactions" under it. Reads the window; writes the route (a
//! row).
use super::*;

pub(super) fn transactions(view: &Explorer, program: &str, cx: Cx, k: &Kit) -> AnyElement {
    let list = pages::chain_gate(view, cx, k)
        .unwrap_or_else(|| pages::transactions(view, Some(program), cx, k));
    over(
        "explorer-program-transactions",
        program.to_owned(),
        view,
        cx,
        k,
    )
    .caption("Transactions")
    .child(list)
    .into_any_element()
}
