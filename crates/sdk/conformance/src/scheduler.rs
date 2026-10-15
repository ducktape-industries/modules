//! The scheduler role: a program schedules a run for a later height as itself, only it cancels
//! the entry and only before the run, only the kernel says it ran, and `Due` pages a height's
//! entries in the order they were made. [`run`] checks every rule below.

use abi::Receipt;
use abi::role::scheduler::{Entry, EntryId, Op, Query, Reply};
use program::{Env, Error, MockHost, Origin, Principal, Program, code};

use crate::{ask, env, execute, same_bytes};

pub trait Fixture {
    type Program: Program;

    fn host(&self) -> MockHost;
}

pub fn run(fixture: &impl Fixture) {
    the_role_is_the_first_variants(fixture);
    only_a_program_schedules_and_only_for_a_later_height(fixture);
    a_schedule_answers_its_entry_id_and_keeps_the_entry(fixture);
    only_its_scheduler_cancels_an_entry_and_only_before_it_runs(fixture);
    only_the_kernel_says_an_entry_ran_and_the_entry_keeps_the_receipt(fixture);
    due_pages_a_heights_entries_in_the_order_they_were_made(fixture);
}

const SCHEDULER: &str = "caller";
const OTHER: &str = "other";
const TARGET: &str = "target";
const HEIGHT: u64 = 10;

fn program() -> String {
    MockHost::role(abi::role::SCHEDULER)
}

fn from(origin: Origin) -> Env {
    let sender = match origin {
        Origin::Root => Principal::Root,
        _ => Principal::Account(7),
    };
    env(&program(), HEIGHT, origin, Some(sender))
}

fn by(sender: &str) -> Origin {
    Origin::Program(sender.into())
}

fn signed() -> Origin {
    Origin::Signed(b"anyone".to_vec())
}

fn schedule_op(height: u64) -> Op {
    Op::Schedule {
        height,
        target: TARGET.into(),
        payload: vec![height as u8],
    }
}

fn receipt() -> Receipt {
    Receipt {
        program: TARGET.into(),
        outcome: abi::Outcome::Applied { output: vec![1] },
        events: Vec::new(),
        nested: Vec::new(),
    }
}

fn send<F: Fixture>(host: &MockHost, origin: Origin, op: &Op) -> Result<(), Error> {
    host.attempt(|| execute::<F::Program>(host, from(origin), op))
}

fn schedule<F: Fixture>(host: &MockHost, scheduler: &str, height: u64) -> EntryId {
    host.take_output();
    send::<F>(host, by(scheduler), &schedule_op(height))
        .unwrap_or_else(|e| panic!("scheduler: {scheduler} schedules at {height}: {e:?}"));
    abi::decode(&host.take_output())
        .unwrap_or_else(|e| panic!("scheduler: Schedule answers its entry id: {e}"))
}

fn entry<F: Fixture>(host: &MockHost, id: EntryId) -> Option<Entry> {
    match ask::<F::Program, Reply>(host, &program(), HEIGHT, &Query::Entry { id }) {
        Ok(Reply::Entry(entry)) => entry,
        other => panic!("scheduler: Entry({id}) answers Entry, not {other:?}"),
    }
}

fn due<F: Fixture>(
    host: &MockHost,
    height: u64,
    after: Option<EntryId>,
    limit: Option<u32>,
) -> (Vec<EntryId>, Option<EntryId>) {
    let query = Query::Due {
        height,
        after,
        limit,
    };
    match ask::<F::Program, Reply>(host, &program(), HEIGHT, &query) {
        Ok(Reply::Due { entries, next }) => (entries.iter().map(|entry| entry.id).collect(), next),
        other => panic!("scheduler: {query:?} answers Due, not {other:?}"),
    }
}

#[track_caller]
fn refused_as(result: Result<(), Error>, code: &str, what: &str) {
    assert_eq!(
        result.map_err(|e| e.code),
        Err(code.into()),
        "scheduler: {what} is refused {code}"
    );
}

pub fn the_role_is_the_first_variants<F: Fixture>(_: &F) {
    type M<F> = <F as Fixture>::Program;
    let m = program();
    for op in [
        schedule_op(HEIGHT + 1),
        Op::Cancel { entry: 3 },
        Op::Ran {
            entry: 3,
            receipt: receipt(),
        },
    ] {
        same_bytes::<<M<F> as Program>::Op>(&m, "Op", &op);
    }
    for query in [
        Query::Due {
            height: HEIGHT,
            after: Some(2),
            limit: Some(5),
        },
        Query::Entry { id: 3 },
    ] {
        same_bytes::<<M<F> as Program>::Query>(&m, "Query", &query);
    }
    let kept = Entry {
        id: 3,
        scheduler: SCHEDULER.into(),
        height: HEIGHT,
        target: TARGET.into(),
        payload: vec![1],
        receipt: Some(receipt()),
    };
    for reply in [
        Reply::Due {
            entries: vec![kept.clone()],
            next: Some(3),
        },
        Reply::Entry(Some(kept)),
        Reply::Entry(None),
    ] {
        same_bytes::<<M<F> as Program>::Reply>(&m, "Reply", &reply);
    }
}

