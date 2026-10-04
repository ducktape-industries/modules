//! Every screen of this view, replayed from the forge program's own bytes.
//!
//! `crates/app/forge/fixtures/replies.bin` holds the program's real `Respond`
//! bytes; nothing here builds a reply by hand. The fake host decodes one and
//! hands it back, so an unhandled ask is a panic and a screen that reads a
//! field the program does not send cannot compile.
use super::*;
use crate::api::{Ask, HostSession, Session, SubmitForge};
use crate::state::{ChangeTab, Filter, RepoTab};
use ducktape_view_guest::methods::HostId;
use ducktape_view_guest::methods::{Query as ProgramQuery, Submit};
use ducktape_view_guest::testing::{StreamSender, TestAppContext};
use ducktape_view_guest::{Entity, Theme, wire};
use forge::{ChangeFilter, ChangeState, Op, PageRequest, PageResponse, Query, Reply};

use ducktape_view_guest::methods::{Change, Changes};
use program::role::Identity;

#[path = "../../forge/fixtures/loader.rs"]
mod loader;

/// One committed fixture, exactly as the program answered it.
pub(crate) fn bytes(name: &str) -> Vec<u8> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../forge/fixtures");
    loader::bytes(&dir, name)
}

fn reply(name: &str) -> Reply {
    borsh::from_slice(&bytes(name)).unwrap_or_else(|error| panic!("decode {name}: {error}"))
}

/// One committed refusal, as the host hands the program's `Err` to a view.
fn refusal(name: &str) -> ducktape_view_guest::host::Error {
    let refusal: abi::Refusal =
        borsh::from_slice(&bytes(name)).unwrap_or_else(|error| panic!("decode {name}: {error}"));
    ducktape_view_guest::host::Error::new(&refusal.reason, &refusal.sentence)
}

/// Which change record the program is holding in a given scenario.
fn change_fixture(mode: &str) -> &'static str {
    match mode {
        "reviewed" | "review" => "change-reviewed",
        "merged" => "change-merged",
        "closed" => "change-closed",
        "outdated" => "change-outdated",
        _ => "change",
    }
}

/// The program's answers, chosen by query shape — never by hand.
fn answer(query: &Query, mode: &str) -> Reply {
    match query {
        Query::Repos { .. } if mode == "empty" => reply("repos-empty"),
        Query::Repos { .. } => reply("repos"),
        Query::Repo { .. } => reply("repo"),
        Query::Refs {
            page: PageRequest { after: None, .. },
            ..
        } if mode == "unborn" => reply("refs-empty"),
        Query::Refs {
            page: PageRequest { after: None, .. },
            ..
        } => reply("refs"),
        Query::Refs { .. } => reply("refs-next"),
        Query::Activity { .. } => reply("activity"),
        Query::Tree {
            page: PageRequest { after: None, .. },
            path,
            ..
        } if path.is_empty() => reply("tree"),
        Query::Tree { path, .. } if path.is_empty() => reply("tree-next"),
        Query::Tree { .. } => reply("tree-directory"),
        Query::Blob { oid, .. } => match oid.as_str() {
            "95d586e774a04676a07a142f0e2f97a4f32562cb" => reply("blob-binary"),
            "b90a09e55e43808905fe881245853c1b35b3fb82" => reply("blob-oversize"),
            _ => reply("blob"),
        },
        Query::Log {
            page: PageRequest { after: None, .. },
            ..
        } => reply("log"),
        Query::Log { .. } => reply("log-next"),
        Query::Diff { .. } if mode == "binary" => reply("diff-binary"),
        Query::Diff { .. } => reply("diff-text"),
        Query::Compare { .. } if mode == "diverged" => reply("compare-diverged"),
        Query::Compare { .. } => reply("compare"),
        Query::Changes {
            filter:
                ChangeFilter {
                    state: Some(ChangeState::Closed),
                    ..
                },
            ..
        } => reply("changes-filtered"),
        Query::Changes { .. } if mode == "empty" => reply("changes-empty"),
        Query::Changes { .. } => reply("changes"),
        Query::Change {
            page: PageRequest { after: None, .. },
            ..
        } => reply(change_fixture(mode)),
        Query::Change { .. } => reply("change-reviews-next"),
        Query::Judgment { .. } if mode == "judgment-empty" => reply("judgment-empty"),
        Query::Judgment { .. } => reply("judgment"),
        other => panic!("no fixture for {other:?}"),
    }
}

/// forge's own account.
const FORGE: u64 = 900;

fn accounts() -> chat::PageResponse<chat::Profile> {
    let row = |number, name: &str| chat::Profile {
        number,
        name: name.into(),
        kind: chat::Kind::Person,
    };
    let forge = chat::Profile {
        kind: chat::Kind::Module(forge::MODULE.into()),
        ..row(FORGE, forge::MODULE)
    };
    chat::PageResponse {
        height: 1,
        next: None,
        items: vec![
            row(1, "Ada"),
            row(2, "Rae"),
            row(4, "Tal"),
            row(9, "Wren"),
            forge,
        ],
    }
}

fn message(seq: u64, author: forge::Principal, text: &str) -> chat::MsgRow {
    chat::MsgRow {
        channel_id: "forge:project:1".into(),
        seq,
        message_id: format!("m{seq}"),
        blocks: vec![chat::Block::paragraph(text)],
        text: text.into(),
        ..chat::MsgRow::by(author)
    }
}

/// Change 1's channel as forge and chat would fill it: forge's own lines
/// under forge's real message ids (the open, one per review, the merge),
/// with a reader's reply among them.
fn forge_lines() -> Vec<chat::MsgRow> {
    let reviews = ["change-reviewed", "change-reviews-next"]
        .into_iter()
        .flat_map(|name| match reply(name) {
            Reply::Change { reviews, .. } => reviews.items,
            _ => panic!("{name} is a change"),
        });
    let forge_line = |seq: u64, message_id: String| chat::MsgRow {
        message_id,
        ..message(seq, forge::Principal::Account(FORGE), "raw forge text")
    };
    let mut rows = vec![forge_line(1, "forge:0000000000000001".into())];
    for review in reviews {
        rows.push(forge_line(rows.len() as u64 + 1, review.message_id));
    }
    rows.push(message(
        rows.len() as u64 + 1,
        forge::Principal::Account(2),
        "Reading it now",
    ));
    rows.push(forge_line(
        rows.len() as u64 + 1,
        "forge:00000000000000ff".into(),
    ));
    rows
}

/// Chat's answers: the roster, and each change channel's rows.
fn chat_answer(query: chat::Query) -> chat::Reply {
    match query {
        chat::Query::Accounts { .. } => chat::Reply::Accounts(accounts()),
        chat::Query::Roots { channel_id, .. } => chat::Reply::Roots(PageResponse {
            height: 1,
            items: match channel_id.as_str() {
                "forge:project:1" => forge_lines(),
                // change 2 was opened, then closed
                "forge:project:2" => (1..=2)
                    .map(|seq| chat::MsgRow {
                        channel_id: channel_id.clone(),
                        message_id: format!("forge:{seq:016x}"),
                        ..message(seq, forge::Principal::Account(FORGE), "raw forge text")
                    })
                    .collect(),
                _ => Vec::new(),
            },
            next: None,
        }),
        other => panic!("unexpected chat query: {other:?}"),
    }
}

pub(crate) fn configure(cx: &mut TestAppContext, mode: &'static str) {
    cx.host().handle::<Ask>(move |query| {
        if mode == "refused" && !matches!(query, Query::Repos { .. }) {
            return Err(refusal("refused-not-found"));
        }
        Ok(answer(&query, mode))
    });
    cx.host()
        .handle::<ProgramQuery<::chat::Chat>>(|query| Ok(chat_answer(query)));
    cx.host().handle::<Submit<::chat::Chat>>(|_| Ok(Vec::new()));
    cx.host().handle::<SubmitForge>(|_| Ok(Vec::new()));
    cx.host().handle::<HostId>(|kind| Ok(format!("{kind}-1")));
}

/// Boots the view, seats a reader and waits for the first reads to land.
pub(crate) fn booted(mode: &'static str) -> (TestAppContext, Entity<Forge>) {
    booted_as(mode, 2)
}

/// Boots seated as `account`: 2 (Rae) reviews and holds no write; 1 (Ada)
/// owns `project`; 9 (Wren) is its granted writer.
pub(crate) fn booted_as(mode: &'static str, account: u64) -> (TestAppContext, Entity<Forge>) {
    let mut cx = TestAppContext::new();
    configure(&mut cx, mode);
    let props = cx.host().stream::<HostSession>();
    let view = cx.open::<Forge>();
    cx.run_until_parked();
    props.send(Session {
        signer: abi::hex(b"reviewer"),
        account: Some(account),
        connected: true,
        chain_id: "testnet#0a1b2c3d".into(),
        ..Session::default()
    });
    cx.run_until_parked();
    (cx, view)
}

/// A block at `height` that wrote `keys`, as `module.changes` carries it.
fn block(height: u64, keys: Vec<Vec<u8>>) -> Option<Change> {
    Some(Change { height, keys })
}

/// What a push writes: the ref it moved.
fn ref_key() -> Vec<u8> {
    forge::tables::REFS.key(&("project".to_owned(), b"refs/heads/main".to_vec()))
}

/// What a review writes: its record.
fn review_key() -> Vec<u8> {
    forge::tables::REVIEWS.key(&("project".to_owned(), 1u64, 1u64))
}

/// What a message posted in `channel` writes: its root.
fn root_key(channel: &str) -> Vec<u8> {
    chat::tables::ROOTS.key(&(channel.to_owned(), 0u64))
}

/// What joining `channel` writes: a member row.
fn member_key(channel: &str) -> Vec<u8> {
    chat::tables::MEMBERS.key(&(channel.to_owned(), forge::Principal::Account(2)))
}

/// What a reply to the reader's thread in `channel` writes that a judgment
/// reads: the reader's `ANSWERED` entry.
fn answered_key(channel: &str) -> Vec<u8> {
    chat::tables::ANSWERED.key(&(channel.to_owned(), forge::Principal::Account(2), 0u64))
}

/// What a review actually writes (`submit_review`, then `touch`): its
/// record, its author index entry, the change's counts, forge's message
/// counter, the repository's record and activity, and the write count.
fn real_review_keys() -> Vec<Vec<u8>> {
    let (repo, n, id) = ("project".to_owned(), 1u64, 1u64);
    vec![
        review_key(),
        forge::tables::AUTHORED.key(&(repo.clone(), n, forge::Principal::Account(2), id)),
        forge::tables::CHANGES.key(&(repo.clone(), n)),
        forge::tables::MESSAGES.key(),
        forge::tables::REPOS.key(&repo),
        forge::tables::ACTIVITY.key(&(u64::MAX - 100, repo)),
        forge::tables::WRITES.key(),
    ]
}

