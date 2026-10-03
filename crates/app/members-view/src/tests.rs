use super::*;
use abi::Scheme;
use ducktape_view_guest::methods::{
    Block, BlockPage, ChainBlocks, Description, ModuleDescribe, Session, Tx,
};
use ducktape_view_guest::testing::{StreamSender, TestAppContext};
use ducktape_view_guest::wire::{ContainerNode, Node, TextNode};
use ducktape_view_guest::{Hsla, StyleRefinement, Styled, Theme};

// the list and the detail fit at 320, the desk's smallest window
#[test]
fn the_view_is_laid_out_from_320() {
    assert_eq!(<Members as View>::MIN_WINDOW_WIDTH, 320);
}

#[test]
fn the_root_tracks_the_shared_theme() {
    let (mut cx, _) = ready();
    let dark = Theme::dark();
    cx.set_global(dark);
    let Some(Node::Container(ContainerNode { style, .. })) = cx.find("members") else {
        panic!("members root is a styled container");
    };
    assert_eq!(
        style
            .background
            .as_ref()
            .and_then(|fill| fill.color())
            .and_then(|background| background.as_solid()),
        Some(dark.background)
    );
    assert_eq!(style.text.color, Some(dark.foreground));
}

fn account(number: u64, name: &str, control: identity::Control) -> identity::Account {
    identity::Account {
        number,
        card: identity::Card {
            name: name.into(),
            avatar: None,
            bio: Some(format!("{name}'s bio")),
            updated_at: 0,
        },
        control,
    }
}

fn key(key: &[u8], label: &str) -> identity::Key {
    identity::Key {
        scheme: Scheme::Ed25519,
        key: key.to_vec(),
        label: Some(label.into()),
        added_at: 0,
    }
}

fn person(number: u64, name: &str, keys: Vec<identity::Key>) -> identity::Account {
    account(number, name, identity::Control::Person { keys })
}

fn module(number: u64, name: &str) -> identity::Account {
    account(
        number,
        name,
        identity::Control::Module {
            module: name.into(),
        },
    )
}

fn agent(number: u64, name: &str, manager: u64, life: identity::Life) -> identity::Account {
    account(
        number,
        name,
        identity::Control::Managed {
            manager,
            category: identity::Category::Agent,
            life,
        },
    )
}

fn page<T>(items: Vec<T>) -> module_registry::PageResponse<T> {
    module_registry::PageResponse {
        height: 1,
        items,
        next: None,
    }
}

const EDDY: &[u8] = b"\x01\x02";
const SCOUT: &[u8] = b"\x09\x09";

/// eddy (#7, the reader, a validator) manages scout (#9, active) and relay
/// (#10, revoked); chat (#8) is a module; ada (#11) is another person.
fn respond(cx: &mut TestAppContext) {
    cx.host().handle::<Query<Identity>>(|query| {
        assert!(matches!(query, identity::Query::List { .. }));
        Ok(identity::Reply::Accounts(page(vec![
            module(8, "chat"),
            person(7, "eddy", vec![key(EDDY, "laptop")]),
            agent(
                9,
                "scout",
                7,
                identity::Life::Active {
                    keys: vec![key(SCOUT, "sandbox")],
                },
            ),
            agent(10, "relay", 7, identity::Life::Revoked),
            person(11, "ada", vec![key(b"\x0b", "phone")]),
        ])))
    });
    cx.host().handle::<Query<Valset>>(|query| {
        assert!(matches!(query, valset::Query::Memberships { .. }));
        Ok(valset::Reply::Memberships(page(vec![valset::Membership {
            key: EDDY.to_vec(),
            address: "10.0.0.1:4000".into(),
            role: valset::Role::Validator,
        }])))
    });
    // two blocks: scout posted at 12, eddy at 11 and 12
    cx.host().handle::<ChainBlocks>(|page: BlockPage| {
        let tx = |signer: &[u8], seq| Tx {
            signer: signer.to_vec(),
            seq,
            target: "chat".into(),
            payload: vec![seq as u8],
            ..Tx::default()
        };
        let block = |height, txs| Block {
            height,
            time: height * 60_000,
            txs,
            ..Block::default()
        };
        Ok(match page.before {
            None => vec![
                block(12, vec![tx(EDDY, 2), tx(SCOUT, 5)]),
                block(11, vec![tx(EDDY, 1)]),
            ],
            Some(_) => Vec::new(),
        })
    });
    cx.host().handle::<ModuleDescribe>(|(target, payload)| {
        Ok(Some(Description {
            title: format!("Post {} in {target}", payload[0]),
            fields: Vec::new(),
        }))
    });
}