pub fn only_a_program_schedules_and_only_for_a_later_height<F: Fixture>(fixture: &F) {
    let host = fixture.host();
    for origin in [signed(), Origin::Root] {
        let scheduled = send::<F>(&host, origin.clone(), &schedule_op(HEIGHT + 1));
        refused_as(
            scheduled,
            code::UNAUTHORIZED,
            &format!("Schedule from {origin:?}"),
        );
    }
    for height in [HEIGHT - 1, HEIGHT] {
        let scheduled = send::<F>(&host, by(SCHEDULER), &schedule_op(height));
        refused_as(
            scheduled,
            code::INVALID_INPUT,
            &format!("Schedule at height {height} from height {HEIGHT}"),
        );
    }
    for height in [HEIGHT - 1, HEIGHT, HEIGHT + 1] {
        assert_eq!(
            due::<F>(&host, height, None, None),
            (Vec::new(), None),
            "scheduler: a refused Schedule keeps nothing"
        );
    }
}

pub fn a_schedule_answers_its_entry_id_and_keeps_the_entry<F: Fixture>(fixture: &F) {
    let host = fixture.host();
    let first = schedule::<F>(&host, SCHEDULER, HEIGHT + 2);
    let second = schedule::<F>(&host, OTHER, HEIGHT + 2);
    assert_ne!(first, second, "scheduler: every entry has its own id");
    let kept = Entry {
        id: first,
        scheduler: SCHEDULER.into(),
        height: HEIGHT + 2,
        target: TARGET.into(),
        payload: vec![(HEIGHT + 2) as u8],
        receipt: None,
    };
    assert_eq!(
        entry::<F>(&host, first),
        Some(kept),
        "scheduler: the entry names the program that scheduled it, as the kernel runs it"
    );
}

pub fn only_its_scheduler_cancels_an_entry_and_only_before_it_runs<F: Fixture>(fixture: &F) {
    let host = fixture.host();
    let cancelled = schedule::<F>(&host, SCHEDULER, HEIGHT + 1);
    let ran = schedule::<F>(&host, SCHEDULER, HEIGHT + 1);
    let cancel = |entry| Op::Cancel { entry };
    for origin in [by(OTHER), signed(), Origin::Root] {
        let refused = send::<F>(&host, origin.clone(), &cancel(cancelled));
        refused_as(
            refused,
            code::UNAUTHORIZED,
            &format!("Cancel from {origin:?}"),
        );
    }
    send::<F>(&host, by(SCHEDULER), &cancel(cancelled))
        .unwrap_or_else(|e| panic!("scheduler: its scheduler cancels an entry: {e:?}"));
    assert_eq!(
        entry::<F>(&host, cancelled),
        None,
        "scheduler: a cancelled entry is gone"
    );
    assert_eq!(
        due::<F>(&host, HEIGHT + 1, None, None),
        (vec![ran], None),
        "scheduler: a cancelled entry is due no longer"
    );
    let ran_op = Op::Ran {
        entry: ran,
        receipt: receipt(),
    };
    send::<F>(&host, Origin::Root, &ran_op).unwrap();
    let late = send::<F>(&host, by(SCHEDULER), &cancel(ran));
    refused_as(late, code::WRONG_STATE, "Cancel of an entry that ran");
}

pub fn only_the_kernel_says_an_entry_ran_and_the_entry_keeps_the_receipt<F: Fixture>(fixture: &F) {
    let host = fixture.host();
    let id = schedule::<F>(&host, SCHEDULER, HEIGHT + 1);
    let ran = Op::Ran {
        entry: id,
        receipt: receipt(),
    };
    for origin in [by(SCHEDULER), signed()] {
        let refused = send::<F>(&host, origin.clone(), &ran);
        refused_as(refused, code::UNAUTHORIZED, &format!("Ran from {origin:?}"));
    }
    send::<F>(&host, Origin::Root, &ran)
        .unwrap_or_else(|e| panic!("scheduler: the kernel says an entry ran: {e:?}"));
    assert_eq!(
        entry::<F>(&host, id).and_then(|entry| entry.receipt),
        Some(receipt()),
        "scheduler: the entry keeps the run's receipt"
    );
    assert_eq!(
        due::<F>(&host, HEIGHT + 1, None, None),
        (Vec::new(), None),
        "scheduler: an entry that ran is due no longer"
    );
    assert!(
        send::<F>(&host, Origin::Root, &ran).is_err(),
        "scheduler: an entry runs once"
    );
}

pub fn due_pages_a_heights_entries_in_the_order_they_were_made<F: Fixture>(fixture: &F) {
    let host = fixture.host();
    let first = schedule::<F>(&host, SCHEDULER, HEIGHT + 2);
    let elsewhere = schedule::<F>(&host, SCHEDULER, HEIGHT + 1);
    let second = schedule::<F>(&host, OTHER, HEIGHT + 2);
    let third = schedule::<F>(&host, SCHEDULER, HEIGHT + 2);
    assert_eq!(
        due::<F>(&host, HEIGHT + 2, None, None),
        (vec![first, second, third], None),
        "scheduler: Due with no limit is every entry due at the height, in the order made"
    );
    assert_eq!(
        due::<F>(&host, HEIGHT + 1, None, None),
        (vec![elsewhere], None),
        "scheduler: Due is the height's alone"
    );
    let (page, next) = due::<F>(&host, HEIGHT + 2, None, Some(2));
    assert_eq!(page, [first, second], "scheduler: Due pages by limit");
    let Some(next) = next else {
        panic!("scheduler: a full page names where the next starts");
    };
    let (rest, _) = due::<F>(&host, HEIGHT + 2, Some(next), Some(2));
    assert_eq!(rest, [third], "scheduler: Due resumes after an entry");
}
