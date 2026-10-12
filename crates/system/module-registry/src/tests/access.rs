// Who changes the registry: the validators' vote, or a maintainer by its
// grant, at least `NOTICE` blocks ahead; and what the vote's word does to
// the grants and to the changes they made.

use guest::abi::role::identity::{Category, Kind, Profile, Standing};
use guest::{AccountNumber, Error, Principal};

use super::*;
use crate::rules::BY;
use crate::{Grant, MAX_PENDING, NOTICE, Pending, Scope};

/// People who maintain things, an agent, and forge's own account.
const CAROL: AccountNumber = 7;
const DAN: AccountNumber = 8;
const EVE: AccountNumber = 9;
const FORGE: AccountNumber = 10;
const AGENT: AccountNumber = 11;
/// The tip the frames below land after: each is made at `TIP + 1`.
const TIP: u64 = 10;
/// The first block a maintainer's change made at `TIP + 1` lands at.
const EARLIEST: u64 = TIP + 1 + NOTICE;

fn profile(number: AccountNumber, kind: Kind) -> Profile {
    Profile {
        number,
        name: format!("account {number}"),
        kind,
    }
}

/// chat, forge and activity (a hook) running, the view `lens`; identity
/// profiling the accounts above; code published at `TIP + 1`.
fn seated() -> (MockHost, BlobId) {
    let store = MockHost::default();
    crate::rules::init(
        &store.exec(env(0, Origin::Root)),
        Genesis {
            programs: ["chat", "forge", "activity"]
                .map(|program| entry(program, BlobId::Sha256([1; 32])))
                .into(),
            views: vec![View {
                name: "lens".into(),
                view: BlobId::Sha256([2; 32]),
            }],
            hooks: vec!["activity".into()],
        },
    );
    store.identity(vec![
        profile(CAROL, Kind::Person),
        profile(DAN, Kind::Person),
        profile(EVE, Kind::Person),
        profile(FORGE, Kind::Module("forge".into())),
        profile(
            AGENT,
            Kind::Managed {
                manager: CAROL,
                category: Category::Agent,
                standing: Standing::Active,
            },
        ),
    ]);
    // at the frames' height, so the fold every op starts with has run
    // and a refusal below writes nothing
    Modules::execute(
        &store.exec(env(TIP + 1, Origin::Signed(vec![9]))),
        Op::Publish {
            body: b"wasm".to_vec(),
        },
    )
    .unwrap();
    let code: BlobId = abi::decode(&store.take_output()).unwrap();
    (store, code)
}

/// A frame signed by `account`'s key, made at `TIP + 1`.
fn by(account: AccountNumber) -> Env {
    Env {
        height: TIP + 1,
        ..MockHost::env(crate::MODULE)
    }
    .signed([account as u8; 32], Some(account))
}

fn vote() -> Env {
    authority(TIP + 1)
}

/// `op` in `env`; a refusal checked to leave the host as it was.
fn run(store: &MockHost, env: Env, op: Op) -> Result<(), Error> {
    store.attempt(|| Modules::execute(&store.exec(env), op))
}

/// The code a refused `op` met; the refusal checked to leave the host as it was.
fn refused(store: &MockHost, env: Env, op: Op) -> String {
    store
        .refused(|| Modules::execute(&store.exec(env), op))
        .code
}

fn at(height: u64, change: Change) -> Op {
    Op::Schedule(Scheduled { height, change })
}

fn set(program: &str, code: BlobId) -> Change {
    Change::Set(entry(program, code))
}

fn cancel_at(height: u64, program: &str) -> Op {
    Op::Cancel {
        height,
        program: program.into(),
    }
}

fn grant(scope: Scope, account: AccountNumber) -> Grant {
    Grant { scope, account }
}

fn module(id: &str) -> Scope {
    Scope::Module(id.into())
}

/// The vote grants `scope` to `account`.
fn granted(store: &MockHost, scope: Scope, account: AccountNumber) {
    run(store, vote(), Op::Grant(grant(scope, account))).unwrap();
}