/// What the line a review posts into the change's conversation writes in
/// chat (`discussion::post`): the message row, its root entry, its id and
/// the channel's head.
fn posted_line_keys(channel: &str) -> Vec<Vec<u8>> {
    vec![
        chat::tables::MESSAGES.key(&(channel.to_owned(), 1u64)),
        root_key(channel),
        chat::tables::MESSAGE_IDS.key(&format!("{channel}:review:1")),
        chat::tables::HEADS.key(&channel.to_owned()),
    ]
}

/// The live heads of the three programs forge follows, in the test's hands.
struct Heads {
    forge: StreamSender<Changes<forge::Forge>>,
    chat: StreamSender<Changes<::chat::Chat>>,
    identity: StreamSender<Changes<Identity>>,
}

/// Boots like [`booted`], following heads the test sends, and opens
/// `project`.
fn followed(mode: &'static str) -> (TestAppContext, Heads) {
    let mut cx = TestAppContext::new();
    configure(&mut cx, mode);
    let heads = Heads {
        forge: cx.host().stream(),
        chat: cx.host().stream(),
        identity: cx.host().stream(),
    };
    let props = cx.host().stream::<HostSession>();
    cx.open::<Forge>();
    props.send(Session {
        signer: abi::hex(b"reviewer"),
        account: Some(2),
        connected: true,
        chain_id: "testnet#0a1b2c3d".into(),
        ..Session::default()
    });
    cx.run_until_parked();
    cx.simulate_click("forge-repo-project-open");
    cx.run_until_parked();
    (cx, heads)
}

/// A block that changed nothing on screen: a forge block re-reads the
/// reads of the tables it wrote to, a chat block and an identity block
/// re-read no forge read at all, the same replies land, and the view draws
/// nothing for the items or the landings.
#[test]
fn a_block_whose_rereads_land_the_same_replies_draws_nothing() {
    let (mut cx, heads) = followed("default");
    let renders = cx.renders();
    let asked = cx.host().requests::<Ask>().len();
    heads.forge.send(block(100, vec![ref_key()]));
    cx.run_until_parked();
    assert!(
        cx.host().requests::<Ask>().len() > asked,
        "the refs re-read"
    );
    let asked = cx.host().requests::<Ask>().len();
    heads.chat.send(block(101, vec![root_key("general")]));
    heads.identity.send(block(102, Vec::new()));
    cx.run_until_parked();
    assert_eq!(
        cx.host().requests::<Ask>().len(),
        asked,
        "no forge read moved"
    );
    assert_eq!(cx.renders(), renders, "nothing drew");
}

/// A refusal on screen is asked again with each block, without a Retry:
/// while it holds it stays, drawing nothing, and the block after the node
/// answers replaces it with the page.
#[test]
fn a_refused_read_is_asked_again_with_each_block_until_it_answers() {
    let (mut cx, heads) = followed("default");
    cx.host().handle::<Ask>(|query| match query {
        Query::Log { .. } => Err(refusal("refused-not-found")),
        query => Ok(answer(&query, "default")),
    });
    cx.simulate_click("forge-tab-commits");
    cx.run_until_parked();
    let sentence = refusal("refused-not-found").message;
    assert!(cx.has_text(&sentence), "{:?}", cx.texts());
    let logs = |cx: &TestAppContext| {
        let asked = cx.host().requests::<Ask>();
        asked
            .iter()
            .filter(|query| matches!(query, Query::Log { .. }))
            .count()
    };
    let (renders, asked) = (cx.renders(), logs(&cx));
    // a block that wrote nothing the log reads asks a refusal again all the
    // same: forge refuses a listing `stale` when any op moved its count
    heads.forge.send(block(100, vec![review_key()]));
    cx.run_until_parked();
    assert_eq!(logs(&cx), asked + 1, "the refusal was asked again");
    assert!(cx.has_text(&sentence));
    assert_eq!(cx.renders(), renders, "a refusal that holds drew nothing");
    cx.host()
        .handle::<Ask>(|query| Ok(answer(&query, "default")));
    heads.forge.send(block(101, vec![review_key()]));
    cx.run_until_parked();
    assert!(!cx.has_text(&sentence), "{:?}", cx.texts());
    assert!(cx.has_text("Feature"), "{:?}", cx.texts());
}

/// A change's conversation refused (the node was away) is asked again with
/// each block: while the refusal holds it stays, drawing nothing, and the
/// block after the node answers replaces it with the rows. It has no Retry
/// of its own, so before this only leaving the change cleared it.
#[test]
fn a_refused_conversation_is_asked_again_with_each_block_until_it_answers() {
    let (mut cx, heads) = followed("default");
    cx.host()
        .handle::<ProgramQuery<::chat::Chat>>(|query| match query {
            chat::Query::Roots { .. } => Err(ducktape_view_guest::host::Error::new(
                "not_connected",
                "the node is not connected",
            )),
            query => Ok(chat_answer(query)),
        });
    cx.simulate_click("forge-tab-changes");
    cx.run_until_parked();
    cx.simulate_click("forge-change-1");
    cx.run_until_parked();
    assert!(
        cx.find("forge-conversation-refused").is_some(),
        "{:?}",
        cx.texts()
    );
    let renders = cx.renders();
    heads
        .chat
        .send(block(100, vec![member_key("forge:project:1")]));
    cx.run_until_parked();
    assert!(cx.find("forge-conversation-refused").is_some());
    assert_eq!(cx.renders(), renders, "a refusal that holds drew nothing");
    cx.host()
        .handle::<ProgramQuery<::chat::Chat>>(|query| Ok(chat_answer(query)));
    heads
        .chat
        .send(block(101, vec![member_key("forge:project:1")]));
    cx.run_until_parked();
    assert!(
        cx.find("forge-conversation-refused").is_none(),
        "{:?}",
        cx.texts()
    );
    assert!(cx.has_text("Reading it now"), "{:?}", cx.texts());
}

/// A read still out when a block lands is left to land: a block asks
/// again only what is on screen, so heads that come faster than a slow
/// read (a wide Compare, a large Diff) never restart it.
#[test]
fn a_block_leaves_a_read_still_out_to_land() {
    let (mut cx, heads) = followed("default");
    cx.simulate_click("forge-tab-changes");
    cx.run_until_parked();
    let blocks = |cx: &mut TestAppContext| {
        for height in 100..103 {
            heads
                .chat
                .send(block(height, vec![root_key("forge:project:1")]));
            cx.run_until_parked();
        }
    };
    // chat stops answering: the change's conversation is still out
    cx.host().never::<ProgramQuery<::chat::Chat>>();
    cx.simulate_click("forge-change-1");
    cx.run_until_parked();
    let conversations = |cx: &TestAppContext| {
        let asked = cx.host().requests::<ProgramQuery<::chat::Chat>>();
        asked
            .iter()
            .filter(|query| matches!(query, chat::Query::Roots { .. }))
            .count()
    };
    assert_eq!(conversations(&cx), 1);
    blocks(&mut cx);
    assert_eq!(
        conversations(&cx),
        1,
        "the conversation was not asked again"
    );
    // then forge stops: the Files tab's diff is still out
    cx.host().never::<Ask>();
    cx.simulate_click("forge-change-tab-files");
    cx.run_until_parked();
    let diffs = |cx: &TestAppContext| {
        let asked = cx.host().requests::<Ask>();
        asked
            .iter()
            .filter(|query| matches!(query, Query::Diff { .. }))
            .count()
    };
    assert_eq!(diffs(&cx), 1);
    blocks(&mut cx);
    assert_eq!(diffs(&cx), 1, "the diff was not asked again");
}

/// Boots and opens `project`.
pub(crate) fn opened(mode: &'static str) -> (TestAppContext, Entity<Forge>) {
    opened_as(mode, 2)
}

pub(crate) fn opened_as(mode: &'static str, account: u64) -> (TestAppContext, Entity<Forge>) {
    let (mut cx, view) = booted_as(mode, account);
    cx.simulate_click("forge-repo-project-open");
    cx.run_until_parked();
    (cx, view)
}

fn disabled(cx: &TestAppContext, id: &str) -> bool {
    let Some(wire::Node::Container(ducktape_view_guest::wire::ContainerNode {
        interactivity, ..
    })) = cx.find(id)
    else {
        panic!("{id} button")
    };
    interactivity.aria.disabled == Some(true) && interactivity.on_click.is_none()
}

#[test]
fn session_key_resolves_to_its_account() {
    let (_cx, view) = booted("default");
    view.read(|forge| {
        assert_eq!(forge.my_account(), Some(2));
        assert_eq!(forge.me_principal(), Some(forge::Principal::Account(2)));
    });
}

/// Seats `key` (with `account` once identity names one) in a fresh view
/// and opens `project`'s changes.
fn seated(key: &[u8], account: Option<u64>) -> (TestAppContext, Entity<Forge>) {
    let mut cx = TestAppContext::new();
    configure(&mut cx, "judgment");
    let props = cx.host().stream::<HostSession>();
    let view = cx.open::<Forge>();
    cx.run_until_parked();
    props.send(Session {
        signer: abi::hex(key),
        account,
        connected: true,
        chain_id: "testnet#0a1b2c3d".into(),
        ..Session::default()
    });
    cx.run_until_parked();
    cx.simulate_click("forge-repo-project-open");
    cx.run_until_parked();
    cx.simulate_click("forge-tab-changes");
    cx.run_until_parked();
    (cx, view)
}

/// The principals forge was asked to judge.
fn judged(cx: &TestAppContext) -> Vec<forge::Principal> {
    cx.host()
        .requests::<Ask>()
        .into_iter()
        .filter_map(|query| match query {
            Query::Judgment { principal, .. } => Some(principal),
            _ => None,
        })
        .collect()
}

/// With no key seated nobody is "me": the filters about me are off.
#[test]
fn no_seated_key_has_no_changes_of_its_own() {
    let (cx, view) = seated(b"", None);
    view.read(|forge| assert_eq!(forge.me_principal(), None));
    assert!(disabled(&cx, "forge-filter-judgment"));
    assert!(disabled(&cx, "forge-filter-authored"));
}

