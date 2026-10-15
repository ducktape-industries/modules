//! The awaits role: a ticket for each call a parked frame waits on, and the answer each ticket
//! hands back to its frame through the host's resume. [`run`] checks every rule below.

use abi::Refusal;
use abi::role::awaits::{
    Answer, Call, EXPIRY_BLOCKS, FrameId, Op, Query, Reply, Ticket, TicketRow, expired,
};
use program::kernel::error_from;
use program::{Env, Error, MockHost, Origin, Principal, Program, Resumed, code};

use crate::{ask, env, execute, same_bytes};

pub trait Fixture {
    type Program: Program;

    fn host(&self) -> MockHost;
}

pub fn run(fixture: &impl Fixture) {
    the_role_is_the_first_variants(fixture);
    park_and_close_are_the_kernels_alone(fixture);
    a_park_opens_a_ticket_for_each_call(fixture);
    only_the_called_program_resolves_its_ticket(fixture);
    a_resolve_hands_the_answer_to_the_frame(fixture);
    a_resolve_of_a_ticket_no_longer_open_does_nothing(fixture);
    a_ticket_expires_at_its_deadline_and_not_before(fixture);
    a_close_drops_the_frames_tickets_alone(fixture);
    owed_pages_a_targets_open_tickets(fixture);
    overdue_pages_what_may_expire(fixture);
}

const CALLER: &str = "caller";
const ASKED: &str = "oracle";
const OTHER: &str = "other";
const HEIGHT: u64 = 10;

fn program() -> String {
    MockHost::role(abi::role::AWAITS)
}

fn from(origin: Origin, height: u64) -> Env {
    let sender = match origin {
        Origin::Root => Principal::Root,
        _ => Principal::Account(7),
    };
    env(&program(), height, origin, Some(sender))
}

fn by(sender: &str) -> Origin {
    Origin::Program(sender.into())
}

fn signed() -> Origin {
    Origin::Signed(b"anyone".to_vec())
}

fn park_op(frame: FrameId, calls: &[(Ticket, &str)]) -> Op {
    let calls = calls
        .iter()
        .map(|(ticket, target)| Call {
            ticket: *ticket,
            target: (*target).into(),
        })
        .collect();
    Op::Park {
        frame,
        program: CALLER.into(),
        calls,
    }
}

fn send<F: Fixture>(host: &MockHost, origin: Origin, height: u64, op: &Op) -> Result<(), Error> {
    host.attempt(|| execute::<F::Program>(host, from(origin, height), op))
}

fn park<F: Fixture>(host: &MockHost, height: u64, frame: FrameId, calls: &[(Ticket, &str)]) {
    send::<F>(host, Origin::Root, height, &park_op(frame, calls))
        .unwrap_or_else(|e| panic!("awaits: the kernel parks frame {frame}: {e:?}"));
}

fn resolve<F: Fixture>(
    host: &MockHost,
    origin: Origin,
    ticket: Ticket,
    answer: Answer,
) -> Result<(), Error> {
    send::<F>(host, origin, HEIGHT, &Op::Resolve { ticket, answer })
}

fn row<F: Fixture>(host: &MockHost, ticket: Ticket) -> Option<TicketRow> {
    match ask::<F::Program, Reply>(host, &program(), HEIGHT, &Query::Ticket { ticket }) {
        Ok(Reply::Ticket(row)) => row,
        other => panic!("awaits: Ticket({ticket}) answers Ticket, not {other:?}"),
    }
}

fn listed<F: Fixture>(host: &MockHost, query: Query) -> Vec<Ticket> {
    match ask::<F::Program, Reply>(host, &program(), HEIGHT, &query) {
        Ok(Reply::Tickets(rows)) => rows.into_iter().map(|row| row.ticket).collect(),
        other => panic!("awaits: {query:?} answers Tickets, not {other:?}"),
    }
}