/// What waits: each change's height, program and who made it.
fn waiting(store: &MockHost) -> Vec<(u64, String, Option<AccountNumber>)> {
    let asked = Query::Scheduled {
        page: PageRequest::default(),
    };
    match Modules::query(&store.query(env(TIP + 1, Origin::Root)), asked).unwrap() {
        Reply::Scheduled(page) => page
            .items
            .into_iter()
            .map(|Pending { height, change, by }| (height, change.program().to_owned(), by))
            .collect(),
        other => panic!("{other:?}"),
    }
}

fn grants(store: &MockHost, page: PageRequest) -> store::PageResponse<Grant> {
    match Modules::query(
        &store.query(env(TIP + 1, Origin::Root)),
        Query::Grants { page },
    )
    .unwrap()
    {
        Reply::Grants(page) => page,
        other => panic!("{other:?}"),
    }
}

fn all_grants(store: &MockHost) -> Vec<Grant> {
    grants(store, PageRequest::default()).items
}

/// The code `program` runs at `height`, as the kernel asks it.
fn code_at(store: &MockHost, height: u64, program: &str) -> Option<BlobId> {
    match Modules::query(&store.query(env(height, Origin::Root)), Query::At(height)).unwrap() {
        Reply::Programs(entries) => entries
            .into_iter()
            .find(|entry| entry.program == program)
            .map(|entry| entry.code),
        other => panic!("{other:?}"),
    }
}

/// An op at `height`, which folds every change due by then.
fn fold_to(store: &MockHost, height: u64) {
    Modules::execute(
        &store.exec(env(height, Origin::Signed(vec![9]))),
        Op::Publish { body: vec![1] },
    )
    .unwrap();
}

#[test]
fn a_maintainers_change_lands_no_sooner_than_notice_blocks_after_it_is_made() {
    let (store, code) = seated();
    granted(&store, module("chat"), CAROL);
    assert_eq!(
        refused(&store, by(CAROL), at(EARLIEST - 1, set("chat", code))),
        code::INVALID_INPUT,
        "one block short of the notice"
    );
    run(&store, by(CAROL), at(EARLIEST, set("chat", code))).unwrap();
    assert_eq!(waiting(&store), [(EARLIEST, "chat".into(), Some(CAROL))]);
    assert_ne!(code_at(&store, EARLIEST - 1, "chat"), Some(code));
    assert_eq!(code_at(&store, EARLIEST, "chat"), Some(code));
    fold_to(&store, EARLIEST);
    assert_eq!(code_at(&store, EARLIEST, "chat"), Some(code));
    assert!(waiting(&store).is_empty());
    assert!(
        !BY.has(
            &store.query(env(EARLIEST, Origin::Root)),
            &(EARLIEST, "chat".into())
        ),
        "fold clears who made it"
    );
}

#[test]
fn a_maintainer_changes_only_what_its_grant_covers_and_never_a_role_or_a_hook() {
    let (store, code) = seated();
    granted(&store, module("chat"), CAROL);
    granted(&store, Scope::All, EVE);
    let cases: Vec<(&str, Env, Change)> = vec![
        ("forge by chat's maintainer", by(CAROL), set("forge", code)),
        ("a role, with every module", by(EVE), set("valset", code)),
        ("the registry itself", by(EVE), set("module-registry", code)),
        ("set a hook", by(EVE), Change::SetHook("chat".into())),
        (
            "remove a hook",
            by(EVE),
            Change::RemoveHook("activity".into()),
        ),
        (
            "remove a hooked program",
            by(EVE),
            Change::Remove("activity".into()),
        ),
        (
            "no account",
            by(EVE).signed([9; 32], None),
            set("chat", code),
        ),
        (
            "a module's message, its account holding every module",
            by(EVE).from_module("forge", Some(EVE)),
            set("chat", code),
        ),
        (
            "a chain frame",
            Env {
                sender: Some(Principal::Root),
                ..by(EVE)
            }
            .root(),
            set("chat", code),
        ),
        ("no grant", by(DAN), set("chat", code)),
    ];
    for (what, env, change) in cases {
        assert_eq!(
            refused(&store, env, at(EARLIEST, change)),
            code::UNAUTHORIZED,
            "{what}"
        );
    }
    // a hooked program's code changes by grant; only its removal is the vote's
    run(&store, by(EVE), at(EARLIEST, set("activity", code))).unwrap();
    run(
        &store,
        by(EVE),
        at(EARLIEST, Change::Remove("forge".into())),
    )
    .unwrap();
    run(&store, by(CAROL), at(EARLIEST + 1, set("chat", code))).unwrap();
}

