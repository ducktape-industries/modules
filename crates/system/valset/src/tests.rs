// The module natively over `guest::MockHost`: what the founding suite checks on the host, without the host.

use guest::{Env, Error, MockHost, Module, Origin, code};
use store::PageRequest;

use crate::{
    Genesis, Grant, INVITE_NAMESPACE, Invite, Member, Membership, Motion, Op, Query, Reply, Role,
    Valset, Voted,
};

const NETWORK: &[u8] = b"net";
const TIME: u64 = 1_000;

fn key(n: u8) -> Vec<u8> {
    vec![n; 32]
}

fn env(origin: Origin) -> Env {
    Env {
        chain_id: NETWORK.to_vec(),
        height: 3,
        time: TIME,
        origin,
        sender: None,
        ..MockHost::env(crate::MODULE)
    }
}

fn member(n: u8) -> Member {
    Member {
        key: key(n),
        address: Some(format!("node-{n}")),
    }
}

fn membership(n: u8, role: Role) -> Membership {
    Membership {
        key: key(n),
        address: Some(format!("node-{n}")),
        role,
    }
}

fn found(validators: &[u8], member_cap: u32) -> Result<MockHost, Error> {
    let store = MockHost::default();
    store.borrow_mut().verifier = Some(Box::new(|_, key, namespace, message, signature| {
        namespace == INVITE_NAMESPACE && signature == [key, message].concat()
    }));
    let genesis = abi::encode(&Genesis {
        validators: validators.iter().rev().map(|n| member(*n)).collect(),
        member_cap,
    });
    Valset::init(&store.exec(env(Origin::Root)), &genesis)?;
    Ok(store)
}

fn founded() -> MockHost {
    found(&[1, 2], 8).unwrap()
}

fn run(store: &MockHost, signer: u8, op: Op) -> Result<(), Error> {
    store.attempt(|| Valset::execute(&store.exec(env(Origin::Signed(key(signer)))), op))
}

fn refused(store: &MockHost, signer: u8, op: Op) -> String {
    store
        .refused(|| Valset::execute(&store.exec(env(Origin::Signed(key(signer)))), op))
        .code
}

fn enroll(n: u8, invite: Option<Invite>) -> Op {
    Op::Enroll {
        address: Some(format!("node-{n}")),
        invite,
    }
}

fn vote(store: &MockHost, voter: u8, motion: Motion) -> Result<Voted, Error> {
    run(store, voter, Op::Vote(motion))?;
    Ok(abi::decode(&store.take_output()).unwrap())
}

fn pass(store: &MockHost, voters: &[u8], motion: Motion) {
    let (last, first) = voters.split_last().unwrap();
    for voter in first {
        let counted = vote(store, *voter, motion.clone()).unwrap();
        assert!(matches!(counted, Voted::Counted { .. }), "{counted:?}");
    }
    assert_eq!(vote(store, *last, motion).unwrap(), Voted::Enacted);
}

fn signed(issuer: u8, grant: Grant) -> Invite {
    let signature = [key(issuer), grant.preimage()].concat();
    Invite {
        issuer: key(issuer),
        grant,
        signature,
    }
}

fn invite(issuer: u8, nonce: u8) -> Invite {
    signed(
        issuer,
        Grant {
            network: NETWORK.to_vec(),
            nonce: [nonce; 16],
            expires: TIME + 1,
        },
    )
}

fn ask(store: &MockHost, query: Query) -> Reply {
    Valset::query(&store.query(env(Origin::Root)), query).unwrap()
}

fn held(store: &MockHost, n: u8) -> Option<Membership> {
    match ask(store, Query::Membership { key: key(n) }) {
        Reply::Membership(membership) => membership,
        other => panic!("{other:?}"),
    }
}

fn validators(store: &MockHost) -> Vec<Vec<u8>> {
    match ask(store, Query::Validators) {
        Reply::Validators(validators) => validators,
        other => panic!("{other:?}"),
    }
}

#[test]
fn founding_seats_the_validators_in_key_order() {
    let store = founded();
    assert_eq!(validators(&store), [key(1), key(2)]);
    let Reply::Members(members) = ask(&store, Query::Members) else {
        panic!()
    };
    assert_eq!(members[0], member(1));
}

#[test]
fn founding_more_validators_than_the_cap_is_refused() {
    let refused = found(&[1, 2], 1).err();
    assert_eq!(refused.map(|error| error.code), Some(code::CAPACITY.into()));
}

#[test]
fn only_a_signed_frame_writes() {
    let store = founded();
    for origin in [Origin::Root, Origin::Module("probe".into())] {
        let refusal = store.refused(|| Valset::execute(&store.exec(env(origin)), enroll(3, None)));
        assert_eq!(refusal.code, code::UNAUTHORIZED);
    }
}