fn owed<F: Fixture>(
    host: &MockHost,
    target: &str,
    after: Option<Ticket>,
    limit: u32,
) -> Vec<Ticket> {
    let target = target.into();
    listed::<F>(
        host,
        Query::Owed {
            target,
            after,
            limit,
        },
    )
}

fn overdue<F: Fixture>(
    host: &MockHost,
    height: u64,
    after: Option<Ticket>,
    limit: u32,
) -> Vec<Ticket> {
    listed::<F>(
        host,
        Query::Overdue {
            height,
            after,
            limit,
        },
    )
}

fn resumed(frame: FrameId, ticket: Ticket, answer: Answer) -> Resumed {
    Resumed {
        frame,
        ticket,
        answer: answer.map_err(error_from),
    }
}

#[track_caller]
fn refused_as(result: Result<(), Error>, code: &str, what: &str) {
    assert_eq!(
        result.map_err(|e| e.code),
        Err(code.into()),
        "awaits: {what} is refused {code}"
    );
}

pub fn the_role_is_the_first_variants<F: Fixture>(_: &F) {
    type M<F> = <F as Fixture>::Program;
    let m = program();
    for op in [
        park_op(4, &[(1, ASKED), (2, OTHER)]),
        Op::Resolve {
            ticket: 1,
            answer: Ok(b"42".to_vec()),
        },
        Op::Resolve {
            ticket: 2,
            answer: Err(Refusal::new(code::NOT_FOUND, "no such pair")),
        },
        Op::Expire { ticket: 1 },
        Op::Close { frame: 4 },
    ] {
        same_bytes::<<M<F> as Program>::Op>(&m, "Op", &op);
    }
    for query in [
        Query::Ticket { ticket: 1 },
        Query::Owed {
            target: ASKED.into(),
            after: Some(1),
            limit: 5,
        },
        Query::Overdue {
            height: 80,
            after: None,
            limit: 5,
        },
    ] {
        same_bytes::<<M<F> as Program>::Query>(&m, "Query", &query);
    }
    let row = TicketRow {
        ticket: 1,
        frame: 4,
        caller: CALLER.into(),
        target: ASKED.into(),
        opened: HEIGHT,
        deadline: HEIGHT + EXPIRY_BLOCKS,
    };
    for reply in [
        Reply::Ticket(Some(row.clone())),
        Reply::Ticket(None),
        Reply::Tickets(vec![row]),
    ] {
        same_bytes::<<M<F> as Program>::Reply>(&m, "Reply", &reply);
    }
}

pub fn park_and_close_are_the_kernels_alone<F: Fixture>(fixture: &F) {
    let host = fixture.host();
    for origin in [signed(), by(CALLER)] {
        let parked = send::<F>(&host, origin.clone(), HEIGHT, &park_op(4, &[(1, ASKED)]));
        refused_as(parked, code::UNAUTHORIZED, &format!("Park from {origin:?}"));
    }
    assert_eq!(
        row::<F>(&host, 1),
        None,
        "awaits: a refused Park opens nothing"
    );
    park::<F>(&host, HEIGHT, 4, &[(1, ASKED)]);
    for origin in [signed(), by(CALLER)] {
        let closed = send::<F>(&host, origin.clone(), HEIGHT, &Op::Close { frame: 4 });
        refused_as(
            closed,
            code::UNAUTHORIZED,
            &format!("Close from {origin:?}"),
        );
    }
    assert!(
        row::<F>(&host, 1).is_some(),
        "awaits: a refused Close keeps the frame's tickets"
    );
}