fn ready() -> (TestAppContext, StreamSender<Changes<Identity>>) {
    let mut cx = TestAppContext::new();
    let session = cx.host().stream::<HostSession>();
    let feed = cx.host().stream::<Changes<Identity>>();
    cx.host()
        .never::<ducktape_view_guest::methods::HostOffset>();
    respond(&mut cx);
    cx.open::<Members>();
    session.send(Session {
        account: Some(7),
        chain_id: "testnet#0a1b2c3d".into(),
        ..Session::default()
    });
    cx.run_until_parked();
    (cx, feed)
}

/// The colour `text` is drawn in under `key`, inherited down the tree.
fn color_of(cx: &TestAppContext, key: &str, text: &str) -> Option<Hsla> {
    fn walk(node: &Node, text: &str, color: Option<Hsla>) -> Option<Option<Hsla>> {
        match node {
            Node::Text(TextNode { content, style, .. }) if content == text => {
                Some(style.text.color.or(color))
            }
            Node::Container(ContainerNode {
                style, children, ..
            }) => {
                let color = style.text.color.or(color);
                children.iter().find_map(|child| walk(child, text, color))
            }
            _ => None,
        }
    }
    walk(cx.find(key)?, text, None).flatten()
}

#[test]
fn the_list_groups_people_then_agents_then_modules_and_chooses_no_one() {
    let (cx, _) = ready();
    let texts = cx.texts();
    // the last of each, past the chips of the same names
    let at = |text: &str| texts.iter().rposition(|t| t == text).unwrap();
    assert!(at("People") < at("eddy") && at("eddy") < at("ada"));
    assert!(at("ada") < at("Agents") && at("Agents") < at("scout"));
    assert!(at("relay") < at("Modules") && at("Modules") < at("chat"));
    for text in [
        "5 accounts",
        "you",
        "Person",
        "Module",
        "Agent · managed by eddy",
        "Agent · managed by eddy · revoked",
        "Choose a member",
    ] {
        assert!(cx.has_text(text), "{text}: {texts:?}");
    }
    // nothing chosen, nothing read of the chain
    assert!(cx.host().requests::<ChainBlocks>().is_empty());
    assert_eq!(cx.host().requests::<Changes<Identity>>().len(), 1);
}

#[test]
fn choosing_a_person_shows_their_devices_the_agents_they_manage_and_their_activity() {
    let (mut cx, _) = ready();
    cx.simulate_click("members-row-7");
    cx.run_until_parked();
    for text in [
        "account 7",
        "1 device",
        "Validator",
        "eddy's bio",
        "Devices",
        "laptop",
        "0102",
        "Manages",
        "account 9 · Agent",
        "active",
        "revoked",
        "Recent activity",
        "last 1,000 blocks",
        "Post 2 in chat",
        "Post 1 in chat",
        "block 12",
        "block 11",
        "1m",
    ] {
        assert!(cx.has_text(text), "{text}: {:?}", cx.texts());
    }
    // scout's post is scout's, not eddy's
    assert!(!cx.has_text("Post 5 in chat"));
    // the reader's own account has no one to DM
    assert!(cx.find("members-open-dm").is_none());
    cx.simulate_click("members-explorer");
    assert_eq!(
        cx.host().opened_links(),
        vec!["duck://explorer/account/7".to_owned()]
    );
    // a managed agent is chosen in place
    cx.simulate_click("members-manages-9");
    cx.run_until_parked();
    assert!(cx.has_text("scout's bio"));
}

#[test]
fn an_agent_names_its_manager_its_standing_and_its_devices() {
    let (mut cx, _) = ready();
    cx.simulate_click("members-row-9");
    cx.run_until_parked();
    for text in [
        "account 9",
        "Agent · managed by",
        "eddy",
        "active",
        "1 device",
        "sandbox",
        "Post 5 in chat",
        "Suspend, revoke, rename and keys live in Account → Agents, for the manager.",
    ] {
        assert!(cx.has_text(text), "{text}: {:?}", cx.texts());
    }
    assert!(!cx.has_text("Post 1 in chat") && !cx.has_text("Post 2 in chat"));
    cx.simulate_click("members-open-dm");
    assert_eq!(
        cx.host().opened_links(),
        vec![ducklink::mint("testnet#0a1b2c3d", "chat", &["dm-7-9"]).unwrap()]
    );
    // the manager's name chooses the manager
    cx.simulate_click("members-manager");
    cx.run_until_parked();
    assert!(cx.has_text("eddy's bio"));
}

