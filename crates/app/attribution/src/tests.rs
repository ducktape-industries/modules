//! attribution on a `MockChain`: a program reporting its own objects, and subscribers told
//! of every change.
use ::program::{Error, ExecCtx, MockChain, Origin, Program, QueryCtx, Reply as Wanted, code};
use borsh::{BorshDeserialize, BorshSerialize};
use store::Item;

use crate::*;

const ADA: &[u8] = b"ada-key";
const REPORTER: &str = "reporter";
const LISTENER: &str = "listener";

#[derive(Debug, BorshSerialize, BorshDeserialize)]
enum Report {
    Send(Op),
}

struct Reporter;

impl Program for Reporter {
    const NAME: &'static str = REPORTER;
    type Op = Report;
    type Query = ();
    type Reply = ();

    fn execute(ctx: &ExecCtx, op: Report) -> Result<(), Error> {
        let Report::Send(op) = op;
        ctx.emit(PROGRAM, &op, Wanted::None);
        Ok(())
    }

    fn query(_: &QueryCtx, (): ()) -> Result<(), Error> {
        Ok(())
    }
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
enum Listen {
    Subscribe,
    Told(Box<Change>),
    Refuse,
}

const TOLD: Item<Vec<Change>> = Item::new("told");
const REFUSING: Item<bool> = Item::new("refusing");

struct Listener;

impl Program for Listener {
    const NAME: &'static str = LISTENER;
    type Op = Listen;
    type Query = ();
    type Reply = Vec<Change>;

    fn execute(ctx: &ExecCtx, op: Listen) -> Result<(), Error> {
        match op {
            Listen::Subscribe => {
                let deliver = vec![1];
                ctx.emit(PROGRAM, &Op::Subscribe { deliver }, Wanted::None);
                Ok(())
            }
            Listen::Told(change) => {
                ctx.env().sent_by(PROGRAM)?;
                if REFUSING.get(ctx)?.unwrap_or(false) {
                    return Err(::program::invalid("not now"));
                }
                let mut told = TOLD.get(ctx)?.unwrap_or_default();
                told.push(*change);
                TOLD.put(ctx, &told);
                Ok(())
            }
            Listen::Refuse => {
                REFUSING.put(ctx, &true);
                Ok(())
            }
        }
    }