/// A key that holds no account reads everything and writes nothing: every
/// write control is off, the filters about "me" ask nothing, and the
/// screen says an account is needed.
#[test]
fn a_key_without_an_account_writes_nothing() {
    let (cx, view) = seated(b"stranger", None);
    view.read(|forge| {
        assert_eq!(forge.my_account(), None);
        assert_eq!(forge.me_principal(), None);
        assert!(!forge.may_write());
    });
    assert!(disabled(&cx, "forge-filter-judgment"));
    assert!(disabled(&cx, "forge-filter-authored"));
    assert!(cx.find("forge-no-account").is_some());
    assert!(judged(&cx).is_empty());
}

/// A person's second device key is the same person: "mine" and judgment
/// are the account's, whichever key is seated.
#[test]
fn a_second_device_key_reads_as_the_same_person() {
    let (mut cx, view) = seated(b"tester-laptop", Some(1));
    cx.simulate_click("forge-filter-authored");
    cx.run_until_parked();
    let authored = cx.host().requests::<Ask>().into_iter().any(|query| {
        matches!(query, Query::Changes { filter, .. }
            if filter.author == Some(forge::Principal::Account(1)))
    });
    assert!(authored, "my changes are my account's");
    view.read(|forge| assert_eq!(forge.me_principal(), Some(forge::Principal::Account(1))));
    cx.simulate_click("forge-filter-judgment");
    cx.run_until_parked();
    assert_eq!(judged(&cx), [forge::Principal::Account(1)]);
}

/// The reader creates the account in Account, then switches to Forge: the
/// seated key never changes; the host resolves its new account and hands it
/// over as a session change, and the same key writes and is judged as the
/// account from then on.
#[test]
fn an_account_gained_later_is_who_forge_judges() {
    let mut cx = TestAppContext::new();
    configure(&mut cx, "judgment");
    let props = cx.host().stream::<HostSession>();
    let view = cx.open::<Forge>();
    cx.run_until_parked();
    let unregistered = Session {
        signer: abi::hex(b"reviewer"),
        connected: true,
        chain_id: "testnet#0a1b2c3d".into(),
        ..Session::default()
    };
    props.send(unregistered.clone());
    cx.run_until_parked();
    cx.simulate_click("forge-repo-project-open");
    cx.run_until_parked();
    cx.simulate_click("forge-tab-changes");
    cx.run_until_parked();
    assert!(disabled(&cx, "forge-filter-judgment"));
    props.send(Session {
        account: Some(2),
        ..unregistered
    });
    cx.run_until_parked();
    view.read(|forge| assert_eq!(forge.my_account(), Some(2)));
    assert!(cx.find("forge-no-account").is_none());
    cx.simulate_click("forge-filter-judgment");
    cx.run_until_parked();
    assert_eq!(judged(&cx), [forge::Principal::Account(2)]);
}

// at 560 the repository tab bar's About toggle is past the right edge
#[test]
fn the_view_is_laid_out_from_640() {
    assert_eq!(<Forge as View>::MIN_WINDOW_WIDTH, 640);
}