#[test]
fn a_module_has_no_devices_and_no_one_to_dm() {
    let (mut cx, _) = ready();
    cx.simulate_click("members-row-8");
    cx.run_until_parked();
    assert!(cx.has_text("Module") && !cx.has_text("Module · chat"));
    assert!(cx.has_text("No devices"));
    assert!(!cx.has_text("Recent activity") && !cx.has_text("1 device"));
    assert!(cx.find("members-open-dm").is_none());
    assert!(cx.find("members-explorer").is_some());
    assert!(cx.host().requests::<ChainBlocks>().is_empty());
}

#[test]
fn a_revoked_agent_stays_listed_dimmed() {
    let (cx, _) = ready();
    let theme = Theme::light();
    assert_eq!(color_of(&cx, "members-row-10", "relay"), Some(theme.faint));
    assert_ne!(color_of(&cx, "members-row-9", "scout"), Some(theme.faint));
}

#[test]
fn the_filter_and_the_chips_narrow_together_and_keep_the_choice() {
    let (mut cx, _) = ready();
    cx.simulate_click("members-row-9");
    cx.run_until_parked();
    let reads = cx.host().requests::<Query<Identity>>().len();
    // Modules: only chat
    cx.simulate_click("members-chip-3");
    assert!(cx.find("members-row-8").is_some() && cx.find("members-row-7").is_none());
    // and a filter on top: nothing is both a module and "ed"
    cx.simulate_input("members-filter", "ed");
    assert!(cx.has_text("Nothing matches"));
    // People and "ed": eddy alone
    cx.simulate_click("members-chip-1");
    assert!(cx.find("members-row-7").is_some() && cx.find("members-row-11").is_none());
    // the chosen agent is hidden from the list, not from the detail
    assert!(cx.has_text("scout's bio"));
    cx.simulate_click("members-chip-0");
    cx.simulate_input("members-filter", "");
    assert!(cx.find("members-row-11").is_some());
    assert!(cx.has_text("scout's bio"));
    assert_eq!(cx.host().requests::<Query<Identity>>().len(), reads);
}

#[test]
fn loading_waits_for_the_host() {
    let mut cx = TestAppContext::new();
    cx.host().stream::<HostSession>();
    cx.host().stream::<Changes<Identity>>();
    cx.host()
        .never::<ducktape_view_guest::methods::HostOffset>();
    cx.host().never::<Query<Identity>>();
    cx.open::<Members>();
    cx.run_until_parked();
    assert!(cx.has_text("Reading the roster…"));
}

#[test]
fn a_roster_with_nobody_in_it_says_so() {
    let mut cx = TestAppContext::new();
    cx.host().stream::<HostSession>();
    cx.host().stream::<Changes<Identity>>();
    cx.host()
        .never::<ducktape_view_guest::methods::HostOffset>();
    cx.host()
        .handle::<Query<Identity>>(|_| Ok(identity::Reply::Accounts(page(vec![]))));
    cx.host()
        .handle::<Query<Valset>>(|_| Ok(valset::Reply::Memberships(page(vec![]))));
    cx.open::<Members>();
    cx.run_until_parked();
    assert!(cx.has_text("No accounts"));
}

#[test]
fn a_refusal_shows_its_sentence_and_retry_asks_again() {
    let mut cx = TestAppContext::new();
    cx.host().stream::<HostSession>();
    cx.host().stream::<Changes<Identity>>();
    cx.host()
        .never::<ducktape_view_guest::methods::HostOffset>();
    cx.host()
        .refuse::<Query<Identity>>("unavailable", "identity is not running here");
    cx.open::<Members>();
    cx.run_until_parked();
    assert!(cx.has_text("identity is not running here"));
    respond(&mut cx);
    cx.simulate_click("members-retry");
    cx.run_until_parked();
    assert!(cx.has_text("eddy"));
    assert_eq!(cx.host().requests::<Query<Identity>>().len(), 2);
}

