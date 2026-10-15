use program::{Cause, Principal, code};

use super::*;

#[test]
fn an_entry_runs_at_the_end_of_its_block_as_the_program_that_scheduled_it() {
    let chain = chain(true);
    plan(&chain, "planner", vec![note(3, "ran")]);
    let [id] = scheduled(&chain, "planner")[..] else {
        panic!("one entry was scheduled");
    };
    chain.at(3, 30);
    assert_eq!(noted(&chain, "ran"), None);
    assert_eq!(entry(&chain, id).unwrap().receipt, None);
    chain.at(4, 40);
    let env = noted(&chain, "ran").expect("the run noted its env");
    assert_eq!(env.origin, Origin::Program("planner".into()));
    assert_eq!(env.sender, Some(Principal::Account(PLANNER)));
    assert_eq!(env.cause, Cause::Scheduled { entry: id });
    assert_eq!(env.height, 3);
    let receipt = entry(&chain, id)
        .unwrap()
        .receipt
        .expect("the scheduler kept the receipt");
    assert_eq!(receipt.program, "mark");
    assert!(matches!(receipt.outcome, abi::Outcome::Applied { .. }));
}

#[test]
fn a_cancelled_entry_never_runs() {
    let chain = chain(true);
    plan(&chain, "planner", vec![note(3, "ran")]);
    let [id] = scheduled(&chain, "planner")[..] else {
        panic!("one entry was scheduled");
    };
    plan(&chain, "other", vec![Plan::Cancel(id)]);
    let [Err(refused)] = &answers(&chain, "other")[..] else {
        panic!("another program cannot cancel the entry");
    };
    assert_eq!(refused.code, code::UNAUTHORIZED);
    plan(&chain, "planner", vec![Plan::Cancel(id)]);
    assert_eq!(answers(&chain, "planner"), vec![Ok(None)]);
    chain.at(5, 0);
    assert_eq!(noted(&chain, "ran"), None);
    assert_eq!(entry(&chain, id), None);
}

#[test]
fn entries_due_at_one_height_run_in_the_order_they_were_made() {
    let chain = chain(true);
    let count = |key: &str| Plan::Schedule {
        height: 2,
        op: Mark::Count(key.into()),
    };
    plan(&chain, "planner", vec![count("a"), count("b"), count("c")]);
    chain.at(3, 0);
    let order: Vec<Option<u64>> = ["a", "b", "c"]
        .iter()
        .map(|key| kept(&chain, key))
        .collect();
    assert_eq!(order, [Some(0), Some(1), Some(2)]);
}

#[test]
fn a_refused_run_undoes_only_its_own_writes_and_its_receipt_records_the_refusal() {
    let chain = chain(true);
    let refuse = Plan::Schedule {
        height: 2,
        op: Mark::Refuse("refused".into()),
    };
    plan(&chain, "planner", vec![refuse, note(2, "applied")]);
    let [refused, applied] = scheduled(&chain, "planner")[..] else {
        panic!("two entries were scheduled");
    };
    chain.at(3, 0);
    assert_eq!(noted(&chain, "refused"), None);
    assert!(noted(&chain, "applied").is_some());
    let receipt = entry(&chain, refused).unwrap().receipt.expect("ran");
    let abi::Outcome::Rejected(refusal) = receipt.outcome else {
        panic!("the refusing run was rejected");
    };
    assert_eq!(refusal.reason, code::INVALID_INPUT);
    let receipt = entry(&chain, applied).unwrap().receipt.expect("ran");
    assert!(matches!(receipt.outcome, abi::Outcome::Applied { .. }));
}

#[test]
fn a_scheduled_run_can_schedule_a_later_entry() {
    let chain = chain(true);
    let again = Plan::Schedule {
        height: 2,
        op: Mark::Again {
            height: 4,
            key: "later".into(),
        },
    };
    plan(&chain, "planner", vec![again]);
    chain.at(3, 0);
    let later = entry(&chain, 1).expect("the run scheduled one");
    assert_eq!((later.scheduler.as_str(), later.height), ("mark", 4));
    chain.at(4, 0);
    assert_eq!(noted(&chain, "later"), None);
    chain.at(5, 0);
    let env = noted(&chain, "later").expect("the later entry ran");
    assert_eq!(env.origin, Origin::Program("mark".into()));
    assert_eq!(env.cause, Cause::Scheduled { entry: 1 });
}

#[test]
fn an_entry_an_earlier_run_of_its_block_cancels_does_not_run() {
    let chain = chain(true);
    let pair = Plan::Schedule {
        height: 2,
        op: Mark::Pair {
            height: 3,
            key: "cancelled".into(),
        },
    };
    plan(&chain, "planner", vec![pair]);
    chain.at(3, 0);
    let cancelled = entry(&chain, 2).expect("the second entry waits");
    assert_eq!(
        (cancelled.scheduler.as_str(), cancelled.height),
        ("mark", 3)
    );
    chain.at(4, 0);
    let cancelling = entry(&chain, 1).unwrap().receipt.expect("ran");
    assert!(matches!(cancelling.outcome, abi::Outcome::Applied { .. }));
    assert_eq!(entry(&chain, 2), None);
    assert_eq!(noted(&chain, "cancelled"), None);
}

#[test]
fn with_no_scheduler_row_bound_nothing_runs() {
    let chain = chain(false);
    let direct = abi::role::scheduler::Op::Schedule {
        height: 2,
        target: "mark".into(),
        payload: abi::encode(&Mark::Note("ran".into())),
    };
    chain
        .submit(Origin::Program("planner".into()), PROGRAM, &direct)
        .unwrap();
    chain.at(4, 0);
    assert_eq!(noted(&chain, "ran"), None);
    assert_eq!(entry(&chain, 0).unwrap().receipt, None);
    plan(&chain, "planner", vec![note(5, "ran")]);
    let [Err(refused)] = &answers(&chain, "planner")[..] else {
        panic!("ctx.schedule needs the scheduler role");
    };
    assert_eq!(refused.code, code::UNSUPPORTED);
}
