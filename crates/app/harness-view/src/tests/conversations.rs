use super::*;

#[test]
fn the_manager_starts_or_forks_a_conversation_from_the_agent_page() {
    let (mut cx, _) = seated(Net::default(), EDDY);
    cx.host()
        .handle::<Submit<harness::Harness>>(|_| Ok(applied(view::methods::encode(&11u64))));
    cx.simulate_click("agent-12");
    cx.run_until_parked();
    assert!(cx.find("thread-7-current").is_some(), "{:?}", cx.texts());
    assert!(cx.find("thread-3-current").is_none());
    cx.simulate_click("agent-new-thread");
    cx.run_until_parked();
    cx.simulate_click("agent-fork-thread");
    cx.run_until_parked();
    assert_eq!(
        cx.host().requests::<Submit<harness::Harness>>(),
        [
            harness::Op::NewThread { agent: SCOUT },
            harness::Op::ForkThread { agent: SCOUT }
        ]
    );
}

#[test]
fn only_the_manager_sees_new_and_fork_and_no_current_conversation_shows_no_current_badge() {
    let (mut cx, _) = seated(Net::default(), ADA);
    cx.simulate_click("agent-12");
    cx.run_until_parked();
    assert!(cx.find("agent-new-thread").is_none());
    assert!(cx.find("agent-fork-thread").is_none());

    let mut net = Net::default();
    net.setups[0].current = None;
    let (mut cx, _) = seated(net, EDDY);
    cx.simulate_click("agent-12");
    cx.run_until_parked();
    assert!(cx.find("agent-new-thread").is_some());
    assert!(cx.find("thread-7-current").is_none());
}

#[test]
fn closing_a_conversation_says_whether_it_is_the_current_one() {
    let (mut cx, _) = seated(Net::default(), EDDY);
    cx.simulate_click("agent-12");
    cx.run_until_parked();
    cx.simulate_click("thread-3");
    cx.run_until_parked();
    assert!(
        cx.find("thread-close").is_none(),
        "a closed conversation closes no more"
    );

    let mut net = Net::default();
    net.threads[1].closed = false;
    let (mut cx, _) = seated(net, EDDY);
    cx.simulate_click("agent-12");
    cx.run_until_parked();
    cx.simulate_click("thread-3");
    cx.run_until_parked();
    cx.simulate_click("thread-close");
    cx.run_until_parked();
    assert!(
        cx.has_text(
            "Close this conversation? It is not the agent's current one, so nothing more reaches it."
        ),
        "{:?}",
        cx.texts()
    );
}