#[test]
fn the_root_wears_the_shared_theme_and_is_accessible() {
    let (mut cx, _) = booted("default");
    let dark = Theme::dark();
    cx.set_global(dark);
    let Some(wire::Node::Container(ducktape_view_guest::wire::ContainerNode { style, .. })) =
        cx.find("forge")
    else {
        panic!("the forge root is a styled container");
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

#[test]
fn the_repositories_list_shows_every_column_of_the_plan() {
    let (mut cx, _) = booted("default");
    assert!(cx.has_text("Repositories"));
    assert!(cx.has_text("project"), "{:?}", cx.texts());
    assert!(cx.has_text("main"), "the default head is a badge");
    assert!(cx.has_text("Ada"), "the owner key resolves to a name");
    assert!(cx.has_text("6"), "the refs column");
    for column in ["Name", "Owner", "Default", "Refs", "Last active"] {
        assert!(cx.has_text(column), "{column}: {:?}", cx.texts());
    }
    assert!(cx.has_text("block 2"));
    assert!(
        cx.has_text("duck://testnet-0a1b2c3d/forge/project"),
        "the row shows where it clones from"
    );
    cx.simulate_click("forge-repo-project-activity");
    assert_eq!(
        cx.host()
            .requests::<ducktape_view_guest::methods::LinkOpen>(),
        ["duck://explorer/block/2"]
    );
}

#[test]
fn an_empty_program_explains_how_a_repository_begins() {
    let (cx, _) = booted("empty");
    assert!(cx.has_text("No repositories yet"));
    assert!(
        cx.texts()
            .iter()
            .any(|text| text.contains("Create one with New repository"))
    );
}

#[test]
fn an_unborn_repo_says_so_instead_of_resolving_forever() {
    let (cx, _) = opened("unborn");
    assert!(cx.has_text("No commits yet"), "{:?}", cx.texts());
    assert!(!cx.has_text("Resolving the ref…"));
}

#[test]
fn a_refused_read_keeps_its_reason_and_offers_one_retry() {
    let mut cx = TestAppContext::new();
    cx.host()
        .handle::<Ask>(|_| Err(refusal("refused-object-not-held")));
    cx.host()
        .handle::<ProgramQuery<::chat::Chat>>(|_| Ok(chat::Reply::Accounts(accounts())));
    cx.open::<Forge>();
    cx.run_until_parked();
    let sentence = "object ffffffffffffffffffffffffffffffffffffffff is not held by this node";
    assert!(cx.has_text(sentence), "{:?}", cx.texts());
    assert!(cx.find("forge-repos-list-retry").is_some());
    cx.simulate_click("forge-repos-list-retry");
    cx.run_until_parked();
    assert!(cx.has_text(sentence));
}

#[test]
fn creating_a_repository_validates_its_name_then_shows_the_submission() {
    let (mut cx, view) = booted("default");
    cx.simulate_click("forge-new-repo");
    assert!(cx.has_text("New repository"));
    cx.simulate_input("forge-new-repo-name", "not a name");
    cx.simulate_click("forge-new-repo-submit");
    cx.run_until_parked();
    assert!(
        cx.has_text("A repository name is 1–37 bytes of letters, digits, dot, dash or underscore"),
        "{:?}",
        cx.texts()
    );
    assert!(cx.host().requests::<SubmitForge>().is_empty());
    // SHA-256 unless SHA-1 is picked
    cx.simulate_input("forge-new-repo-name", "ledger");
    cx.simulate_click("forge-new-repo-submit");
    cx.run_until_parked();
    view.read(|forge| assert!(forge.new_repo.is_none()));
    cx.simulate_click("forge-new-repo");
    assert!(
        cx.has_text(
            "Pick SHA-1 to push an existing Git project. The format is fixed once created."
        )
    );
    cx.simulate_input("forge-new-repo-name", "imported");
    cx.simulate_click("forge-new-repo-sha1");
    cx.simulate_click("forge-new-repo-submit");
    cx.run_until_parked();
    let created: Vec<(String, abi::HashKind)> = cx
        .host()
        .requests::<SubmitForge>()
        .iter()
        .filter_map(|op| match op {
            Op::Create { repo, hash } => Some((repo.clone(), *hash)),
            _ => None,
        })
        .collect();
    assert_eq!(
        created,
        [
            ("ledger".to_owned(), abi::HashKind::Sha256),
            ("imported".to_owned(), abi::HashKind::Sha1),
        ]
    );
    view.read(|forge| assert!(forge.new_repo.is_none()));
}

#[test]
fn a_repository_opens_on_code_with_its_header_ref_picker_and_tabs() {
    let (cx, view) = opened("default");
    view.read(|forge| assert_eq!(forge.nav().repo.as_deref(), Some("project")));
    assert!(
        cx.has_text("owner Ada · 6 refs · active"),
        "{:?}",
        cx.texts()
    );
    assert!(
        cx.texts()
            .iter()
            .any(|text| text.starts_with("duck://testnet-0a1b2c3d/forge/project")),
        "{:?}",
        cx.texts()
    );
    for tab in RepoTab::ALL {
        assert!(
            cx.find(&format!("forge-tab-{}", tab.slug())).is_some(),
            "{} tab",
            tab.label()
        );
    }
    // The ref picker is one dropdown; open, it carries every ref the paged
    // read followed, and a press outside folds it.
    assert!(cx.find("forge-ref-refs/heads/clean").is_none());
    let mut cx = cx;
    cx.simulate_click("forge-ref-picker");
    cx.run_until_parked();
    assert!(cx.find("forge-ref-refs/heads/clean").is_some());
    assert!(cx.find("forge-ref-refs/heads/conflict").is_some());
    assert!(cx.has_text("Branches"), "{:?}", cx.texts());
    cx.update(&view, |forge, _, cx| forge.open_menu(None, cx));
    cx.run_until_parked();
    assert!(cx.find("forge-ref-picker-menu").is_none());
}

/// The repository tabs read on open, so they are a manual tab list: → moves
/// the active tab without opening it, Enter opens it, and a click resets
/// the arrows to the open tab.
#[test]
fn the_repository_tabs_move_on_an_arrow_and_open_on_enter() {
    let (mut cx, view) = opened("default");
    let list = cx.interactivity("forge-tab-list");
    assert_eq!(list.role, Some(ducktape_view_guest::Role::TabList));
    assert!(list.focusable && list.tab_stop == Some(true));
    assert!(!cx.interactivity("forge-tab-code").focusable);
    // the repository opens on its README; the arrows start there
    view.read(|forge| assert_eq!(forge.nav().tab, RepoTab::Readme));
    assert!(cx.interactivity("forge-tab-readme").aria.active_descendant);
    cx.simulate_focus("forge-tab-list");
    cx.simulate_key_down("forge-tab-list", "right");
    view.read(|forge| assert_eq!(forge.nav().tab, RepoTab::Readme));
    let code = cx.interactivity("forge-tab-code");
    assert!(code.aria.active_descendant);
    assert_eq!(code.aria.selected, Some(false));
    assert!(!cx.interactivity("forge-tab-readme").aria.active_descendant);
    cx.simulate_key_down("forge-tab-list", "enter");
    cx.run_until_parked();
    view.read(|forge| assert_eq!(forge.nav().tab, RepoTab::Code));
    assert_eq!(cx.interactivity("forge-tab-code").aria.selected, Some(true));
    cx.simulate_key_down("forge-tab-list", "right");
    cx.simulate_click("forge-tab-refs");
    cx.run_until_parked();
    view.read(|forge| {
        assert_eq!(forge.nav().tab, RepoTab::Refs);
        assert_eq!(forge.tab_cursor, None);
    });
    assert!(cx.interactivity("forge-tab-refs").aria.active_descendant);
}

#[test]
fn the_about_panel_docks_what_the_repo_record_carries() {
    let (mut cx, view) = opened("default");
    cx.simulate_click("forge-dock-about");
    cx.run_until_parked();
    view.read(|forge| assert_eq!(forge.nav().dock, Some(crate::state::Dock::About)));
    assert!(cx.has_text("sha1"), "{:?}", cx.texts());
    assert!(cx.has_text("64 bytes"), "the founded inline blob bound");
    assert!(cx.has_text("Wren"), "the granted writer");
    cx.simulate_click("forge-dock-close");
    cx.run_until_parked();
    view.read(|forge| assert!(forge.nav().dock.is_none()));
}

#[test]
fn a_repo_opens_on_its_readme_and_code_holds_the_tree() {
    let (mut cx, view) = opened("default");
    cx.simulate_click("forge-ref-picker");
    cx.simulate_click("forge-ref-refs/heads/clean");
    cx.run_until_parked();
    view.read(|forge| {
        assert_eq!(forge.head_name(), b"refs/heads/clean".to_vec());
        assert_eq!(forge.nav().tab, RepoTab::Readme, "README first");
    });
    // The README is the page, rendered, under the repository header.
    assert!(cx.find("forge-repo-header").is_some());
    assert!(cx.find("forge-readme-body").is_some(), "{:?}", cx.texts());
    assert!(cx.find("forge-tree").is_none(), "no tree on the front page");
    cx.simulate_click("forge-tab-code");
    cx.run_until_parked();
    assert!(cx.has_text("README.md"), "{:?}", cx.texts());
    assert!(cx.has_text("empty.txt"));
    assert!(cx.find("forge-code-empty").is_some(), "nothing open yet");
    cx.simulate_input("forge-tree-search", "empty");
    cx.run_until_parked();
    assert!(
        cx.find("forge-tree-README.md").is_none(),
        "the filter drops the row"
    );
    cx.simulate_input("forge-tree-search", "");
    cx.simulate_click("forge-tree-empty.txt");
    cx.run_until_parked();
    view.read(|forge| assert!(forge.nav().blob.is_some()));
    assert!(cx.find("forge-blob-lines").is_some(), "{:?}", cx.texts());
    assert!(cx.has_text("one"), "the first source line is drawn");
    // a markdown file is rendered, not listed
    cx.simulate_click("forge-tree-README.md");
    cx.run_until_parked();
    assert!(cx.find("forge-blob-markdown").is_some());
    assert!(cx.find("forge-blob-lines").is_none());
    cx.simulate_click("forge-blob-close");
    cx.run_until_parked();
    view.read(|forge| assert!(forge.nav().blob.is_none()));
}

/// The rail's and the tree's filters say what they filter; the hint drawn
/// in them stays a hint.
#[test]
fn a_filter_field_is_named_apart_from_its_hint() {
    let (mut cx, _view) = opened("default");
    cx.simulate_click("forge-tab-code");
    cx.run_until_parked();
    for (key, name, hint) in [
        (
            "forge-rail-search",
            "Filter repositories",
            "Search repositories",
        ),
        ("forge-tree-search", "Filter the file tree", "Filter files"),
    ] {
        let Some(wire::Node::Field {
            options,
            placeholder,
            ..
        }) = cx.find(key)
        else {
            panic!("no field {key}");
        };
        assert_eq!((options.label.as_str(), placeholder.as_str()), (name, hint));
    }
}

#[test]
fn a_relative_link_opens_its_file_in_the_code_tab() {
    let (mut cx, view) = opened("default");
    cx.simulate_click("forge-ref-picker");
    cx.simulate_click("forge-ref-refs/heads/clean");
    cx.run_until_parked();
    let follow = |cx: &mut TestAppContext, dir: &[u8], dest: &str| {
        cx.update(&view, |forge, _, cx| forge.follow_link(dir, dest, cx));
        cx.run_until_parked();
    };
    // a folder unfolds, from a document one folder down
    follow(&mut cx, b"docs", "../src");
    view.read(|forge| {
        assert_eq!(forge.nav().tab, RepoTab::Code);
        assert!(forge.nav().expanded.contains(b"src".as_slice()));
    });
    let child = view.read(|forge| {
        forge
            .tree_rows()
            .into_iter()
            .find(|row| row.depth == 1 && !row.is_dir())
            .expect("a file inside src")
            .path
    });
    // a file inside it opens by its path alone, once its folder is read
    cx.update(&view, |forge, _, cx| {
        forge.nav_close_blob(cx);
        forge.open_tab(RepoTab::Readme, cx);
        forge.nav.expanded.clear();
    });
    cx.run_until_parked();
    follow(&mut cx, b"", &format!("./{}#top", path_of(&child)));
    view.read(|forge| {
        assert_eq!(forge.nav().tab, RepoTab::Code);
        assert_eq!(forge.nav().blob.as_ref().map(|(p, _)| p), Some(&child));
    });
    assert!(cx.find("forge-blob").is_some());
    // the root README is its own tab; a missing file says so
    follow(&mut cx, b"src", "../README.md");
    view.read(|forge| assert_eq!(forge.nav().tab, RepoTab::Readme));
    follow(&mut cx, b"", "missing.md");
    view.read(|forge| assert!(forge.notice.contains("missing.md"), "{}", forge.notice));
    // the web still goes to the host
    follow(&mut cx, b"", "https://x.example");
    assert_eq!(
        cx.host()
            .requests::<ducktape_view_guest::methods::LinkOpen>(),
        vec!["https://x.example"]
    );
}

#[test]
fn a_folder_opens_its_children_inline_and_keeps_its_state() {
    let (mut cx, view) = opened("default");
    cx.simulate_click("forge-ref-picker");
    cx.simulate_click("forge-ref-refs/heads/clean");
    cx.simulate_click("forge-tab-code");
    cx.run_until_parked();
    let before = view.read(|forge| forge.tree_rows());
    let src = before
        .iter()
        .position(|row| row.path == b"src" && row.is_dir())
        .expect("src is a directory of the root");
    assert!(before.iter().all(|row| row.depth == 0));
    cx.simulate_click("forge-tree-src");
    cx.run_until_parked();
    let asked = cx.host().requests::<Ask>();
    assert!(
        asked
            .iter()
            .any(|query| matches!(query, Query::Tree { path, .. } if path == b"src")),
        "expanding reads the folder lazily"
    );
    let after = view.read(|forge| forge.tree_rows());
    // every root row stays where it was; the children sit right under src
    assert_eq!(after[..=src], before[..=src]);
    let children: Vec<_> = after[src + 1..]
        .iter()
        .take_while(|row| row.depth == 1)
        .collect();
    assert!(!children.is_empty(), "{after:?}");
    assert!(children.iter().all(|row| row.path.starts_with(b"src/")));
    assert_eq!(after[src + 1 + children.len()..], before[src + 1..]);
    let child = path_of(&children[0].path);
    assert!(cx.find(&format!("forge-tree-{child}")).is_some());
    // another tab and back: the folder is still open
    cx.simulate_click("forge-tab-readme");
    cx.run_until_parked();
    cx.simulate_click("forge-tab-code");
    cx.run_until_parked();
    view.read(|forge| assert!(forge.nav().expanded.contains(b"src".as_slice())));
    assert!(cx.find(&format!("forge-tree-{child}")).is_some());
    // pressing it again folds it
    cx.simulate_click("forge-tree-src");
    cx.run_until_parked();
    assert_eq!(view.read(|forge| forge.tree_rows()), before);
}

/// A folder's row is called by its name alone, open or shut: the drawn
/// `▸`/`▾` is no word of it, and `aria_expanded` says which.
#[test]
fn a_folder_row_is_named_without_its_glyph() {
    let (mut cx, _) = opened("default");
    cx.simulate_click("forge-ref-picker");
    cx.simulate_click("forge-ref-refs/heads/clean");
    cx.simulate_click("forge-tab-code");
    cx.run_until_parked();
    let aria = |cx: &TestAppContext| match cx.find("forge-tree-src") {
        Some(wire::Node::Container(row)) => row.interactivity.aria.clone(),
        _ => panic!("the src row"),
    };
    let shut = aria(&cx);
    assert_eq!(shut.label.as_deref(), Some("src"));
    assert_eq!(shut.expanded, Some(false));
    cx.simulate_click("forge-tree-src");
    cx.run_until_parked();
    let open = aria(&cx);
    assert_eq!(open.label.as_deref(), Some("src"));
    assert_eq!(open.expanded, Some(true));
}

fn path_of(path: &[u8]) -> String {
    String::from_utf8_lossy(path).into_owned()
}

#[test]
fn the_tree_walks_by_keyboard() {
    use crate::tree::Key;
    let (mut cx, view) = opened("default");
    cx.simulate_click("forge-ref-picker");
    cx.simulate_click("forge-ref-refs/heads/clean");
    cx.simulate_click("forge-tab-code");
    cx.run_until_parked();
    let press = |cx: &mut TestAppContext, key| {
        cx.update(&view, |forge, _, cx| forge.tree_key(key, cx));
        cx.run_until_parked();
    };
    let cursor = |cx: &mut TestAppContext| {
        let _ = cx;
        view.read(|forge| forge.nav().cursor.clone().map(|path| path_of(&path)))
    };
    // directories sort first: the first row, src, is the active one before
    // any key, and ↑ there stays
    press(&mut cx, Key::Up);
    assert_eq!(cursor(&mut cx).as_deref(), Some("src"));
    // the focused tree tells assistive technology which row is active
    let Some(wire::Node::Container(row)) = cx.find("forge-tree-src") else {
        panic!("the src row");
    };
    assert!(row.interactivity.aria.active_descendant && !row.interactivity.focusable);
    press(&mut cx, Key::Right);
    view.read(|forge| assert!(forge.nav().expanded.contains(b"src".as_slice())));
    press(&mut cx, Key::Right);
    let child = cursor(&mut cx).expect("stepped into src");
    assert!(child.starts_with("src/"), "{child}");
    press(&mut cx, Key::Left);
    assert_eq!(cursor(&mut cx).as_deref(), Some("src"), "left steps out");
    press(&mut cx, Key::Left);
    view.read(|forge| assert!(forge.nav().expanded.is_empty(), "left folds"));
    press(&mut cx, Key::Down);
    press(&mut cx, Key::Enter);
    view.read(|forge| {
        let (path, _) = forge.nav().blob.clone().expect("enter opens the file");
        assert_eq!(Some(path), forge.nav().cursor.clone());
    });
    press(&mut cx, Key::Up);
    assert_eq!(cursor(&mut cx).as_deref(), Some("src"));
}

#[test]
fn an_oversize_blob_is_a_header_not_a_body() {
    let (mut cx, view) = opened("default");
    cx.simulate_click("forge-ref-picker");
    cx.simulate_click("forge-ref-refs/heads/clean");
    cx.simulate_click("forge-tab-code");
    cx.run_until_parked();
    cx.update(&view, |forge, _, cx| {
        forge.open_file(
            b"large.txt".to_vec(),
            "b90a09e55e43808905fe881245853c1b35b3fb82".into(),
            cx,
        )
    });
    cx.run_until_parked();
    assert!(cx.has_text("Too large to show"), "{:?}", cx.texts());
    assert!(cx.find("forge-blob-lines").is_none());
    cx.update(&view, |forge, _, cx| {
        forge.open_file(
            b"image.bin".to_vec(),
            "95d586e774a04676a07a142f0e2f97a4f32562cb".into(),
            cx,
        )
    });
    cx.run_until_parked();
    assert!(cx.has_text("Binary file"));
}

#[test]
fn commits_follows_the_cursor_and_opens_one_commit_with_its_diff() {
    let (mut cx, view) = opened("default");
    cx.simulate_click("forge-tab-commits");
    cx.run_until_parked();
    // `log` carries a next cursor; the second page is the root commit.
    let asked = cx.host().requests::<Ask>();
    assert!(
        asked.iter().any(|query| matches!(
            query,
            Query::Log {
                page: PageRequest { after: Some(_), .. },
                ..
            }
        )),
        "the log follows its cursor"
    );
    view.read(|forge| {
        let Some(Reply::Log { page, .. }) = forge.ready(&Query::Log {
            repo: "project".into(),
            from: forge.revision(),
            exclude: None,
            page: crate::queries::PAGE,
        }) else {
            panic!("the log landed");
        };
        assert_eq!(page.items.len(), 2, "both pages are one list");
        assert!(page.next.is_none());
    });
    assert!(cx.has_text("Feature"), "{:?}", cx.texts());
    cx.simulate_click("forge-commit-26607f522099476177a45a8058a93108fba5a84d");
    cx.run_until_parked();
    assert!(
        cx.has_text("Feature\n\nReview these bytes.\n"),
        "{:?}",
        cx.texts()
    );
    assert!(cx.has_text("ebfb8b62…4e16"), "the parent is named");
    assert!(
        cx.find("forge-commit-diff-file-src/lib.rs").is_some(),
        "a commit's diff names its files above the rows"
    );
    assert!(cx.find("forge-commit-diff").is_some());
    cx.simulate_click("forge-commit-close");
    cx.run_until_parked();
    view.read(|forge| assert!(forge.nav().commit.is_none()));
}

#[test]
fn refs_carry_their_distance_from_the_default_head_and_open_a_draft() {
    let (mut cx, view) = opened("default");
    cx.simulate_click("forge-tab-refs");
    cx.run_until_parked();
    assert!(cx.has_text("clean"), "{:?}", cx.texts());
    assert!(
        cx.texts()
            .iter()
            .any(|text| text.contains("1 ahead") && text.contains("fast-forward")),
        "{:?}",
        cx.texts()
    );
    assert!(
        cx.texts()
            .iter()
            .any(|text| text.contains("forbids force pushes and ref deletions"))
    );
    // Compare sits beside the row's press, not inside it
    let row = wire::Node::Container(control(&cx, "forge-ref-row-clean"));
    assert!(holds(&row, "forge-ref-row-clean-open") && holds(&row, "forge-compare-clean"));
    let open = wire::Node::Container(control(&cx, "forge-ref-row-clean-open"));
    assert!(!holds(&open, "forge-compare-clean"));
    cx.simulate_click("forge-compare-clean");
    cx.run_until_parked();
    view.read(|forge| {
        let form = forge.form.as_ref().expect("a change draft");
        assert_eq!(form.from, b"refs/heads/clean".to_vec());
        assert_eq!(form.into, b"refs/heads/main".to_vec());
    });
    assert!(cx.has_text("New change"));
}

#[test]
fn settings_shows_only_what_the_contract_exposes_and_grants_by_account() {
    let (mut cx, view) = opened_as("default", 1);
    cx.simulate_click("forge-tab-settings");
    cx.run_until_parked();
    assert!(cx.has_text("Allow force pushes"));
    assert!(cx.has_text("Allow ref deletion"));
    assert!(cx.has_text("Wren"), "the granted writer resolves to a name");
    cx.simulate_click("forge-settings-force");
    cx.simulate_click("forge-settings-head");
    cx.simulate_click("forge-settings-head-clean");
    cx.simulate_click("forge-settings-save");
    cx.run_until_parked();
    assert!(cx.host().requests::<SubmitForge>().iter().any(|op| matches!(
        op,
        Op::Configure { repo, settings }
            if repo == "project" && settings.allow_force && settings.head == b"refs/heads/clean"
    )));
    cx.simulate_input("forge-settings-grant-input", "acct:1");
    cx.simulate_click("forge-settings-grant");
    cx.run_until_parked();
    assert!(cx.host().requests::<SubmitForge>().iter().any(
        |op| matches!(op, Op::Grant { principal, .. } if *principal == forge::Principal::Account(1))
    ));
    // a name searches the roster; picking fills the number Grant sends
    cx.simulate_input("forge-settings-grant-input", "ra");
    cx.run_until_parked();
    assert!(cx.find("forge-grant-pick-2").is_some(), "Rae matches");
    assert!(cx.find("forge-grant-pick-1").is_none(), "Ada does not");
    assert!(
        cx.find(&format!("forge-grant-pick-{FORGE}")).is_none(),
        "no module"
    );
    cx.simulate_click("forge-grant-pick-2");
    cx.run_until_parked();
    let filled = view.read(|forge| {
        forge
            .repo_settings
            .as_ref()
            .map(|form| form.grant.text.clone())
    });
    assert_eq!(filled.as_deref(), Some("2"));
    assert!(
        cx.find("forge-grant-pick-2").is_none(),
        "a number searches nothing"
    );
    cx.simulate_click("forge-settings-grant");
    cx.run_until_parked();
    assert!(cx.host().requests::<SubmitForge>().iter().any(
        |op| matches!(op, Op::Grant { principal, .. } if *principal == forge::Principal::Account(2))
    ));
    cx.simulate_click("forge-settings-revoke-acct-9");
    cx.run_until_parked();
    assert!(
        cx.host().requests::<SubmitForge>().iter().any(
            |op| matches!(op, Op::Revoke { principal, .. } if *principal == forge::Principal::Account(9))
        )
    );
}

#[test]
fn the_narrow_window_folds_the_rail_and_the_dock_into_toggles() {
    let (mut cx, view) = opened("default");
    cx.simulate_measure("forge-viewport", 720., 600.);
    cx.run_until_parked();
    view.read(|forge| assert!(forge.layout.narrow()));
    assert!(cx.find("forge-toggle-rail").is_some());
    assert!(cx.find("forge-rail").is_none(), "the rail folds away");
    cx.simulate_click("forge-toggle-rail");
    cx.run_until_parked();
    assert!(cx.find("forge-rail").is_some());
}

/// A window opens at 60% of the desk, 768 px on a 1280 one: the list keeps
/// its table there and down to the view's 640, header and a column per
/// fact, with no bar over it. Only under the table's own width does a row
/// put its facts on one line under its name, the refs with their unit.
#[test]
fn the_repositories_keep_their_table_at_the_width_a_window_opens() {
    let (mut cx, _) = booted("default");
    let column = |cx: &TestAppContext| {
        control(cx, "forge-repo-project-activity-cell")
            .style
            .size
            .width
    };
    for width in [640., 768.] {
        cx.simulate_measure("forge-viewport", width, 600.);
        cx.run_until_parked();
        assert!(
            cx.find("forge-repos-columns").is_some(),
            "header at {width}"
        );
        assert!(column(&cx).is_some(), "activity column at {width}");
        assert!(cx.find("forge-narrow-bar").is_none(), "bar at {width}");
    }
    cx.simulate_measure("forge-viewport", 600., 600.);
    cx.run_until_parked();
    assert!(cx.find("forge-repos-columns").is_none());
    assert!(
        column(&cx).is_none(),
        "a wrapped fact is as wide as it reads"
    );
    let Reply::Repos { page, .. } = reply("repos") else {
        panic!("the repos fixture")
    };
    for info in &page.items {
        let refs = ducktape_view_guest::design::plural(info.repo.refs_count, "ref", "refs");
        assert!(cx.has_text(&refs), "{refs}: {:?}", cx.texts());
    }
}

/// The rail is one list box: the open repository is its active row on
/// entry, ↑ ↓ move, Enter opens the active repository.
#[test]
fn the_rail_is_a_list_box_whose_enter_opens_the_active_repository() {
    let (mut cx, view) = opened("default");
    let rail = cx.interactivity("forge-rail-list");
    assert_eq!(rail.role, Some(ducktape_view_guest::Role::ListBox));
    assert!(rail.focusable && rail.tab_stop == Some(true));
    let project = cx.interactivity("forge-rail-repo-project");
    assert_eq!(project.role, Some(ducktape_view_guest::Role::ListBoxOption));
    assert!(!project.focusable && project.aria.active_descendant);
    assert_eq!(project.aria.selected, Some(true));
    let rows: Vec<String> = cx
        .find("forge-rail-list")
        .expect("the rail")
        .children()
        .iter()
        .filter_map(|row| row.key().map(str::to_owned))
        .collect();
    cx.simulate_focus("forge-rail-list");
    cx.simulate_key_down("forge-rail-list", "end");
    let last = rows.last().expect("a repository");
    assert!(cx.interactivity(last).aria.active_descendant);
    cx.update(&view, |forge, _, cx| forge.open_repos(cx));
    cx.run_until_parked();
    assert!(
        cx.find("forge-rail-list").is_none(),
        "the list screen has no rail"
    );
    cx.simulate_click("forge-repo-project-open");
    cx.run_until_parked();
    cx.simulate_focus("forge-rail-list");
    cx.simulate_key_down("forge-rail-list", "home");
    cx.simulate_key_down("forge-rail-list", "enter");
    cx.run_until_parked();
    let name = rows[0].trim_start_matches("forge-rail-repo-");
    view.read(|forge| assert_eq!(forge.nav().repo.as_deref(), Some(name)));
}

#[test]
fn a_snapshot_restores_the_same_screen_without_replaying_events() {
    let (mut cx, view) = opened("default");
    cx.simulate_click("forge-tab-changes");
    cx.run_until_parked();
    cx.simulate_click("forge-change-1");
    cx.run_until_parked();
    cx.update(&view, |forge, _, cx| {
        forge.open_change_tab(ChangeTab::Files, cx);
        forge.toggle_viewed(b"src/lib.rs", cx);
    });
    cx.run_until_parked();
    let snapshot = cx.snapshot().unwrap();

    let mut restored = TestAppContext::new();
    configure(&mut restored, "default");
    let view = restored.restore::<Forge>(&snapshot).unwrap();
    restored.run_until_parked();
    view.read(|forge| {
        assert_eq!(forge.nav().repo.as_deref(), Some("project"));
        assert_eq!(forge.nav().change, Some(1));
        assert_eq!(forge.nav().change_tab, ChangeTab::Files);
        assert!(forge.viewed.contains("project#1:src/lib.rs"));
    });
    assert!(
        !restored.host().requests::<Ask>().is_empty(),
        "a restored view reads again"
    );
}

#[test]
fn judgment_is_its_own_query_keyed_by_the_readers_account() {
    let (mut cx, view) = opened("judgment");
    cx.simulate_click("forge-tab-changes");
    cx.run_until_parked();
    cx.simulate_click("forge-filter-judgment");
    cx.run_until_parked();
    view.read(|forge| assert_eq!(forge.filter, Filter::Judgment));
    assert!(
        cx.host()
            .requests::<Ask>()
            .iter()
            .any(|query| matches!(query, Query::Judgment { principal, .. } if *principal == forge::Principal::Account(2))),
        "the reader is their account, from the session"
    );
    assert!(cx.has_text("review requested"), "{:?}", cx.texts());
}

#[test]
fn an_empty_judgment_says_nothing_waits_on_you() {
    let (mut cx, _) = opened("judgment-empty");
    cx.simulate_click("forge-tab-changes");
    cx.run_until_parked();
    cx.simulate_click("forge-filter-judgment");
    cx.run_until_parked();
    assert!(cx.has_text("Nothing waits on you"), "{:?}", cx.texts());
}

/// Opens change #1 of `project` on one of its tabs.
pub(crate) fn change_screen(mode: &'static str, tab: ChangeTab) -> (TestAppContext, Entity<Forge>) {
    change_screen_as(mode, tab, 2)
}

pub(crate) fn change_screen_as(
    mode: &'static str,
    tab: ChangeTab,
    account: u64,
) -> (TestAppContext, Entity<Forge>) {
    let (mut cx, view) = opened_as(mode, account);
    cx.simulate_click("forge-tab-changes");
    cx.run_until_parked();
    cx.simulate_click("forge-change-1");
    cx.run_until_parked();
    cx.simulate_click(&format!("forge-change-tab-{}", tab.slug()));
    cx.run_until_parked();
    (cx, view)
}

#[path = "change_tests.rs"]
mod change_tests;

#[path = "screen_tests.rs"]
mod screen_tests;

#[test]
fn copy_puts_the_address_on_the_clipboard_without_opening_the_repository() {
    let (mut cx, view) = booted("default");
    let copied = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
    let seen = copied.clone();
    cx.host()
        .handle::<ducktape_view_guest::methods::ClipboardWrite>(move |text| {
            *seen.borrow_mut() = text;
            Ok(())
        });
    cx.simulate_click("forge-repo-project-copy");
    cx.run_until_parked();
    assert_eq!(*copied.borrow(), "duck://testnet-0a1b2c3d/forge/project");
    assert!(cx.has_text("Copied"));
    view.read(|forge| assert!(forge.nav.repo.is_none(), "copy is not open"));
}

/// A row's Copy and block link, and the open repository's block link, are
/// at least 24 px each way: the box's own floor, so the bounds the door's
/// AX-017 reads cannot come in under it, whatever the text inside.
#[test]
fn the_small_press_targets_are_at_least_24_px_each_way() {
    use ducktape_view_guest::px;
    let floor = |cx: &TestAppContext, key: &str| {
        let style = control(cx, key).style;
        (style.min_size.width, style.min_size.height)
    };
    let (cx, _) = booted("default");
    for key in ["forge-repo-project-copy", "forge-repo-project-activity"] {
        assert_eq!(
            floor(&cx, key),
            (Some(px(24.).into()), Some(px(24.).into())),
            "{key}"
        );
    }
    let (cx, _) = opened("default");
    assert_eq!(
        floor(&cx, "forge-repo-activity"),
        (Some(px(24.).into()), Some(px(24.).into()))
    );
}

/// The key's roled container: its role, focus and press.
pub(crate) fn control(cx: &TestAppContext, key: &str) -> wire::ContainerNode {
    match cx.find(key) {
        Some(wire::Node::Container(node)) => node.clone(),
        other => panic!("{key} is no container: {other:?}"),
    }
}

/// Whether `key` lies anywhere under `node`.
pub(crate) fn holds(node: &wire::Node, key: &str) -> bool {
    node.children()
        .iter()
        .any(|child| child.key() == Some(key) || holds(child, key))
}

#[test]
fn a_repository_row_is_a_grid_row_whose_press_is_a_button_beside_its_controls() {
    let (cx, _view) = booted("default");
    let row = control(&cx, "forge-repo-project");
    assert_eq!(row.interactivity.role, Some(ducktape_view_guest::Role::Row));
    assert!(!row.interactivity.focusable && row.interactivity.on_click.is_none());
    let open = control(&cx, "forge-repo-project-open");
    assert_eq!(
        open.interactivity.role,
        Some(ducktape_view_guest::Role::Button)
    );
    // the grid holds the focus; its first row's press is active on entry
    assert!(!open.interactivity.focusable && open.interactivity.on_click.is_some());
    assert!(open.interactivity.aria.active_descendant);
    let open = wire::Node::Container(open);
    for sibling in ["forge-repo-project-copy", "forge-repo-project-activity"] {
        assert!(!holds(&open, sibling), "{sibling} is inside the press");
    }
}

/// The repository list is one grid: ↓ moves to the next repository, → to
/// its Copy cell, and Enter presses the cell, so the address lands on
/// the clipboard without opening the repository.
#[test]
fn the_repositories_grid_walks_cells_and_enter_presses_the_active_one() {
    let (mut cx, view) = booted("default");
    cx.simulate_measure("forge-viewport", 1000., 600.);
    cx.run_until_parked();
    let copied = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
    let seen = copied.clone();
    cx.host()
        .handle::<ducktape_view_guest::methods::ClipboardWrite>(move |text| {
            *seen.borrow_mut() = text;
            Ok(())
        });
    let grid = cx.interactivity("forge-repos-list");
    assert_eq!(grid.role, Some(ducktape_view_guest::Role::Grid));
    assert!(grid.focusable && grid.tab_stop == Some(true));
    let rows: Vec<String> = cx
        .find("forge-repos-list")
        .expect("the grid")
        .children()
        .iter()
        .filter(|node| {
            node.interactivity()
                .is_some_and(|row| row.role == Some(ducktape_view_guest::Role::Row))
        })
        .filter_map(|node| node.key().map(str::to_owned))
        .filter(|key| key != "forge-repos-columns")
        .collect();
    assert!(!rows.is_empty(), "{:?}", cx.texts());
    let header = cx.interactivity("forge-repos-columns");
    assert_eq!(header.role, Some(ducktape_view_guest::Role::Row));
    assert_eq!(
        cx.interactivity("forge-repos-column-Name").role,
        Some(ducktape_view_guest::Role::ColumnHeader)
    );
    // the second repository when there is one, else the first
    let at = usize::from(rows.len() > 1);
    cx.simulate_focus("forge-repos-list");
    if at == 1 {
        cx.simulate_key_down("forge-repos-list", "down");
    }
    let name = rows[at].trim_start_matches("forge-repo-").to_owned();
    assert!(
        cx.interactivity(&format!("forge-repo-{name}-open"))
            .aria
            .active_descendant
    );
    cx.simulate_key_down("forge-repos-list", "right");
    let copy = cx.interactivity(&format!("forge-repo-{name}-copy"));
    assert!(copy.aria.active_descendant && !copy.focusable);
    assert!(
        !cx.interactivity(&format!("forge-repo-{name}-open"))
            .aria
            .active_descendant
    );
    cx.simulate_key_down("forge-repos-list", "enter");
    cx.run_until_parked();
    assert_eq!(
        *copied.borrow(),
        format!("duck://testnet-0a1b2c3d/forge/{name}")
    );
    assert!(cx.has_text("Copied"));
    view.read(|forge| assert!(forge.nav.repo.is_none(), "copy is not open"));
    // → again: the activity link; Enter opens it in Explorer
    cx.simulate_key_down("forge-repos-list", "right");
    assert!(
        cx.interactivity(&format!("forge-repo-{name}-activity"))
            .aria
            .active_descendant
    );
    cx.simulate_key_down("forge-repos-list", "enter");
    cx.run_until_parked();
    assert_eq!(
        cx.host()
            .requests::<ducktape_view_guest::methods::LinkOpen>()
            .len(),
        1
    );
    // Home: back to the press; Enter opens the repository
    cx.simulate_key_down("forge-repos-list", "home");
    cx.simulate_key_down("forge-repos-list", "enter");
    cx.run_until_parked();
    view.read(|forge| assert_eq!(forge.nav().repo.as_deref(), Some(name.as_str())));
}

/// The ref picker's menu is one Tab stop in a modal overlay: it takes the
/// keys on open with its first item active, ↓ then Enter picks a ref, and
/// Esc closes it and gives the keys back to the button.
#[test]
fn the_ref_menu_takes_the_keys_on_open_and_gives_them_back() {
    let (mut cx, view) = opened("default");
    let focused = |cx: &TestAppContext| -> Vec<String> {
        cx.host()
            .requests::<ducktape_view_guest::methods::HostWidget>()
            .iter()
            .filter_map(|command| match command {
                wire::WidgetCommand::Focus { target } => target.last().and_then(|id| match id {
                    wire::ElementIdWire::Name(name) => Some(name.to_string()),
                    _ => None,
                }),
                _ => None,
            })
            .collect()
    };
    cx.simulate_click("forge-ref-picker");
    cx.run_until_parked();
    assert_eq!(
        focused(&cx).last().map(String::as_str),
        Some("forge-ref-picker-menu")
    );
    let menu = cx.interactivity("forge-ref-picker-menu");
    assert_eq!(menu.role, Some(ducktape_view_guest::Role::Menu));
    assert!(menu.focusable && menu.on_key_down.is_some());
    assert!(
        cx.find("forge-menu-overlay").is_some(),
        "the menu floats in the overlay"
    );
    let items: Vec<String> = {
        let mut items = Vec::new();
        fn gather(node: &wire::Node, items: &mut Vec<String>) {
            if node
                .interactivity()
                .is_some_and(|item| item.role == Some(ducktape_view_guest::Role::MenuItemRadio))
            {
                items.push(node.key().unwrap_or_default().to_owned());
            }
            node.children()
                .iter()
                .for_each(|child| gather(child, items));
        }
        gather(
            cx.find("forge-ref-picker-menu").expect("the menu"),
            &mut items,
        );
        items
    };
    assert!(items.len() >= 2, "{items:?}");
    assert!(cx.interactivity(&items[0]).aria.active_descendant);
    assert!(!cx.interactivity(&items[0]).focusable);
    cx.simulate_key_down("forge-ref-picker-menu", "escape");
    cx.run_until_parked();
    assert!(cx.find("forge-ref-picker-menu").is_none());
    assert_eq!(
        focused(&cx).last().map(String::as_str),
        Some("forge-ref-picker")
    );
    cx.simulate_click("forge-ref-picker");
    cx.run_until_parked();
    cx.simulate_key_down("forge-ref-picker-menu", "down");
    assert!(cx.interactivity(&items[1]).aria.active_descendant);
    cx.simulate_key_down("forge-ref-picker-menu", "enter");
    cx.run_until_parked();
    let picked = items[1].trim_start_matches("forge-ref-").to_owned();
    view.read(|forge| {
        assert_eq!(forge.nav().rev.as_deref(), Some(picked.as_bytes()));
        assert_eq!(forge.menu, None);
    });
    assert_eq!(
        focused(&cx).last().map(String::as_str),
        Some("forge-ref-picker")
    );
}

/// Every element of the window keeps its path while a dropdown is open and
/// after it closes: the host keys a list's scroll and a field's state by
/// the ids above them, so a menu must not move the window under itself.
#[test]
fn a_dropdown_leaves_the_window_where_it_was() {
    fn paths(
        node: &wire::Node,
        above: &mut Vec<wire::ElementIdWire>,
        out: &mut Vec<Vec<wire::ElementIdWire>>,
    ) {
        let id = node.identity().cloned();
        if let Some(id) = &id {
            above.push(id.clone());
            out.push(above.clone());
        }
        node.children()
            .iter()
            .for_each(|child| paths(child, above, out));
        if id.is_some() {
            above.pop();
        }
    }
    let window = |cx: &TestAppContext| {
        let mut out = Vec::new();
        paths(cx.root(), &mut Vec::new(), &mut out);
        let root = wire::ElementIdWire::Name("forge".into());
        out.retain(|path| path.contains(&root));
        out
    };
    let (mut cx, _view) = opened("default");
    let closed = window(&cx);
    cx.simulate_click("forge-ref-picker");
    cx.run_until_parked();
    assert!(
        cx.find("forge-ref-picker-menu").is_some(),
        "the menu is open"
    );
    assert_eq!(window(&cx), closed);
    cx.simulate_key_down("forge-ref-picker-menu", "escape");
    cx.run_until_parked();
    assert!(
        cx.find("forge-ref-picker-menu").is_none(),
        "the menu is closed"
    );
    assert_eq!(window(&cx), closed);
}

/// The file tree claims a row before any key, and End takes the cursor to
/// the last row.
#[test]
fn the_tree_claims_a_row_on_entry_and_end_reaches_the_last() {
    let (mut cx, view) = opened("default");
    // the default ref carries no commits; `clean` has a tree
    cx.simulate_click("forge-ref-picker");
    cx.simulate_click("forge-ref-refs/heads/clean");
    cx.simulate_click("forge-tab-code");
    cx.run_until_parked();
    let tree = cx.interactivity("forge-tree-rows");
    assert_eq!(tree.role, Some(ducktape_view_guest::Role::Tree));
    assert!(tree.focusable && tree.tab_stop == Some(true));
    let rows = view.read(|forge| forge.tree_rows());
    let first = format!("forge-tree-{}", path_of(&rows[0].path));
    let last = format!("forge-tree-{}", path_of(&rows[rows.len() - 1].path));
    view.read(|forge| assert_eq!(forge.nav().cursor, None));
    assert!(cx.interactivity(&first).aria.active_descendant);
    cx.simulate_focus("forge-tree-rows");
    cx.simulate_key_down("forge-tree-rows", "end");
    cx.run_until_parked();
    view.read(|forge| {
        assert_eq!(
            forge.nav().cursor.as_deref(),
            Some(rows[rows.len() - 1].path.as_slice())
        )
    });
    assert!(cx.interactivity(&last).aria.active_descendant);
    assert!(!cx.interactivity(&first).aria.active_descendant);
    cx.simulate_key_down("forge-tree-rows", "home");
    cx.run_until_parked();
    assert!(cx.interactivity(&first).aria.active_descendant);
}

#[test]
fn a_forge_link_opens_its_repository() {
    let mut cx = TestAppContext::new();
    configure(&mut cx, "default");
    let props = cx.host().stream::<HostSession>();
    let routes = cx
        .host()
        .stream::<ducktape_view_guest::methods::HostRoute>();
    let view = cx.open::<Forge>();
    cx.run_until_parked();
    props.send(Session {
        connected: true,
        chain_id: "testnet#0a1b2c3d".into(),
        ..Session::default()
    });
    cx.run_until_parked();
    routes.send("project".into());
    cx.run_until_parked();
    view.read(|forge| assert_eq!(forge.nav().repo.as_deref(), Some("project")));
    // a change's room links here as `<repo>/<n>`
    routes.send("project/1".into());
    cx.run_until_parked();
    view.read(|forge| {
        assert_eq!(
            (forge.nav().repo.as_deref(), forge.nav().change),
            (Some("project"), Some(1))
        )
    });
    // a route forge does not read falls back to the list
    routes.send("project/extra".into());
    cx.run_until_parked();
    view.read(|forge| assert!(forge.nav().repo.is_none()));
}

#[test]
fn the_commit_list_is_a_list_box_whose_enter_opens_the_active_commit() {
    let (mut cx, view) = opened("default");
    cx.simulate_click("forge-tab-commits");
    cx.run_until_parked();
    let list = cx.interactivity("forge-log-list");
    assert_eq!(list.role, Some(ducktape_view_guest::Role::ListBox));
    assert!(list.focusable && list.tab_stop == Some(true));
    let first = cx.interactivity("forge-commit-26607f522099476177a45a8058a93108fba5a84d");
    assert_eq!(first.role, Some(ducktape_view_guest::Role::ListBoxOption));
    assert!(!first.focusable && first.aria.active_descendant);
    cx.simulate_focus("forge-log-list");
    cx.simulate_key_down("forge-log-list", "down");
    let second: String = cx
        .find("forge-log")
        .expect("the log")
        .children()
        .iter()
        .filter_map(|row| row.key())
        .nth(1)
        .expect("a second commit")
        .to_owned();
    assert!(cx.interactivity(&second).aria.active_descendant);
    cx.simulate_key_down("forge-log-list", "enter");
    cx.run_until_parked();
    let oid = second.trim_start_matches("forge-commit-").to_owned();
    view.read(|forge| assert_eq!(forge.nav().commit.as_deref(), Some(oid.as_str())));
}

#[test]
fn the_change_list_is_a_list_box_whose_enter_opens_the_active_change() {
    let (mut cx, view) = opened("default");
    cx.simulate_click("forge-tab-changes");
    cx.run_until_parked();
    let list = cx.interactivity("forge-changes-list");
    assert_eq!(list.role, Some(ducktape_view_guest::Role::ListBox));
    assert!(list.focusable && list.tab_stop == Some(true));
    let row = cx.interactivity("forge-change-1");
    assert_eq!(row.role, Some(ducktape_view_guest::Role::ListBoxOption));
    assert!(!row.focusable && row.aria.active_descendant);
    cx.simulate_focus("forge-changes-list");
    cx.simulate_key_down("forge-changes-list", "enter");
    cx.run_until_parked();
    view.read(|forge| assert_eq!(forge.nav().change, Some(1)));
}

#[test]
fn the_ref_list_is_a_grid_whose_right_reaches_compare() {
    let (mut cx, view) = opened("default");
    cx.simulate_click("forge-tab-refs");
    cx.run_until_parked();
    let grid = cx.interactivity("forge-refs-list");
    assert_eq!(grid.role, Some(ducktape_view_guest::Role::Grid));
    assert!(grid.focusable && grid.tab_stop == Some(true));
    let rows: Vec<String> = cx
        .find("forge-refs-list")
        .expect("the refs")
        .children()
        .iter()
        .filter_map(|row| row.key().map(str::to_owned))
        .filter(|key| key.starts_with("forge-ref-row-"))
        .collect();
    assert!(
        cx.interactivity(&format!("{}-open", rows[0]))
            .aria
            .active_descendant
    );
    // ↓ to the second ref, → to its Compare, Enter starts a change from it
    cx.simulate_focus("forge-refs-list");
    cx.simulate_key_down("forge-refs-list", "down");
    let branch = &rows[1];
    let label = &branch["forge-ref-row-".len()..];
    assert!(
        cx.interactivity(&format!("{branch}-open"))
            .aria
            .active_descendant
    );
    cx.simulate_key_down("forge-refs-list", "right");
    let compare = cx.interactivity(&format!("forge-compare-{label}"));
    assert!(compare.aria.active_descendant && !compare.focusable);
    cx.simulate_key_down("forge-refs-list", "enter");
    cx.run_until_parked();
    view.read(|forge| {
        let form = forge.form.as_ref().expect("a change draft");
        assert_eq!(form.from, format!("refs/heads/{label}").into_bytes());
    });
}

/// A reader who may not write sees a disabled Compare that is no cell: →
/// stays on the ref, and Enter starts no change.
#[test]
fn a_reader_who_may_not_write_has_no_compare_cell() {
    let (mut cx, view) = seated(b"stranger", None);
    view.read(|forge| assert!(!forge.may_write()));
    cx.simulate_click("forge-tab-refs");
    cx.run_until_parked();
    let rows: Vec<String> = cx
        .find("forge-refs-list")
        .expect("the refs")
        .children()
        .iter()
        .filter_map(|row| row.key().map(str::to_owned))
        .filter(|key| key.starts_with("forge-ref-row-"))
        .collect();
    cx.simulate_focus("forge-refs-list");
    cx.simulate_key_down("forge-refs-list", "down");
    cx.simulate_key_down("forge-refs-list", "right");
    let branch = &rows[1];
    let label = &branch["forge-ref-row-".len()..];
    assert!(
        cx.interactivity(&format!("{branch}-open"))
            .aria
            .active_descendant,
        "the ref stays the active cell"
    );
    let compare = cx.interactivity(&format!("forge-compare-{label}"));
    assert!(compare.aria.disabled == Some(true) && !compare.aria.active_descendant);
    cx.simulate_key_down("forge-refs-list", "enter");
    cx.run_until_parked();
    view.read(|forge| assert!(forge.form.is_none(), "no draft for a reader"));
}

/// The default head and a tag have no Compare, and still sit in the refs
/// grid as rows of cells: a `Row` whose one cell is the ref's press, never
/// a list box's option under a grid (AX-105, the census's Forge: refs).
#[test]
fn a_ref_without_compare_is_a_grid_row_of_one_cell() {
    let (mut cx, view) = opened("default");
    cx.simulate_click("forge-tab-refs");
    cx.run_until_parked();
    let rows: Vec<String> = cx
        .find("forge-refs-list")
        .expect("the refs")
        .children()
        .iter()
        .filter_map(|row| row.key().map(str::to_owned))
        .filter(|key| key.starts_with("forge-ref-row-"))
        .collect();
    for label in ["main", "v1"] {
        let key = format!("forge-ref-row-{label}");
        assert_eq!(
            cx.interactivity(&key).role,
            Some(ducktape_view_guest::Role::Row),
            "{key}"
        );
        assert_eq!(
            cx.interactivity(&format!("{key}-open-cell")).role,
            Some(ducktape_view_guest::Role::GridCell),
            "{key}"
        );
        assert_eq!(
            cx.interactivity(&format!("{key}-open")).role,
            Some(ducktape_view_guest::Role::Button),
            "{key}"
        );
        assert!(cx.find(&format!("forge-compare-{label}")).is_none());
    }
    // the browsed default head says so on its press
    assert_eq!(
        cx.interactivity("forge-ref-row-main-open").aria.current,
        Some(ducktape_view_guest::accesskit::AriaCurrent::True)
    );
    let at = |label: &str| {
        rows.iter()
            .position(|key| *key == format!("forge-ref-row-{label}"))
            .unwrap_or_else(|| panic!("{label} is listed: {rows:?}"))
    };
    // ↓ to the default head, whose press claims; → finds no second cell
    cx.simulate_focus("forge-refs-list");
    for _ in 0..at("main") {
        cx.simulate_key_down("forge-refs-list", "down");
    }
    cx.simulate_key_down("forge-refs-list", "right");
    assert!(
        cx.interactivity("forge-ref-row-main-open")
            .aria
            .active_descendant
    );
    // ↓ on to the tag, and Enter browses it
    for _ in at("main")..at("v1") {
        cx.simulate_key_down("forge-refs-list", "down");
    }
    cx.simulate_key_down("forge-refs-list", "enter");
    cx.run_until_parked();
    view.read(|forge| assert_eq!(forge.head_name(), b"refs/tags/v1".to_vec()));
}

// ---------- what one block costs a repository page ----------

/// The variant name of a forge query, for a count by kind.
fn kind(query: &Query) -> String {
    let text = format!("{query:?}");
    text.split([' ', '(', '{']).next().unwrap_or("?").to_owned()
}

struct Costed {
    cx: TestAppContext,
    forge_heads: StreamSender<Changes<forge::Forge>>,
    chat_heads: StreamSender<Changes<::chat::Chat>>,
    identity_heads: StreamSender<Changes<Identity>>,
    /// bytes of every forge reply the fake node handed back
    reply_bytes: std::rc::Rc<std::cell::RefCell<usize>>,
}

/// `followed("default")` with the forge reply bytes counted.
fn costed() -> Costed {
    let mut cx = TestAppContext::new();
    configure(&mut cx, "default");
    let reply_bytes = std::rc::Rc::new(std::cell::RefCell::new(0usize));
    let counted = reply_bytes.clone();
    cx.host().handle::<Ask>(move |query| {
        let reply = answer(&query, "default");
        *counted.borrow_mut() += borsh::to_vec(&reply).unwrap().len();
        Ok(reply)
    });
    let forge_heads = cx.host().stream::<Changes<forge::Forge>>();
    let chat_heads = cx.host().stream::<Changes<::chat::Chat>>();
    let identity_heads = cx.host().stream::<Changes<Identity>>();
    let props = cx.host().stream::<HostSession>();
    cx.open::<Forge>();
    cx.run_until_parked();
    props.send(Session {
        signer: abi::hex(b"reviewer"),
        account: Some(2),
        connected: true,
        chain_id: "testnet#0a1b2c3d".into(),
        ..Session::default()
    });
    cx.run_until_parked();
    cx.simulate_click("forge-repo-project-open");
    cx.run_until_parked();
    Costed {
        cx,
        forge_heads,
        chat_heads,
        identity_heads,
        reply_bytes,
    }
}

/// The forge asks one live item costs, by kind, printed with the reply
/// bytes as `AUDIT <label>`.
fn costs(
    costed: &mut Costed,
    label: &str,
    send: impl FnOnce(&Costed),
) -> std::collections::BTreeMap<String, usize> {
    let before = costed.cx.host().requests::<Ask>().len();
    let bytes_before = *costed.reply_bytes.borrow();
    send(costed);
    costed.cx.run_until_parked();
    let asked = costed.cx.host().requests::<Ask>();
    let mut by_kind = std::collections::BTreeMap::new();
    for query in &asked[before..] {
        *by_kind.entry(kind(query)).or_insert(0) += 1;
    }
    eprintln!(
        "AUDIT {label}: {} module.query asks, {} reply bytes, by kind {by_kind:?}",
        asked.len() - before,
        *costed.reply_bytes.borrow() - bytes_before
    );
    by_kind
}

/// What one block costs the repository page: the reads of the tables it
/// wrote to, and nothing else. A review record written re-reads nothing the
/// Readme tab shows (before, any block of any of the three programs re-read
/// all of it: 8 asks, 1,554 bytes); a ref moved re-reads the refs and what
/// hangs off them; the repo record written re-reads the repositories and
/// the activity, so a real review block, which marks the repository active,
/// costs those three; a chat block re-reads no forge read the Readme tab
/// shows; an identity block only the names; and a reopened link (`None`)
/// re-reads everything. On the Judgment filter a real review lands as a
/// forge item and, with the line it posted, a chat item, and each asks the
/// judgment again.
#[test]
fn a_block_costs_the_reads_of_the_tables_it_wrote_to() {
    let mut costed = costed();
    let review = costs(&mut costed, "Readme tab, a review record written", |c| {
        c.forge_heads.send(block(100, vec![review_key()]))
    });
    assert!(review.is_empty(), "{review:?}");
    let real_review = costs(&mut costed, "Readme tab, a real review block", |c| {
        c.forge_heads.send(block(100, real_review_keys()))
    });
    assert_eq!(
        real_review.keys().collect::<Vec<_>>(),
        ["Activity", "Repo", "Repos"],
        "{real_review:?}"
    );
    let pushed = costs(&mut costed, "Readme tab, a ref moved", |c| {
        c.forge_heads.send(block(101, vec![ref_key()]))
    });
    assert_eq!(
        pushed.keys().collect::<Vec<_>>(),
        ["Blob", "Refs", "Tree"],
        "{pushed:?}"
    );
    let repo = forge::tables::REPOS.key(&"project".to_owned());
    let configured = costs(&mut costed, "Readme tab, the repo record written", |c| {
        c.forge_heads.send(block(102, vec![repo]))
    });
    assert_eq!(
        configured.keys().collect::<Vec<_>>(),
        ["Activity", "Repo", "Repos"],
        "{configured:?}"
    );
    let chat = costs(&mut costed, "Readme tab, one Changes<chat> item", |c| {
        c.chat_heads.send(block(103, vec![root_key("general")]))
    });
    assert!(chat.is_empty(), "{chat:?}");
    let names = costed
        .cx
        .host()
        .requests::<ProgramQuery<::chat::Chat>>()
        .len();
    let identity = costs(&mut costed, "Readme tab, one Changes<identity> item", |c| {
        c.identity_heads.send(block(104, Vec::new()))
    });
    assert!(identity.is_empty(), "{identity:?}");
    assert!(
        costed
            .cx
            .host()
            .requests::<ProgramQuery<::chat::Chat>>()
            .len()
            > names,
        "the names re-read"
    );
    let all = costs(&mut costed, "Readme tab, a reopened link (None)", |c| {
        c.forge_heads.send(None)
    });
    assert_eq!(
        all.keys().collect::<Vec<_>>(),
        ["Activity", "Blob", "Refs", "Repo", "Repos", "Tree"],
        "{all:?}"
    );
    assert_eq!(all.values().sum::<usize>(), 8);
    costed.cx.simulate_click("forge-tab-changes");
    costed.cx.run_until_parked();
    costed.cx.simulate_click("forge-filter-judgment");
    costed.cx.run_until_parked();
    let judged = costs(
        &mut costed,
        "Judgment filter, a real review block, the forge item",
        |c| c.forge_heads.send(block(106, real_review_keys())),
    );
    assert_eq!(
        judged.keys().collect::<Vec<_>>(),
        ["Activity", "Judgment", "Repo", "Repos"],
        "{judged:?}"
    );
    let posted = costs(
        &mut costed,
        "Judgment filter, a real review block, the chat item",
        |c| {
            c.chat_heads
                .send(block(106, posted_line_keys("forge:project:1")))
        },
    );
    assert_eq!(
        posted.keys().collect::<Vec<_>>(),
        ["Judgment"],
        "{posted:?}"
    );
}

/// What a reader on "Needs my judgment" waits on is chat's: a chat block
/// that answered a thread (an `ANSWERED` entry written) asks the judgment
/// again, one that only seated a member does not, and a reopened chat link
/// does. Before, a chat block re-read no forge read, and the list stayed
/// stale until a forge block landed.
#[test]
fn a_chat_block_that_answered_a_thread_re_reads_the_judgment() {
    let (mut cx, heads) = followed("judgment");
    cx.simulate_click("forge-tab-changes");
    cx.run_until_parked();
    cx.simulate_click("forge-filter-judgment");
    cx.run_until_parked();
    let judgments = |cx: &TestAppContext| {
        cx.host()
            .requests::<Ask>()
            .iter()
            .filter(|query| matches!(query, Query::Judgment { .. }))
            .count()
    };
    let asked = judgments(&cx);
    assert!(asked > 0, "the filter asked");
    heads
        .chat
        .send(block(100, vec![member_key("forge:project:1")]));
    cx.run_until_parked();
    assert_eq!(judgments(&cx), asked, "a member seated moves no judgment");
    heads
        .chat
        .send(block(101, vec![answered_key("forge:project:1")]));
    cx.run_until_parked();
    assert_eq!(judgments(&cx), asked + 1, "a thread answered");
    heads.chat.send(None);
    cx.run_until_parked();
    assert_eq!(judgments(&cx), asked + 2, "a reopened chat link");
}