#[test]
fn a_grant_may_name_a_module_that_runs_nothing_yet() {
    let (store, code) = seated();
    granted(&store, module("trade"), CAROL);
    run(&store, by(CAROL), at(EARLIEST, set("trade", code))).unwrap();
    fold_to(&store, EARLIEST);
    assert_eq!(code_at(&store, EARLIEST, "trade"), Some(code));
}

#[test]
fn a_change_the_vote_left_waiting_holds_the_name_against_maintainers() {
    let (store, code) = seated();
    granted(&store, module("chat"), CAROL);
    run(&store, vote(), at(5_000, set("chat", code))).unwrap();
    assert_eq!(
        refused(&store, by(CAROL), at(EARLIEST, set("chat", code))),
        code::WRONG_STATE
    );
    run(&store, vote(), cancel_at(5_000, "chat")).unwrap();
    run(&store, by(CAROL), at(EARLIEST, set("chat", code))).unwrap();
}

#[test]
fn the_votes_removal_takes_the_names_grants_and_their_changes_along() {
    let (store, code) = seated();
    granted(&store, module("chat"), CAROL);
    granted(&store, module("chat"), DAN);
    granted(&store, module("forge"), DAN);
    granted(&store, Scope::All, EVE);
    run(&store, by(CAROL), at(EARLIEST, set("chat", code))).unwrap();
    run(&store, by(DAN), at(EARLIEST, set("forge", code))).unwrap();
    // the vote's own changes on chat, made after the grant's
    run(&store, vote(), at(EARLIEST + 1, set("chat", code))).unwrap();
    run(
        &store,
        vote(),
        at(EARLIEST + 2, Change::Remove("chat".into())),
    )
    .unwrap();
    assert_eq!(
        waiting(&store),
        [
            (EARLIEST, "forge".into(), Some(DAN)),
            (EARLIEST + 1, "chat".into(), None),
            (EARLIEST + 2, "chat".into(), None),
        ]
    );
    assert_eq!(
        all_grants(&store),
        [grant(module("forge"), DAN), grant(Scope::All, EVE)]
    );
    // every module's maintainer may set the name again once it is gone
    fold_to(&store, EARLIEST + 2);
    assert_eq!(code_at(&store, EARLIEST + 2, "chat"), None);
    let later = Env {
        height: EARLIEST + 3,
        ..by(EVE)
    };
    run(&store, later, at(EARLIEST + 3 + NOTICE, set("chat", code))).unwrap();
}