#[test]
fn a_live_bump_re_reads_and_a_snapshot_restores_the_choice() {
    let (mut cx, feed) = ready();
    cx.simulate_click("members-row-9");
    cx.run_until_parked();
    cx.host()
        .refuse::<Query<Identity>>("unavailable", "refresh temporarily unavailable");
    feed.send(None);
    cx.run_until_parked();
    assert!(cx.has_text("scout's bio"));
    assert_eq!(cx.host().requests::<Query<Identity>>().len(), 2);
    cx.host().handle::<Query<Identity>>(|_| {
        Ok(identity::Reply::Accounts(page(vec![
            person(7, "eddy", vec![key(EDDY, "laptop")]),
            agent(
                9,
                "scout",
                7,
                identity::Life::Suspended {
                    keys: vec![key(SCOUT, "sandbox")],
                },
            ),
        ])))
    });
    feed.send(None);
    cx.run_until_parked();
    assert!(cx.has_text("suspended") && !cx.has_text("ada"));
    cx.simulate_input("members-filter", "sc");
    cx.simulate_click("members-chip-2");
    let bytes = cx.snapshot().unwrap();
    let mut restored = TestAppContext::new();
    restored.host().stream::<HostSession>();
    restored.host().stream::<Changes<Identity>>();
    restored
        .host()
        .never::<ducktape_view_guest::methods::HostOffset>();
    restored.host().never::<Query<Identity>>();
    restored.host().never::<ChainBlocks>();
    let view = restored.restore::<Members>(&bytes).unwrap();
    restored.run_until_parked();
    assert!(restored.has_text("scout's bio"));
    view.read(|view| {
        assert_eq!(view.filter, "sc");
        assert_eq!(view.only, Some(Group::Agents));
        assert_eq!(view.selected, Some(9));
    });
    assert_eq!(restored.host().requests::<Query<Identity>>().len(), 1);
    // the activity is read again for the restored choice
    assert_eq!(restored.host().requests::<ChainBlocks>().len(), 1);
}

/// A block that changed no account: identity's head re-reads the roster,
/// the same rows land, and the view draws nothing for either.
#[test]
fn a_live_bump_that_lands_the_same_roster_draws_nothing() {
    let (mut cx, feed) = ready();
    cx.simulate_click("members-row-9");
    cx.run_until_parked();
    let (asked, renders) = (cx.host().requests::<Query<Identity>>().len(), cx.renders());
    feed.send(Some(13));
    cx.run_until_parked();
    assert_eq!(cx.host().requests::<Query<Identity>>().len(), asked + 1);
    assert_eq!(cx.renders(), renders, "the same roster drew nothing");
    assert!(cx.has_text("scout's bio"));
}

/// The detail's activity read again lands what it shows: nothing draws.
#[test]
fn an_activity_read_anew_that_lands_the_same_draws_nothing() {
    let mut cx = TestAppContext::new();
    let session = cx.host().stream::<HostSession>();
    cx.host().stream::<Changes<Identity>>();
    cx.host()
        .never::<ducktape_view_guest::methods::HostOffset>();
    respond(&mut cx);
    let view = cx.open::<Members>();
    session.send(Session {
        account: Some(7),
        chain_id: "testnet#0a1b2c3d".into(),
        ..Session::default()
    });
    cx.run_until_parked();
    cx.simulate_click("members-row-7");
    cx.run_until_parked();
    assert!(cx.has_text("block 12"), "{:?}", cx.texts());
    let (asked, renders) = (cx.host().requests::<ChainBlocks>().len(), cx.renders());
    view.update(&mut cx, |members, _, cx| members.read_activity(cx));
    cx.run_until_parked();
    assert!(
        cx.host().requests::<ChainBlocks>().len() > asked,
        "read anew"
    );
    assert_eq!(cx.renders(), renders, "the same activity drew nothing");
}

#[test]
fn the_list_and_the_detail_are_accessible() {
    let (mut cx, _) = ready();
    cx.simulate_click("members-row-9");
    cx.run_until_parked();
}

/// Nothing selected, the list still has an active row — its first shown —
/// so Tab lands on a row, not on the box; End selects the last shown row.
#[test]
fn the_first_row_is_active_before_a_choice_and_end_selects_the_last() {
    let (mut cx, _) = ready();
    let list = cx.interactivity("members-list");
    assert!(list.focusable && list.tab_stop == Some(true));
    let rows: Vec<String> = cx
        .find("members-list")
        .expect("the list")
        .children()
        .iter()
        .filter_map(|node| node.key())
        .filter(|key| key.starts_with("members-row-"))
        .map(str::to_owned)
        .collect();
    let first = cx.interactivity(&rows[0]);
    assert!(first.aria.active_descendant);
    assert_eq!(first.aria.selected, Some(false));
    assert!(
        rows[1..]
            .iter()
            .all(|row| !cx.interactivity(row).aria.active_descendant)
    );
    // Enter selects the default row, which Home could not reach (it is there)
    cx.simulate_key_down("members-list", "enter");
    cx.run_until_parked();
    assert_eq!(cx.interactivity(&rows[0]).aria.selected, Some(true));
    cx.simulate_key_down("members-list", "end");
    cx.run_until_parked();
    let last = cx.interactivity(rows.last().expect("rows"));
    assert!(last.aria.active_descendant);
    assert_eq!(last.aria.selected, Some(true));
    assert!(!cx.interactivity(&rows[0]).aria.active_descendant);
    cx.simulate_key_down("members-list", "home");
    cx.run_until_parked();
    assert_eq!(cx.interactivity(&rows[0]).aria.selected, Some(true));
}

