use abi::role::awaits::{Op, Query, Reply, Ticket, TicketRow};
use borsh::{BorshDeserialize, BorshSerialize};
use program::{
    Awaiting, Ctx, Error, ExecCtx, MessageId, MockChain, Origin, Outcome, Program, QueryCtx, code,
};

use crate::{Awaits, PROGRAM};

#[derive(Debug, BorshSerialize, BorshDeserialize)]
struct Plan {
    target: String,
    asks: Vec<Vec<u8>>,
    fail: bool,
}

type Answers = Vec<Result<Vec<u8>, Error>>;

struct Asker;

impl Program for Asker {
    const NAME: &'static str = "asker";
    type Op = Plan;
    type Query = ();
    type Reply = Option<Answers>;

    fn query(ctx: &QueryCtx, (): ()) -> Result<Option<Answers>, Error> {
        ctx.record("answers")
    }
}

impl Awaiting for Asker {
    async fn execute(ctx: Ctx, plan: Plan) -> Result<(), Error> {
        let mut answers = Answers::new();
        for ask in plan.asks {
            let answer = ctx.call_raw(plan.target.clone(), ask).await;
            answers.push(answer);
            ctx.put("answers", &answers);
        }
        if plan.fail {
            return Err(Error::new(code::INVALID_INPUT, "asked to fail"));
        }
        ctx.set_return_data(abi::encode(&(answers, ctx.env().height)));
        Ok(())
    }
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
enum Tell {
    Now(Vec<u8>),
    Later,
    Refuse(String),
    Answer { ticket: Ticket, bytes: Vec<u8> },
}

struct Teller;

impl Program for Teller {
    const NAME: &'static str = "teller";
    type Op = Tell;
    type Query = ();
    type Reply = ();

    fn execute(ctx: &ExecCtx, tell: Tell) -> Result<(), Error> {
        match tell {
            Tell::Now(bytes) => ctx.set_return_data(bytes),
            Tell::Later => {
                let ticket = ctx.defer()?;
                ctx.put("owed", &ticket);
            }
            Tell::Refuse(sentence) => return Err(Error::new(code::INVALID_INPUT, sentence)),
            Tell::Answer { ticket, bytes } => {
                ctx.resolve(ticket, Ok(bytes))?;
            }
        }
        Ok(())
    }

    fn query(_: &QueryCtx, (): ()) -> Result<(), Error> {
        Ok(())
    }
}

struct Sender;

impl Program for Sender {
    const NAME: &'static str = "sender";
    type Op = Plan;
    type Query = ();
    type Reply = Option<Outcome>;

    fn execute(ctx: &ExecCtx, plan: Plan) -> Result<(), Error> {
        ctx.emit("asker", &plan, program::Reply::Wanted);
        Ok(())
    }

    fn reply(ctx: &ExecCtx, _: &MessageId, outcome: &Outcome) -> Result<(), Error> {
        ctx.put("outcome", outcome);
        Ok(())
    }

