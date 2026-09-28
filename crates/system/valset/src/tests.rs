// The module natively over `guest::MockHost`: what the founding suite checks on the host, without the host.

use guest::{Env, MockHost, Module, Origin, code};
use store::PageRequest;

use crate::{Genesis, Member, Membership, Op, Query, Reply, Role, Valset};

fn key(n: u8) -> Vec<u8> {
    vec![n; 32]
}

fn env(origin: Origin) -> Env {
    Env {
        height: 3,
        origin,
        // these rules read the origin alone
        sender: None,
        ..MockHost::env(crate::MODULE)
    }
}

fn member(n: u8, address: &str) -> Member {
    Member {
        key: key(n),
        address: address.into(),
    }
}

fn found(validators: Vec<Member>, member_cap: u32) -> Result<MockHost, guest::Error> {
    let store = MockHost::default();
    let genesis = abi::encode(&Genesis {
        validators,
        member_cap,
    });
    Valset::init(&store.exec(env(Origin::Root)), &genesis)?;
    Ok(store)
}

/// Validators 2 and 1, under a cap of three members.
fn founded() -> MockHost {
    found(vec![member(2, "b"), member(1, "a")], 3).unwrap()
}

fn govern(store: &MockHost, op: Op) -> Result<(), guest::Error> {
    Valset::execute(&store.exec(env(Origin::Signed(key(9)))), op)
}

fn ask(store: &MockHost, query: Query) -> Reply {
    Valset::query(&store.query(env(Origin::Root)), query).unwrap()
}

fn membership(n: u8, role: Role) -> Membership {
    Membership {
        key: key(n),
        address: format!("node-{n}"),
        role,
    }
}

#[test]
fn founding_seats_the_validators_in_key_order() {
    let store = founded();
    assert_eq!(
        ask(&store, Query::Validators),
        Reply::Validators(vec![key(1), key(2)])
    );
    let Reply::Members(members) = ask(&store, Query::Members) else {
        panic!()
    };
    assert_eq!(members[0].address, "a");
}

#[test]
fn anyone_writes_and_a_key_is_32_bytes() {
    let store = founded();
    let short = govern(
        &store,
        Op::Set(Membership {
            key: vec![1, 2],
            address: "x".into(),
            role: Role::Resident,
        }),
    );
    assert_eq!(short.unwrap_err().code, code::INVALID_INPUT);
    govern(&store, Op::Set(membership(3, Role::Resident))).unwrap();
    assert_eq!(
        ask(&store, Query::Membership { key: key(3) }),
        Reply::Membership(Some(membership(3, Role::Resident)))
    );
    assert_eq!(
        ask(&store, Query::Validators),
        Reply::Validators(vec![key(1), key(2)])
    );
}

#[test]
fn the_last_validator_stays_seated() {
    let store = founded();
    govern(&store, Op::Remove { key: key(1) }).unwrap();
    let demote = govern(&store, Op::Set(membership(2, Role::Resident)));
    assert_eq!(demote.unwrap_err().code, code::WRONG_STATE);
    let remove = govern(&store, Op::Remove { key: key(2) });
    assert_eq!(remove.unwrap_err().code, code::WRONG_STATE);
    govern(&store, Op::Set(membership(5, Role::Validator))).unwrap();
    govern(&store, Op::Remove { key: key(2) }).unwrap();
    assert_eq!(
        ask(&store, Query::Validators),
        Reply::Validators(vec![key(5)])
    );
}

#[test]
fn memberships_page_in_key_order_at_the_answering_height() {
    let store = founded();
    govern(&store, Op::Set(membership(3, Role::Resident))).unwrap();
    let Reply::Memberships(first) = ask(
        &store,
        Query::Memberships {
            page: PageRequest::first(2),
        },
    ) else {
        panic!()
    };
    assert_eq!(first.height, 3);
    assert_eq!(
        first.items.iter().map(|m| m.key[0]).collect::<Vec<_>>(),
        [1, 2]
    );
    let Reply::Memberships(rest) = ask(
        &store,
        Query::Memberships {
            page: PageRequest::resume(first.next, 2),
        },
    ) else {
        panic!()
    };
    assert_eq!(rest.items, [membership(3, Role::Resident)]);
    assert_eq!(rest.next, None);
}

#[test]
fn role_asks_the_program_bound_to_the_validators_role() {
    let store = founded();
    store.sibling::<Valset>("third-party-valset", &store);
    let mut bound = env(Origin::Root);
    bound.roles = guest::Roles {
        validators: "third-party-valset".into(),
        ..MockHost::roles()
    };
    // nothing answers at the literal name: only the binding reaches valset
    assert_eq!(
        crate::role(&store.query(bound), &key(1)).unwrap(),
        Some(Role::Validator)
    );
}

fn members(store: &MockHost) -> Vec<Member> {
    let Reply::Members(members) = ask(store, Query::Members) else {
        panic!()
    };
    members
}

#[test]
fn a_newcomer_past_the_cap_is_refused() {
    let store = founded();
    govern(&store, Op::Set(membership(3, Role::Resident))).unwrap();
    let full = govern(&store, Op::Set(membership(4, Role::Resident)));
    assert_eq!(full.unwrap_err().code, code::CAPACITY);
    assert_eq!(members(&store).len(), 3);
}

#[test]
fn a_member_at_the_cap_changes_role_and_address() {
    let store = founded();
    govern(&store, Op::Set(membership(3, Role::Resident))).unwrap();
    govern(&store, Op::Set(membership(1, Role::Resident))).unwrap();
    let moved = Membership {
        address: "elsewhere".into(),
        ..membership(3, Role::Validator)
    };
    govern(&store, Op::Set(moved.clone())).unwrap();
    assert_eq!(
        ask(&store, Query::Membership { key: key(3) }),
        Reply::Membership(Some(moved))
    );
}

#[test]
fn a_removal_frees_a_place() {
    let store = founded();
    govern(&store, Op::Set(membership(3, Role::Resident))).unwrap();
    govern(&store, Op::Remove { key: key(3) }).unwrap();
    govern(&store, Op::Set(membership(4, Role::Resident))).unwrap();
}

#[test]
fn founding_more_validators_than_the_cap_is_refused() {
    let refused = found(vec![member(1, "a"), member(2, "b")], 1).err();
    assert_eq!(refused.map(|error| error.code), Some(code::CAPACITY.into()));
}