pub fn a_park_opens_a_ticket_for_each_call<F: Fixture>(fixture: &F) {
    let host = fixture.host();
    park::<F>(&host, HEIGHT, 4, &[(1, ASKED), (2, OTHER)]);
    for (ticket, target) in [(1, ASKED), (2, OTHER)] {
        let opened = TicketRow {
            ticket,
            frame: 4,
            caller: CALLER.into(),
            target: target.into(),
            opened: HEIGHT,
            deadline: HEIGHT + EXPIRY_BLOCKS,
        };
        assert_eq!(
            row::<F>(&host, ticket),
            Some(opened),
            "awaits: Park opens ticket {ticket} at the parking height, due EXPIRY_BLOCKS later"
        );
    }
    assert_eq!(
        row::<F>(&host, 3),
        None,
        "awaits: Park opens no other ticket"
    );
    assert_eq!(host.take_resumes(), [], "awaits: Park resumes nothing");
}

pub fn only_the_called_program_resolves_its_ticket<F: Fixture>(fixture: &F) {
    let host = fixture.host();
    park::<F>(&host, HEIGHT, 4, &[(1, ASKED)]);
    for origin in [by(OTHER), by(CALLER), signed()] {
        let answered = resolve::<F>(&host, origin.clone(), 1, Ok(b"no".to_vec()));
        refused_as(
            answered,
            code::UNAUTHORIZED,
            &format!("Resolve from {origin:?}"),
        );
    }
    assert!(
        row::<F>(&host, 1).is_some(),
        "awaits: a refused Resolve keeps the ticket open"
    );
}

pub fn a_resolve_hands_the_answer_to_the_frame<F: Fixture>(fixture: &F) {
    let host = fixture.host();
    park::<F>(&host, HEIGHT, 4, &[(1, ASKED), (2, ASKED)]);
    resolve::<F>(&host, by(ASKED), 1, Ok(b"42".to_vec()))
        .unwrap_or_else(|e| panic!("awaits: the called program resolves its ticket: {e:?}"));
    assert_eq!(
        host.take_resumes(),
        [resumed(4, 1, Ok(b"42".to_vec()))],
        "awaits: Resolve resumes the frame with the answer"
    );
    assert_eq!(
        row::<F>(&host, 1),
        None,
        "awaits: Resolve closes the ticket"
    );
    assert!(
        row::<F>(&host, 2).is_some(),
        "awaits: Resolve leaves the frame's other tickets open"
    );
    let refusal = Refusal::new(code::NOT_FOUND, "no such pair");
    resolve::<F>(&host, by(ASKED), 2, Err(refusal.clone()))
        .unwrap_or_else(|e| panic!("awaits: a refusal is an answer too: {e:?}"));
    assert_eq!(
        host.take_resumes(),
        [resumed(4, 2, Err(refusal))],
        "awaits: Resolve hands a refusal back as the call's refusal"
    );
}

pub fn a_resolve_of_a_ticket_no_longer_open_does_nothing<F: Fixture>(fixture: &F) {
    let host = fixture.host();
    resolve::<F>(&host, by(ASKED), 7, Ok(vec![])).unwrap_or_else(|e| {
        panic!("awaits: a Resolve of a ticket never opened is no refusal: {e:?}")
    });
    park::<F>(&host, HEIGHT, 4, &[(1, ASKED)]);
    resolve::<F>(&host, by(ASKED), 1, Ok(vec![])).unwrap();
    host.take_resumes();
    resolve::<F>(&host, by(ASKED), 1, Ok(vec![]))
        .unwrap_or_else(|e| panic!("awaits: a second Resolve is no refusal: {e:?}"));
    assert_eq!(
        host.take_resumes(),
        [],
        "awaits: a Resolve of a ticket no longer open resumes nothing"
    );
}