#[test]
fn the_closed_door_refuses_an_uninvited_stranger() {
    let store = founded();
    assert_eq!(refused(&store, 3, enroll(3, None)), code::UNAUTHORIZED);
    assert_eq!(held(&store, 3), None);
}

#[test]
fn an_invite_admits_a_resident_once() {
    let store = founded();
    let ticket = invite(1, 7);
    run(&store, 3, enroll(3, Some(ticket.clone()))).unwrap();
    assert_eq!(held(&store, 3), Some(membership(3, Role::Resident)));
    assert_eq!(validators(&store), [key(1), key(2)]);
    assert_eq!(
        refused(&store, 4, enroll(4, Some(ticket))),
        code::WRONG_STATE
    );
    run(&store, 4, enroll(4, Some(invite(2, 7)))).unwrap();
}

#[test]
fn an_expired_foreign_forged_or_resident_issued_invite_is_refused() {
    let store = founded();
    let expired = signed(
        1,
        Grant {
            expires: TIME,
            ..invite(1, 1).grant
        },
    );
    assert_eq!(
        refused(&store, 3, enroll(3, Some(expired))),
        code::INVALID_INPUT
    );
    let foreign = signed(
        1,
        Grant {
            network: b"elsewhere".to_vec(),
            ..invite(1, 2).grant
        },
    );
    assert_eq!(
        refused(&store, 3, enroll(3, Some(foreign))),
        code::INVALID_INPUT
    );
    let forged = Invite {
        signature: invite(9, 3).signature,
        ..invite(1, 3)
    };
    assert_eq!(
        refused(&store, 3, enroll(3, Some(forged))),
        code::UNAUTHORIZED
    );
    run(&store, 3, enroll(3, Some(invite(1, 4)))).unwrap();
    assert_eq!(
        refused(&store, 4, enroll(4, Some(invite(3, 5)))),
        code::UNAUTHORIZED
    );
    let stranger = invite(9, 6);
    assert_eq!(
        refused(&store, 4, enroll(4, Some(stranger))),
        code::UNAUTHORIZED
    );
}

#[test]
fn an_open_door_admits_without_an_invite() {
    let store = founded();
    pass(&store, &[1, 2], Motion::Door { open: true });
    run(&store, 3, enroll(3, None)).unwrap();
    assert_eq!(held(&store, 3), Some(membership(3, Role::Resident)));
    let again = vote(&store, 1, Motion::Door { open: true }).unwrap_err();
    assert_eq!(again.code, code::WRONG_STATE);
    pass(&store, &[1, 2], Motion::Door { open: false });
    assert_eq!(refused(&store, 4, enroll(4, None)), code::UNAUTHORIZED);
}

#[test]
fn a_member_enrolling_again_moves_its_address_and_keeps_its_role() {
    let store = founded();
    let moved = Op::Enroll {
        address: Some("elsewhere".into()),
        invite: None,
    };
    run(&store, 1, moved).unwrap();
    assert_eq!(
        held(&store, 1),
        Some(Membership {
            address: Some("elsewhere".into()),
            ..membership(1, Role::Validator)
        })
    );
}

#[test]
fn a_member_enrolling_without_an_address_is_held_without_one() {
    let store = founded();
    let undialed = Op::Enroll {
        address: None,
        invite: None,
    };
    run(&store, 1, undialed).unwrap();
    assert_eq!(
        held(&store, 1),
        Some(Membership {
            address: None,
            ..membership(1, Role::Validator)
        })
    );
}

#[test]
fn a_full_network_refuses_a_newcomer_and_a_key_is_32_bytes() {
    let store = found(&[1, 2], 3).unwrap();
    run(&store, 3, enroll(3, Some(invite(1, 1)))).unwrap();
    assert_eq!(
        refused(&store, 4, enroll(4, Some(invite(1, 2)))),
        code::CAPACITY
    );
    run(&store, 3, enroll(3, None)).unwrap();
    let short = store.refused(|| {
        Valset::execute(
            &store.exec(env(Origin::Signed(vec![5, 5]))),
            enroll(5, Some(invite(1, 3))),
        )
    });
    assert_eq!(short.code, code::INVALID_INPUT);
}

#[test]
fn only_a_validator_votes() {
    let store = founded();
    run(&store, 3, enroll(3, Some(invite(1, 1)))).unwrap();
    let promote = Motion::Promote { key: key(3) };
    assert_eq!(
        refused(&store, 3, Op::Vote(promote.clone())),
        code::UNAUTHORIZED
    );
    assert_eq!(refused(&store, 9, Op::Vote(promote)), code::UNAUTHORIZED);
}

