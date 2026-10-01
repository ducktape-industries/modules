//! hx/b2 measurement: how much of a Transactions frame is style.
use super::*;
use ducktape_view_guest::testing::b2_report as report;

#[test]
fn b2_explorer_frames() {
    let mut cx = TestAppContext::new();
    heavy(&mut cx);
    cx.open::<Explorer>();
    cx.run_until_parked();
    report("explorer-overview", cx.root());
    cx.simulate_click("explorer-tab-transactions");
    cx.run_until_parked();
    assert!(cx.find("explorer-transactions").is_some(), "{:?}", cx.texts());
    report("explorer-transactions", cx.root());
    cx.simulate_click("explorer-tab-blocks");
    cx.run_until_parked();
    report("explorer-blocks", cx.root());
}