    fn query(ctx: &QueryCtx, (): ()) -> Result<Vec<Change>, Error> {
        Ok(TOLD.get(ctx)?.unwrap_or_default())
    }
}

fn chain() -> MockChain {
    let mut chain = MockChain::default();
    chain.seat::<Attribution>(PROGRAM);
    chain.seat::<Reporter>(REPORTER);
    chain.seat::<Listener>(LISTENER);
    for (program, number) in [(PROGRAM, 900), (REPORTER, 902), (LISTENER, 903)] {
        chain.register(program, number);
    }
    chain.hold(ADA.to_vec(), 1);
    chain
}

fn signed(key: &[u8]) -> Origin {
    Origin::Signed(key.to_vec())
}

fn source(id: &str) -> Source {
    Source {
        program: REPORTER.into(),
        object: Object {
            kind: "task".into(),
            id: id.into(),
        },
    }
}

fn relations(chain: &MockChain, source: Source) -> Option<Relations> {
    let Reply::Relations(relations) = chain.query(PROGRAM, &Query::Relations { source }).unwrap()
    else {
        panic!()
    };
    relations
}

fn changes_to(chain: &MockChain, recipient: AccountNumber) -> Vec<(String, Reason, Kind)> {
    let asked = Query::ChangesTo {
        recipient,
        page: PageRequest::default(),
    };
    let Reply::Changes(page) = chain.query(PROGRAM, &asked).unwrap() else {
        panic!()
    };
    page.items
        .into_iter()
        .map(|change| (change.source.object.id, change.reason, change.kind))
        .collect()
}

fn update(id: &str, revision: u64, relations: Vec<Relation>, transfers: Vec<Transfer>) -> Op {
    Op::Attribute(Update {
        object: Object {
            kind: "task".into(),
            id: id.into(),
        },
        revision,
        actor: Actor::System,
        relations,
        transfers,
    })
}

fn owner(recipient: AccountNumber) -> Relation {
    Relation {
        recipient,
        reason: Reason::Ownership,
        detail: b"since today".to_vec(),
    }
}

fn report(chain: &MockChain, op: Op) -> Result<Vec<u8>, Error> {
    chain.submit(signed(ADA), REPORTER, &Report::Send(op))
}

#[test]
fn a_program_reports_only_its_own_objects_and_only_forward() {
    let chain = chain();
    let signed_by_hand = chain.submit(
        signed(ADA),
        PROGRAM,
        &update("t", 1, Vec::new(), Vec::new()),
    );
    assert_eq!(signed_by_hand.unwrap_err().code, code::UNAUTHORIZED);

    report(&chain, update("t", 5, vec![owner(1)], Vec::new())).unwrap();
    assert_eq!(
        relations(&chain, source("t")).unwrap().relations,
        [owner(1)]
    );
    let again = report(&chain, update("t", 5, Vec::new(), Vec::new()));
    assert_eq!(again.unwrap_err().code, code::STALE);

    let unmatched = Transfer {
        reason: Reason::Ownership,
        from: 1,
        to: 3,
    };
    let refused = report(&chain, update("t", 6, vec![owner(2)], vec![unmatched]));
    assert_eq!(refused.unwrap_err().code, code::INVALID_INPUT);
    let handed = Transfer {
        reason: Reason::Ownership,
        from: 1,
        to: 2,
    };
    report(&chain, update("t", 6, vec![owner(2)], vec![handed])).unwrap();
    assert_eq!(
        changes_to(&chain, 1),
        [
            ("t".into(), Reason::Ownership, Kind::Added),
            (
                "t".into(),
                Reason::Ownership,
                Kind::TransferredOut { to: 2 }
            )
        ]
    );
    assert_eq!(
        changes_to(&chain, 2),
        [(
            "t".into(),
            Reason::Ownership,
            Kind::TransferredIn { from: 1 }
        )]
    );

    for bad in [
        update("a/b", 1, Vec::new(), Vec::new()),
        update("u", 1, vec![owner(0)], Vec::new()),
        update("u", 1, vec![owner(1), owner(1)], Vec::new()),
        update(
            "u",
            1,
            vec![Relation {
                recipient: 1,
                reason: Reason::Defined(String::new()),
                detail: Vec::new(),
            }],
            Vec::new(),
        ),
    ] {
        assert_eq!(report(&chain, bad).unwrap_err().code, code::INVALID_INPUT);
    }
}

fn told(chain: &MockChain) -> Vec<Change> {
    chain.query(LISTENER, &()).unwrap()
}

#[test]
fn every_subscriber_is_told_each_change_and_its_refusal_changes_nothing() {
    let chain = chain();
    chain
        .submit(signed(ADA), LISTENER, &Listen::Subscribe)
        .unwrap();
    let Reply::Subscribers(subscribers) = chain.query(PROGRAM, &Query::Subscribers).unwrap() else {
        panic!()
    };
    assert_eq!(subscribers, [LISTENER.to_owned()]);
    let direct_subscribe = chain.submit(signed(ADA), PROGRAM, &Op::Subscribe { deliver: vec![1] });
    assert_eq!(direct_subscribe.unwrap_err().code, code::UNAUTHORIZED);

    report(&chain, update("t", 1, vec![owner(1), owner(2)], Vec::new())).unwrap();
    let seen: Vec<(AccountNumber, Kind)> = told(&chain)
        .into_iter()
        .map(|change| (change.recipient, change.kind))
        .collect();
    assert_eq!(seen, [(1, Kind::Added), (2, Kind::Added)]);
    let forged = Listen::Told(Box::new(told(&chain)[0].clone()));
    assert_eq!(
        chain
            .submit(signed(ADA), LISTENER, &forged)
            .unwrap_err()
            .code,
        code::UNAUTHORIZED
    );

    chain
        .submit(signed(ADA), LISTENER, &Listen::Refuse)
        .unwrap();
    report(&chain, update("t", 2, vec![owner(2)], Vec::new())).unwrap();
    assert_eq!(told(&chain).len(), 2);
    assert_eq!(
        relations(&chain, source("t")).unwrap().relations,
        [owner(2)]
    );
}