#[test]
fn a_motion_passes_at_the_quorum_of_the_current_validators() {
    let store = found(&[1, 2, 3, 4], 8).unwrap();
    run(&store, 5, enroll(5, Some(invite(1, 1)))).unwrap();
    let promote = Motion::Promote { key: key(5) };
    let counted = |votes| Voted::Counted { votes, needed: 3 };
    assert_eq!(vote(&store, 1, promote.clone()).unwrap(), counted(1));
    assert_eq!(vote(&store, 1, promote.clone()).unwrap(), counted(1));
    assert_eq!(vote(&store, 2, promote.clone()).unwrap(), counted(2));
    assert_eq!(held(&store, 5).unwrap().role, Role::Resident);
    assert_eq!(vote(&store, 3, promote.clone()).unwrap(), Voted::Enacted);
    assert_eq!(held(&store, 5).unwrap().role, Role::Validator);
    let again = vote(&store, 4, promote).unwrap_err();
    assert_eq!(again.code, code::WRONG_STATE);
}

#[test]
fn a_voter_no_longer_a_validator_is_dropped_at_tally() {
    let store = found(&[1, 2, 3, 4], 8).unwrap();
    assert_eq!(
        vote(&store, 4, Motion::Remove { key: key(1) }).unwrap(),
        Voted::Counted {
            votes: 1,
            needed: 3
        }
    );
    pass(&store, &[1, 2, 3], Motion::Remove { key: key(4) });
    assert_eq!(
        vote(&store, 2, Motion::Remove { key: key(1) }).unwrap(),
        Voted::Counted {
            votes: 1,
            needed: 3
        }
    );
}

#[test]
fn a_removal_clears_the_open_motions_that_name_the_member() {
    let store = found(&[1, 2, 3, 4], 8).unwrap();
    run(&store, 5, enroll(5, Some(invite(1, 1)))).unwrap();
    vote(&store, 1, Motion::Promote { key: key(5) }).unwrap();
    vote(&store, 2, Motion::Promote { key: key(5) }).unwrap();
    pass(&store, &[1, 2, 3], Motion::Remove { key: key(5) });
    assert_eq!(held(&store, 5), None);
    run(&store, 5, enroll(5, Some(invite(1, 2)))).unwrap();
    assert_eq!(
        vote(&store, 3, Motion::Promote { key: key(5) }).unwrap(),
        Voted::Counted {
            votes: 1,
            needed: 3
        }
    );
}

#[test]
fn a_motion_on_the_wrong_role_or_a_stranger_is_refused() {
    let store = founded();
    let promote_a_validator = Op::Vote(Motion::Promote { key: key(1) });
    assert_eq!(refused(&store, 1, promote_a_validator), code::WRONG_STATE);
    run(&store, 3, enroll(3, Some(invite(1, 1)))).unwrap();
    let demote_a_resident = Op::Vote(Motion::Demote { key: key(3) });
    assert_eq!(refused(&store, 1, demote_a_resident), code::WRONG_STATE);
    let remove_a_stranger = Op::Vote(Motion::Remove { key: key(8) });
    assert_eq!(refused(&store, 1, remove_a_stranger), code::NOT_FOUND);
}

#[test]
fn the_last_validator_is_held() {
    let store = founded();
    pass(&store, &[1, 2], Motion::Demote { key: key(2) });
    assert_eq!(validators(&store), [key(1)]);
    assert_eq!(held(&store, 2).unwrap().role, Role::Resident);
    let demote = Op::Vote(Motion::Demote { key: key(1) });
    assert_eq!(refused(&store, 1, demote), code::WRONG_STATE);
    let remove = Op::Vote(Motion::Remove { key: key(1) });
    assert_eq!(refused(&store, 1, remove), code::WRONG_STATE);
    assert_eq!(refused(&store, 1, Op::Leave), code::WRONG_STATE);
    assert_eq!(validators(&store), [key(1)]);
}

#[test]
fn a_member_leaves_and_a_stranger_cannot() {
    let store = founded();
    run(&store, 3, enroll(3, Some(invite(1, 1)))).unwrap();
    run(&store, 3, Op::Leave).unwrap();
    assert_eq!(held(&store, 3), None);
    run(&store, 2, Op::Leave).unwrap();
    assert_eq!(validators(&store), [key(1)]);
    assert_eq!(refused(&store, 4, Op::Leave), code::NOT_FOUND);
}

#[test]
fn a_registry_motion_emits_its_op_to_the_registry_once_enacted() {
    let store = founded();
    let op = b"a registry op".to_vec();
    let motion = Motion::Registry { op: op.clone() };
    vote(&store, 1, motion.clone()).unwrap();
    assert!(store.take_emissions().is_empty());
    assert_eq!(vote(&store, 2, motion).unwrap(), Voted::Enacted);
    let sent = store.take_emissions();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].target, MockHost::roles().registry);
    assert_eq!(sent[0].payload, op);
    assert!(!sent[0].reply);
}

#[test]
fn memberships_page_in_key_order_at_the_answering_height() {
    let store = founded();
    run(&store, 3, enroll(3, Some(invite(1, 1)))).unwrap();
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
    assert_eq!(
        crate::role(&store.query(bound), &key(1)).unwrap(),
        Some(Role::Validator)
    );
}