#[test]
fn the_arrows_walk_the_list_and_the_chosen_row_is_its_active_one() {
    let (mut cx, _) = ready();
    let of = |cx: &TestAppContext, key: &str| match cx.find(key) {
        Some(Node::Container(ContainerNode { interactivity, .. })) => interactivity.clone(),
        _ => panic!("{key} is a container"),
    };
    let list = of(&cx, "members-list");
    assert_eq!(list.role, Some(ducktape_view_guest::Role::ListBox));
    assert!(list.focusable);
    // the first shown row is active before any choice, so ↓ selects the
    // second, and ↑ comes back
    for (key, number, other) in [("down", 11, 7), ("up", 7, 11)] {
        cx.simulate_key_down("members-list", key);
        let (row, other) = (
            of(&cx, &format!("members-row-{number}")),
            of(&cx, &format!("members-row-{other}")),
        );
        assert_eq!(row.role, Some(ducktape_view_guest::Role::ListBoxOption));
        // a focusable row would lose the flag to the host's sanitizer
        assert!(!row.focusable && row.aria.active_descendant);
        assert_eq!(row.aria.selected, Some(true));
        assert!(!other.aria.active_descendant);
        assert_eq!(other.aria.selected, Some(false));
    }
}

/// ↓ then ↑ (a reader trying the next member, the door's arrow probe)
/// comes back to an account whose block links are there at once, not to
/// "Reading…" for the length of a scan: what was read of it shows while it
/// is read anew, and the new read replaces it.
#[test]
fn an_account_chosen_again_shows_its_activity_at_once_and_reads_it_anew() {
    let (mut cx, _) = ready();
    cx.simulate_click("members-row-7");
    cx.run_until_parked();
    assert!(cx.has_text("block 12"), "{:?}", cx.texts());
    // eddy signs again at 13
    cx.host().handle::<ChainBlocks>(|page: BlockPage| {
        let tx = Tx {
            signer: EDDY.to_vec(),
            seq: 3,
            target: "chat".into(),
            payload: vec![3],
            ..Tx::default()
        };
        Ok(match page.before {
            None => vec![Block {
                height: 13,
                time: 13 * 60_000,
                txs: vec![tx],
                ..Block::default()
            }],
            Some(_) => Vec::new(),
        })
    });
    // ↓ ↑: read anew, the new read shows
    cx.simulate_key_down("members-list", "down");
    cx.simulate_key_down("members-list", "up");
    cx.run_until_parked();
    assert!(cx.has_text("block 13") && !cx.has_text("block 12"));
    // the chain stops answering: ↓ ↑ still shows eddy's links at once
    cx.host().never::<ChainBlocks>();
    cx.simulate_key_down("members-list", "down");
    cx.run_until_parked();
    assert!(cx.has_text("ada's bio"));
    cx.simulate_key_down("members-list", "up");
    cx.run_until_parked();
    assert!(cx.has_text("eddy's bio"));
    assert!(!cx.has_text("Reading the last 1,000 blocks…"));
    assert!(
        cx.has_text("block 13") && cx.find("members-block-0").is_some(),
        "{:?}",
        cx.texts()
    );
}

/// A row is called by the member's name, not the avatar's initial drawn
/// before it; what else the row says is its description.
#[test]
fn a_row_is_named_by_the_member_not_the_avatar() {
    let (cx, _) = ready();
    let aria = |key: &str| match cx.find(key) {
        Some(Node::Container(ContainerNode { interactivity, .. })) => interactivity.aria.clone(),
        _ => panic!("{key} is a container"),
    };
    for (number, name, description) in [
        (7, "eddy", "you · Person"),
        (11, "ada", "Person"),
        (10, "relay", "Agent · managed by eddy · revoked"),
        (8, "chat", "Module"),
    ] {
        let row = aria(&format!("members-row-{number}"));
        assert_eq!(row.label.as_deref(), Some(name));
        assert_eq!(row.description.as_deref(), Some(description));
    }
}

