use abi::Receipt;
use program::code;

use super::*;
use crate::Op;

fn by(program: &str) -> Origin {
    Origin::Program(program.into())
}

fn schedule(height: u64) -> Op {
    Op::Schedule {
        height,
        target: "mark".into(),
        payload: abi::encode(&Mark::Note("k".into())),
    }
}

fn receipt() -> Receipt {
    Receipt {
        program: "mark".into(),
        outcome: abi::Outcome::Applied { output: Vec::new() },
        events: Vec::new(),
        nested: Vec::new(),
    }
}

fn due(
    chain: &MockChain,
    height: u64,
    after: Option<EntryId>,
    limit: Option<u32>,
) -> (Vec<EntryId>, Option<EntryId>) {
    let asked = Query::Due {
        height,
        after,
        limit,
    };
    match chain.query(PROGRAM, &asked).unwrap() {
        Reply::Due { entries, next } => (entries.iter().map(|entry| entry.id).collect(), next),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_program_schedules_as_itself_and_gets_the_entry_id() {
    let chain = chain(true);
    let first = chain.submit(by("planner"), PROGRAM, &schedule(3)).unwrap();
    let second = chain.submit(by("other"), PROGRAM, &schedule(3)).unwrap();
    assert_eq!(abi::decode::<EntryId>(&first).unwrap(), 0);
    assert_eq!(abi::decode::<EntryId>(&second).unwrap(), 1);
    let kept = entry(&chain, 1).expect("the entry is kept");
    assert_eq!(
        (
            kept.scheduler.as_str(),
            kept.height,
            kept.target.as_str(),
            kept.receipt
        ),
        ("other", 3, "mark", None)
    );
}

#[test]
fn only_a_program_schedules_and_only_for_a_later_height() {
    let chain = chain(true);
    let signed = chain.submit(Origin::Signed(KEY.to_vec()), PROGRAM, &schedule(3));
    assert_eq!(signed.unwrap_err().code, code::UNAUTHORIZED);
    let root = chain.submit(Origin::Root, PROGRAM, &schedule(3));
    assert_eq!(root.unwrap_err().code, code::UNAUTHORIZED);
    for height in [0, 1] {
        let refused = chain.submit(by("planner"), PROGRAM, &schedule(height));
        assert_eq!(refused.unwrap_err().code, code::INVALID_INPUT, "{height}");
    }
    assert_eq!(entry(&chain, 0), None);
}

#[test]
fn only_the_scheduling_program_cancels_and_only_before_the_run() {
    let chain = chain(true);
    chain.submit(by("planner"), PROGRAM, &schedule(3)).unwrap();
    chain.submit(by("planner"), PROGRAM, &schedule(3)).unwrap();
    let cancel = |entry| Op::Cancel { entry };
    let theirs = chain.submit(by("other"), PROGRAM, &cancel(0));
    assert_eq!(theirs.unwrap_err().code, code::UNAUTHORIZED);
    let signed = chain.submit(Origin::Signed(KEY.to_vec()), PROGRAM, &cancel(0));
    assert_eq!(signed.unwrap_err().code, code::UNAUTHORIZED);
    chain.submit(by("planner"), PROGRAM, &cancel(0)).unwrap();
    assert_eq!(entry(&chain, 0), None);
    assert_eq!(due(&chain, 3, None, None).0, [1]);
    let gone = chain.submit(by("planner"), PROGRAM, &cancel(0));
    assert_eq!(gone.unwrap_err().code, code::NOT_FOUND);
    let ran = Op::Ran {
        entry: 1,
        receipt: receipt(),
    };
    chain.submit(Origin::Root, PROGRAM, &ran).unwrap();
    let late = chain.submit(by("planner"), PROGRAM, &cancel(1));
    assert_eq!(late.unwrap_err().code, code::WRONG_STATE);
}

#[test]
fn only_the_kernel_says_an_entry_ran_and_the_entry_keeps_the_receipt() {
    let chain = chain(true);
    chain.submit(by("planner"), PROGRAM, &schedule(3)).unwrap();
    let ran = Op::Ran {
        entry: 0,
        receipt: receipt(),
    };
    let from_planner = chain.submit(by("planner"), PROGRAM, &ran);
    assert_eq!(from_planner.unwrap_err().code, code::UNAUTHORIZED);
    chain.submit(Origin::Root, PROGRAM, &ran).unwrap();
    assert_eq!(entry(&chain, 0).unwrap().receipt, Some(receipt()));
    assert_eq!(due(&chain, 3, None, None), (Vec::new(), None));
    let again = chain.submit(Origin::Root, PROGRAM, &ran);
    assert_eq!(again.unwrap_err().code, code::WRONG_STATE);
    let missing = Op::Ran {
        entry: 9,
        receipt: receipt(),
    };
    let unknown = chain.submit(Origin::Root, PROGRAM, &missing);
    assert_eq!(unknown.unwrap_err().code, code::NOT_FOUND);
}

#[test]
fn due_pages_one_height_in_the_order_entries_were_made_and_no_limit_is_every_row() {
    let chain = chain(true);
    for height in [5, 4, 5, 5] {
        chain
            .submit(by("planner"), PROGRAM, &schedule(height))
            .unwrap();
    }
    assert_eq!(due(&chain, 5, None, None), (vec![0, 2, 3], None));
    assert_eq!(due(&chain, 4, None, None), (vec![1], None));
    assert_eq!(due(&chain, 5, None, Some(2)), (vec![0, 2], Some(2)));
    assert_eq!(due(&chain, 5, Some(2), Some(2)), (vec![3], None));
    assert_eq!(due(&chain, 6, None, None), (Vec::new(), None));
}
