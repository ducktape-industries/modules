//! Chat over the fake node: the chain seeded through chat's own ops, what
//! the person sees read off it, and a post through the composer landing on
//! the chain and coming back through `module.changes`.
use chat::{Block, Op, PostPolicy, Query, Reply};
use chat_view::Chat;
use ducktape_view_guest::live::Network;
use ducktape_view_guest::methods::{HostId, HostSession, HostVisible, StoreGet};
use ducktape_view_guest::testing::TestAppContext;

fn post(net: &Network, who: u64, id: &str, text: &str) {
    net.submit(
        who,
        chat::MODULE,
        &Op::PostMessage {
            channel_id: "general".into(),
            message_id: id.into(),
            blocks: vec![Block::paragraph(text)],
            thread: None,
        },
    )
    .unwrap();
}

#[test]
fn a_seeded_room_is_read_off_the_chain_and_a_post_comes_back_through_it() {
    let net = Network::new();
    net.seat::<chat::Chat>(chat::MODULE);
    let ada = net.person("ada");
    net.submit(
        ada,
        chat::MODULE,
        &Op::CreateChannel {
            channel_id: "general".into(),
            name: "General".into(),
            post_policy: PostPolicy::Open,
        },
    )
    .unwrap();
    post(&net, ada, "m1", "hello from ada");
    post(&net, net.me(), "m2", "and from me");

    let mut cx = TestAppContext::new();
    net.back(&cx.host());
    // the host's own methods stay the test's: no store, ids minted here
    cx.host().handle::<StoreGet>(|_| Ok(None));
    let mut minted = 0;
    cx.host().handle::<HostId>(move |prefix| {
        minted += 1;
        Ok(format!("{prefix}-{minted}"))
    });
    let session = cx.host().stream::<HostSession>();
    let visible = cx.host().stream::<HostVisible>();
    cx.open::<Chat>();
    cx.run_until_parked();
    session.send(net.session());
    visible.send(true);
    cx.run_until_parked();
    assert!(cx.has_text("General"), "{:?}", cx.texts());
    cx.simulate_click("chat-sidebar-channel-general");
    cx.run_until_parked();
    assert!(cx.has_text("hello from ada"), "{:?}", cx.texts());
    assert!(cx.has_text("and from me"));
    assert!(cx.has_text("ada"), "the author's name is identity's");

    // a post through the composer: chat's rules run it, the block's
    // changes reach the view, and it re-reads the room
    cx.simulate_input("draft-general/editor", "typed in the live test");
    cx.simulate_click("draft-general/send");
    cx.run_until_parked();
    assert!(cx.has_text("typed in the live test"), "{:?}", cx.texts());
    let Reply::Roots(page) = net
        .query(
            chat::MODULE,
            &Query::Roots {
                channel_id: "general".into(),
                viewer: vec![],
                page: chat::PageRequest {
                    after: None,
                    limit: None,
                },
            },
        )
        .unwrap()
    else {
        panic!("roots")
    };
    assert_eq!(page.items.len(), 3);
    assert_eq!(page.items[0].text, "typed in the live test");
    assert_eq!(page.items[0].author, chat::Principal::Account(net.me()));
}