pub fn a_ticket_expires_at_its_deadline_and_not_before<F: Fixture>(fixture: &F) {
    let host = fixture.host();
    park::<F>(&host, HEIGHT, 4, &[(1, ASKED)]);
    let deadline = HEIGHT + EXPIRY_BLOCKS;
    let early = send::<F>(&host, signed(), deadline - 1, &Op::Expire { ticket: 1 });
    refused_as(early, code::NOT_YET, "Expire before the deadline");
    assert!(
        row::<F>(&host, 1).is_some(),
        "awaits: an early Expire keeps the ticket open"
    );
    send::<F>(&host, signed(), deadline, &Op::Expire { ticket: 1 })
        .unwrap_or_else(|e| panic!("awaits: anyone expires a ticket at its deadline: {e:?}"));
    assert_eq!(
        host.take_resumes(),
        [resumed(4, 1, Err(expired(1, deadline)))],
        "awaits: Expire resumes the frame with the expired refusal"
    );
    assert_eq!(row::<F>(&host, 1), None, "awaits: Expire closes the ticket");
    let again = send::<F>(&host, signed(), deadline, &Op::Expire { ticket: 1 });
    assert!(
        again.is_err(),
        "awaits: a closed ticket does not expire twice"
    );
}

pub fn a_close_drops_the_frames_tickets_alone<F: Fixture>(fixture: &F) {
    let host = fixture.host();
    park::<F>(&host, HEIGHT, 4, &[(1, ASKED), (2, OTHER)]);
    park::<F>(&host, HEIGHT, 5, &[(3, ASKED)]);
    send::<F>(&host, Origin::Root, HEIGHT, &Op::Close { frame: 4 })
        .unwrap_or_else(|e| panic!("awaits: the kernel closes a frame: {e:?}"));
    assert_eq!(
        row::<F>(&host, 1),
        None,
        "awaits: Close drops the frame's tickets"
    );
    assert_eq!(
        row::<F>(&host, 2),
        None,
        "awaits: Close drops the frame's tickets"
    );
    assert!(
        row::<F>(&host, 3).is_some(),
        "awaits: Close keeps another frame's tickets"
    );
    assert_eq!(host.take_resumes(), [], "awaits: Close resumes nothing");
    assert_eq!(owed::<F>(&host, ASKED, None, 10), [3]);
}

pub fn owed_pages_a_targets_open_tickets<F: Fixture>(fixture: &F) {
    let host = fixture.host();
    park::<F>(&host, HEIGHT, 4, &[(1, ASKED), (2, OTHER), (3, ASKED)]);
    park::<F>(&host, HEIGHT, 5, &[(5, ASKED)]);
    assert_eq!(
        owed::<F>(&host, ASKED, None, 2),
        [1, 3],
        "awaits: Owed pages by limit"
    );
    assert_eq!(
        owed::<F>(&host, ASKED, Some(3), 2),
        [5],
        "awaits: Owed resumes after a ticket"
    );
    assert_eq!(owed::<F>(&host, ASKED, Some(5), 2), [], "awaits: Owed ends");
    assert_eq!(
        owed::<F>(&host, OTHER, None, 10),
        [2],
        "awaits: Owed is the target's alone"
    );
    resolve::<F>(&host, by(ASKED), 3, Ok(vec![])).unwrap();
    assert_eq!(
        owed::<F>(&host, ASKED, None, 10),
        [1, 5],
        "awaits: a resolved ticket is owed no longer"
    );
}

pub fn overdue_pages_what_may_expire<F: Fixture>(fixture: &F) {
    let host = fixture.host();
    park::<F>(&host, HEIGHT, 4, &[(1, ASKED), (2, OTHER)]);
    park::<F>(&host, HEIGHT + 5, 5, &[(3, ASKED)]);
    let due = HEIGHT + EXPIRY_BLOCKS;
    assert_eq!(
        overdue::<F>(&host, due - 1, None, 10),
        [],
        "awaits: nothing is due early"
    );
    assert_eq!(
        overdue::<F>(&host, due, None, 10),
        [1, 2],
        "awaits: Overdue lists every ticket due by the height"
    );
    assert_eq!(
        overdue::<F>(&host, due + 5, None, 2),
        [1, 2],
        "awaits: Overdue pages by limit"
    );
    assert_eq!(
        overdue::<F>(&host, due + 5, Some(2), 2),
        [3],
        "awaits: Overdue resumes after a ticket"
    );
}