#[test]
fn a_maintainer_cancels_a_grants_change_and_only_the_vote_cancels_the_votes() {
    let (store, code) = seated();
    granted(&store, module("chat"), CAROL);
    granted(&store, module("chat"), DAN);
    granted(&store, module("forge"), EVE);
    run(&store, by(CAROL), at(EARLIEST, set("chat", code))).unwrap();
    run(&store, by(DAN), at(EARLIEST + 1, set("chat", code))).unwrap();
    run(&store, by(DAN), at(EARLIEST + 2, set("chat", code))).unwrap();
    assert_eq!(
        refused(&store, by(EVE), cancel_at(EARLIEST, "chat")),
        code::UNAUTHORIZED,
        "forge's maintainer"
    );
    run(&store, by(CAROL), cancel_at(EARLIEST, "chat")).unwrap();
    run(&store, by(CAROL), cancel_at(EARLIEST + 1, "chat")).unwrap();
    run(&store, vote(), cancel_at(EARLIEST + 2, "chat")).unwrap();
    let read = store.query(env(TIP + 1, Origin::Root));
    for height in EARLIEST..EARLIEST + 3 {
        assert!(
            !BY.has(&read, &(height, "chat".into())),
            "cancel clears {height}"
        );
    }
    run(&store, vote(), at(EARLIEST, set("chat", code))).unwrap();
    assert_eq!(
        refused(&store, by(CAROL), cancel_at(EARLIEST, "chat")),
        code::UNAUTHORIZED,
        "the vote's change"
    );
    run(&store, vote(), cancel_at(EARLIEST, "chat")).unwrap();
    assert!(waiting(&store).is_empty());
}

#[test]
fn the_votes_change_clears_a_stale_maker() {
    let (store, code) = seated();
    // a registry rolled back past a grant's change leaves its maker behind
    let stale = |height: u64| {
        let ctx = store.exec(env(TIP, Origin::Root));
        BY.put(&ctx, &(height, "chat".into()), &CAROL);
    };
    stale(EARLIEST);
    run(&store, vote(), at(EARLIEST, set("chat", code))).unwrap();
    assert_eq!(waiting(&store), [(EARLIEST, "chat".into(), None)]);
    // a removal's cascade takes grants' changes along, never the vote's own
    stale(EARLIEST + 1);
    run(
        &store,
        vote(),
        at(EARLIEST + 1, Change::Remove("chat".into())),
    )
    .unwrap();
    assert_eq!(
        waiting(&store),
        [
            (EARLIEST, "chat".into(), None),
            (EARLIEST + 1, "chat".into(), None)
        ]
    );
}

#[test]
fn an_account_keeps_at_most_max_pending_changes_waiting() {
    let (store, code) = seated();
    granted(&store, Scope::All, CAROL);
    granted(&store, Scope::All, DAN);
    run(&store, vote(), at(EARLIEST, set("forge", code))).unwrap();
    let heights = EARLIEST..EARLIEST + MAX_PENDING as u64;
    for height in heights.clone() {
        run(&store, by(CAROL), at(height, set("chat", code))).unwrap();
    }
    assert_eq!(
        refused(&store, by(CAROL), at(EARLIEST + 99, set("lens2", code))),
        code::CAPACITY
    );
    // another maintainer's count is its own
    run(&store, by(DAN), at(EARLIEST + 99, set("chat", code))).unwrap();
}

#[test]
fn only_the_vote_grants_and_only_to_a_person_or_an_agent_over_a_name_with_no_role() {
    let (store, _) = seated();
    for env in [by(CAROL), by(EVE).from_module("forge", Some(EVE))] {
        assert_eq!(
            refused(&store, env, Op::Grant(grant(Scope::All, CAROL))),
            code::UNAUTHORIZED
        );
    }
    for (what, scope, account) in [
        ("an id in another spelling", module("Chat"), CAROL),
        ("a role", module("identity"), CAROL),
        ("no such account", Scope::All, 99),
        ("a program's account", module("chat"), FORGE),
    ] {
        assert_eq!(
            refused(&store, vote(), Op::Grant(grant(scope, account))),
            code::INVALID_INPUT,
            "{what}"
        );
    }
    granted(&store, module("chat"), AGENT);
    granted(&store, module("chat"), CAROL);
    granted(&store, module("chat"), CAROL);
    assert_eq!(
        all_grants(&store),
        [grant(module("chat"), CAROL), grant(module("chat"), AGENT)],
        "a grant held already is no second one"
    );
}