    fn query(ctx: &QueryCtx, (): ()) -> Result<Option<Outcome>, Error> {
        ctx.record("outcome")
    }
}

fn chain() -> MockChain {
    let mut chain = MockChain::default();
    chain.seat::<Awaits>(PROGRAM);
    chain.seat_awaiting::<Asker>("asker");
    chain.seat_awaiting::<Asker>("outer");
    chain.seat::<Teller>("teller");
    chain.seat::<Sender>("sender");
    chain.hold(b"ada".to_vec(), 1);
    chain
}

fn ada() -> Origin {
    Origin::Signed(b"ada".to_vec())
}

fn plan(target: &str, asks: Vec<Tell>, fail: bool) -> Plan {
    Plan {
        target: target.into(),
        asks: asks.iter().map(abi::encode).collect(),
        fail,
    }
}

fn answer(chain: &MockChain, ticket: Ticket, bytes: &[u8]) -> Result<Vec<u8>, Error> {
    let bytes = bytes.to_vec();
    chain.submit(ada(), "teller", &Tell::Answer { ticket, bytes })
}

fn ticket(chain: &MockChain, ticket: Ticket) -> Option<TicketRow> {
    match chain.query(PROGRAM, &Query::Ticket { ticket }).unwrap() {
        Reply::Ticket(row) => row,
        other => panic!("{other:?}"),
    }
}

fn tickets(chain: &MockChain, query: Query) -> Vec<Ticket> {
    match chain.query(PROGRAM, &query).unwrap() {
        Reply::Tickets(rows) => rows.into_iter().map(|row| row.ticket).collect(),
        other => panic!("{other:?}"),
    }
}

fn answers(chain: &MockChain, program: &str) -> Answers {
    chain
        .query::<_, Option<Answers>>(program, &())
        .unwrap()
        .unwrap_or_default()
}

fn finished(output: &[u8]) -> (Answers, u64) {
    abi::decode(output).unwrap()
}

#[test]
fn a_call_answered_at_once_resumes_the_frame_in_the_same_submission() {
    let chain = chain();
    let asks = vec![Tell::Now(b"one".to_vec()), Tell::Refuse("no".into())];
    let output = chain
        .submit(ada(), "asker", &plan("teller", asks, false))
        .unwrap();
    let refused = Error::new(code::INVALID_INPUT, "no");
    assert_eq!(
        finished(&output),
        (vec![Ok(b"one".to_vec()), Err(refused)], 1)
    );
    assert_eq!(chain.frames(), 0);
    assert_eq!(ticket(&chain, 0), None);
}

#[test]
fn a_deferred_call_parks_the_frame_until_the_called_program_resolves_it() {
    let chain = chain();
    let asks = vec![Tell::Later, Tell::Later];
    chain
        .submit(ada(), "asker", &plan("teller", asks, false))
        .unwrap();
    assert_eq!(chain.frames(), 1);
    let row = ticket(&chain, 0).expect("the call holds a ticket");
    assert_eq!(
        (row.caller.as_str(), row.target.as_str(), row.opened),
        ("asker", "teller", 1)
    );
    assert_eq!(
        tickets(
            &chain,
            Query::Owed {
                target: "teller".into(),
                after: None,
                limit: 9
            }
        ),
        [0]
    );

    chain.at(2, 20);
    answer(&chain, 0, b"A").unwrap();
    assert_eq!(answers(&chain, "asker"), vec![Ok(b"A".to_vec())]);
    assert_eq!(
        (ticket(&chain, 0), ticket(&chain, 1).is_some()),
        (None, true)
    );

    chain.at(3, 30);
    answer(&chain, 1, b"B").unwrap();
    assert_eq!(
        answers(&chain, "asker"),
        vec![Ok(b"A".to_vec()), Ok(b"B".to_vec())]
    );
    assert_eq!(chain.frames(), 0);
    assert_eq!(ticket(&chain, 1), None);
}

#[test]
fn a_parked_frame_replies_to_its_sender_in_the_block_it_finishes() {
    let chain = chain();
    chain
        .submit(ada(), "sender", &plan("teller", vec![Tell::Later], false))
        .unwrap();
    assert_eq!(
        chain.query::<_, Option<Outcome>>("sender", &()).unwrap(),
        None
    );
    chain.at(7, 70);
    answer(&chain, 0, b"A").unwrap();
    let outcome: Option<Outcome> = chain.query("sender", &()).unwrap();
    let Some(Outcome::Applied { output }) = outcome else {
        panic!("sender got {outcome:?}");
    };
    assert_eq!(finished(&output), (vec![Ok(b"A".to_vec())], 7));
}

#[test]
fn only_the_called_program_resolves_and_only_the_kernel_parks_or_closes() {
    let chain = chain();
    chain
        .submit(ada(), "asker", &plan("teller", vec![Tell::Later], false))
        .unwrap();
    let forged = Op::Resolve {
        ticket: 0,
        answer: Ok(b"forged".to_vec()),
    };
    let refused = chain.submit(ada(), PROGRAM, &forged).unwrap_err();
    assert_eq!(refused.code, code::UNAUTHORIZED);
    for op in [
        Op::Park {
            frame: 9,
            program: "asker".into(),
            calls: Vec::new(),
        },
        Op::Close { frame: 0 },
    ] {
        assert_eq!(
            chain.submit(ada(), PROGRAM, &op).unwrap_err().code,
            code::UNAUTHORIZED
        );
    }
    assert!(ticket(&chain, 0).is_some());
    assert_eq!(chain.frames(), 1);
}

#[test]
fn a_called_frame_that_parks_answers_its_caller_once_it_finishes() {
    let chain = chain();
    let inner = plan("teller", vec![Tell::Later], false);
    let outer = Plan {
        target: "asker".into(),
        asks: vec![abi::encode(&inner)],
        fail: false,
    };
    chain.submit(ada(), "outer", &outer).unwrap();
    assert_eq!(chain.frames(), 2);
    answer(&chain, 1, b"A").unwrap();
    let inner_answers = vec![Ok(b"A".to_vec())];
    assert_eq!(answers(&chain, "asker"), inner_answers);
    assert_eq!(
        answers(&chain, "outer"),
        vec![Ok(abi::encode(&(inner_answers, 1u64)))]
    );
    assert_eq!(chain.frames(), 0);
}

#[test]
fn a_rejected_resume_keeps_what_earlier_submissions_wrote() {
    let chain = chain();
    chain
        .submit(
            ada(),
            "asker",
            &plan("teller", vec![Tell::Later, Tell::Later], true),
        )
        .unwrap();
    answer(&chain, 0, b"A").unwrap();
    answer(&chain, 1, b"B").unwrap();
    assert_eq!(answers(&chain, "asker"), vec![Ok(b"A".to_vec())]);
    assert_eq!(chain.frames(), 0);
}