/// A Manages row is called by the agent, not the avatar's initial either;
/// its account and standing are its description.
#[test]
fn a_managed_agent_is_named_by_the_agent_not_the_avatar() {
    let (mut cx, _) = ready();
    cx.simulate_click("members-row-7");
    cx.run_until_parked();
    for (number, name, description) in [
        (9, "scout", "account 9 · Agent · active"),
        (10, "relay", "account 10 · Agent · revoked"),
    ] {
        let Some(Node::Container(ContainerNode { interactivity, .. })) =
            cx.find(&format!("members-manages-{number}"))
        else {
            panic!("members-manages-{number} is a container")
        };
        assert_eq!(interactivity.aria.label.as_deref(), Some(name));
        assert_eq!(interactivity.aria.description.as_deref(), Some(description));
    }
}

#[test]
fn a_kind_with_no_one_in_it_says_so_without_quoting_an_empty_filter() {
    let mut cx = TestAppContext::new();
    cx.host().stream::<HostSession>();
    cx.host().stream::<Changes<Identity>>();
    cx.host()
        .never::<ducktape_view_guest::methods::HostOffset>();
    cx.host().handle::<Query<Identity>>(|_| {
        Ok(identity::Reply::Accounts(page(vec![person(
            7,
            "eddy",
            vec![key(EDDY, "laptop")],
        )])))
    });
    cx.host()
        .handle::<Query<Valset>>(|_| Ok(valset::Reply::Memberships(page(vec![]))));
    cx.open::<Members>();
    cx.run_until_parked();
    cx.simulate_click("members-chip-2");
    assert!(
        cx.has_text("No agents on this network yet."),
        "{:?}",
        cx.texts()
    );
    cx.simulate_input("members-filter", "zz");
    assert!(cx.has_text("No account reads like “zz”."));
}

#[test]
fn a_narrow_pane_covers_the_whole_screen_with_the_detail_and_its_close() {
    let (mut cx, _) = ready();
    cx.simulate_measure("members-viewport", 1000., 640.);
    cx.simulate_click("members-row-7");
    assert!(cx.find("members-detail-over").is_none());
    assert!(cx.find("members-detail-close").is_none());

    cx.simulate_measure("members-viewport", 720., 640.);
    assert!(cx.find("members-list-resize").is_none(), "nothing to drag");
    let full = StyleRefinement::default().inset_0();
    assert_eq!(style(&cx, "members-detail-over").inset, full.inset);
    let whole = StyleRefinement::default().size_full();
    assert_eq!(style(&cx, "members-detail").size, whole.size);
    cx.simulate_click("members-detail-close");
    assert!(cx.find("members-detail-over").is_none());
    assert!(cx.find("members-detail").is_none(), "the list alone");
    cx.simulate_click("members-row-7");
    assert!(cx.find("members-detail-over").is_some());
}

#[test]
fn the_docked_list_drags_within_its_bounds_and_leaves_the_detail_its_narrowest() {
    let (mut cx, _) = ready();
    cx.simulate_measure("members-viewport", 1000., 640.);
    assert!(matches!(
        cx.find("members-list-resize"),
        Some(Node::ResizeHandle {
            on_drag: Some(_),
            ..
        })
    ));
    assert!(list_is(&cx, 400.));
    cx.simulate_drag("members-list-resize", 60., 0.);
    assert!(list_is(&cx, 460.));
    // never past the detail's 440 in 1000
    cx.simulate_drag("members-list-resize", 400., 0.);
    assert!(list_is(&cx, 560.));
    cx.simulate_drag("members-list-resize", -900., 0.);
    assert!(list_is(&cx, 320.));
    // the window shrinks: the list gives way before the detail floats
    cx.simulate_drag("members-list-resize", 200., 0.);
    cx.simulate_measure("members-viewport", 800., 640.);
    assert!(cx.find("members-list-resize").is_some());
    assert!(list_is(&cx, 360.));
}

fn style(cx: &TestAppContext, key: &str) -> StyleRefinement {
    let Some(Node::Container(ContainerNode { style, .. })) = cx.find(key) else {
        panic!("{key} is a styled container");
    };
    (*style).clone()
}

/// Whether the docked list is `width` wide.
fn list_is(cx: &TestAppContext, width: f32) -> bool {
    style(cx, "members-list-pane").size.width
        == StyleRefinement::default()
            .w(ducktape_view_guest::px(width))
            .size
            .width
}