#[test]
fn the_vote_or_its_holder_revokes_a_grant_and_one_not_held_is_no_error() {
    let (store, _) = seated();
    granted(&store, module("chat"), CAROL);
    granted(&store, module("chat"), DAN);
    for env in [by(DAN), by(CAROL).from_module("forge", Some(CAROL))] {
        assert_eq!(
            refused(&store, env, Op::Revoke(grant(module("chat"), CAROL))),
            code::UNAUTHORIZED
        );
    }
    run(&store, by(CAROL), Op::Revoke(grant(module("chat"), CAROL))).unwrap();
    run(&store, by(CAROL), Op::Revoke(grant(module("chat"), CAROL))).unwrap();
    run(&store, vote(), Op::Revoke(grant(module("chat"), CAROL))).unwrap();
    run(&store, vote(), Op::Revoke(grant(module("chat"), DAN))).unwrap();
    assert!(all_grants(&store).is_empty());
}

#[test]
fn a_revoked_account_keeps_only_the_changes_another_of_its_grants_covers() {
    let (store, code) = seated();
    granted(&store, Scope::All, EVE);
    granted(&store, module("chat"), EVE);
    granted(&store, module("forge"), DAN);
    run(&store, by(EVE), at(EARLIEST, set("chat", code))).unwrap();
    run(&store, by(EVE), at(EARLIEST, set("forge", code))).unwrap();
    run(&store, by(DAN), at(EARLIEST + 1, set("forge", code))).unwrap();
    run(&store, vote(), Op::Revoke(grant(Scope::All, EVE))).unwrap();
    assert_eq!(
        waiting(&store),
        [
            (EARLIEST, "chat".into(), Some(EVE)),
            (EARLIEST + 1, "forge".into(), Some(DAN)),
        ]
    );
    // stepping down stops what it scheduled
    run(&store, by(EVE), Op::Revoke(grant(module("chat"), EVE))).unwrap();
    assert_eq!(waiting(&store), [(EARLIEST + 1, "forge".into(), Some(DAN))]);
}

#[test]
fn a_revoke_drops_a_removal_and_the_view_that_reclaims_its_name_together() {
    let (store, code) = seated();
    granted(&store, Scope::All, CAROL);
    run(
        &store,
        by(CAROL),
        at(EARLIEST, Change::Remove("chat".into())),
    )
    .unwrap();
    let view = Change::SetView(View {
        name: "chat".into(),
        view: code,
    });
    run(&store, by(CAROL), at(EARLIEST + 1, view)).unwrap();
    // one at a time, the removal's cancel would be refused
    assert_eq!(
        refused(&store, vote(), cancel_at(EARLIEST, "chat")),
        code::ALREADY_EXISTS
    );
    run(&store, vote(), Op::Revoke(grant(Scope::All, CAROL))).unwrap();
    assert!(waiting(&store).is_empty());
    assert!(code_at(&store, EARLIEST + 1, "chat").is_some());
}

#[test]
fn the_grants_page_by_scope_then_account() {
    let (store, _) = seated();
    granted(&store, Scope::All, CAROL);
    granted(&store, module("forge"), DAN);
    granted(&store, module("chat"), EVE);
    let first = grants(&store, PageRequest::first(2));
    assert_eq!(
        first.items,
        [grant(module("chat"), EVE), grant(module("forge"), DAN)]
    );
    let rest = grants(&store, PageRequest::resume(first.next, 2));
    assert_eq!(rest.items, [grant(Scope::All, CAROL)]);
    assert_eq!(rest.next, None);
}

#[test]
fn a_grant_and_a_revoke_are_described_in_words() {
    use describe::{Value, field};
    let described = crate::describe(&Op::Grant(grant(module("trade"), CAROL)));
    assert_eq!(described.title, "Grant · trade");
    assert_eq!(
        described.fields,
        [
            field("account", Value::Account(CAROL)),
            field("scope", Value::Module("trade".into())),
        ]
    );
    let described = crate::describe(&Op::Revoke(grant(Scope::All, EVE)));
    assert_eq!(described.title, "Revoke · every module");
    assert_eq!(
        described.fields[1],
        field("scope", Value::text("every module"))
    );
}
