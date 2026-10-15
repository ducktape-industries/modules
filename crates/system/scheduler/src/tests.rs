use borsh::{BorshDeserialize, BorshSerialize};
use program::{
    Awaiting, Cause, Ctx, EntryId, Env, Error, MockChain, MockHost, Origin, Program, QueryCtx,
    invalid,
};

use crate::{Entry, PROGRAM, Query, Reply, Scheduler};

mod chain;
mod rules;

const KEY: &[u8] = b"ada-key";
const PLANNER: u64 = 900;
const OTHER: u64 = 901;
const MARK: u64 = 902;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
enum Mark {
    Note(String),
    Count(String),
    Refuse(String),
    Again { height: u64, key: String },
    Pair { height: u64, key: String },
    CancelNext,
}

struct Marker;

impl Program for Marker {
    const NAME: &'static str = "mark";
    type Op = Mark;
    type Query = String;
    type Reply = Option<Vec<u8>>;

    fn query(ctx: &QueryCtx, key: String) -> Result<Option<Vec<u8>>, Error> {
        Ok(ctx.get(key))
    }
}

impl Awaiting for Marker {
    async fn execute(ctx: Ctx, op: Mark) -> Result<(), Error> {
        match op {
            Mark::Note(key) => {
                ctx.put(key, &*ctx.env());
                Ok(())
            }
            Mark::Count(key) => {
                let count: u64 = ctx.record("count")?.unwrap_or(0);
                ctx.put("count", &(count + 1));
                ctx.put(key, &count);
                Ok(())
            }
            Mark::Refuse(key) => {
                ctx.put(key, &*ctx.env());
                Err(invalid("asked to refuse"))
            }
            Mark::Again { height, key } => {
                ctx.schedule(height, "mark", &Mark::Note(key)).await?;
                Ok(())
            }
            Mark::Pair { height, key } => {
                ctx.schedule(height, "mark", &Mark::CancelNext).await?;
                ctx.schedule(height, "mark", &Mark::Note(key)).await?;
                Ok(())
            }
            Mark::CancelNext => {
                let Cause::Scheduled { entry } = ctx.env().cause else {
                    return Err(invalid("only a scheduled run cancels the next entry"));
                };
                ctx.cancel(entry + 1).await
            }
        }
    }
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
enum Plan {
    Schedule { height: u64, op: Mark },
    Cancel(EntryId),
}

type Answers = Vec<Result<Option<EntryId>, Error>>;

struct Planner;

impl Program for Planner {
    const NAME: &'static str = "planner";
    type Op = Vec<Plan>;
    type Query = ();
    type Reply = Option<Answers>;

    fn query(ctx: &QueryCtx, (): ()) -> Result<Option<Answers>, Error> {
        ctx.record("answers")
    }
}

impl Awaiting for Planner {
    async fn execute(ctx: Ctx, plans: Vec<Plan>) -> Result<(), Error> {
        let mut answers = Answers::new();
        for plan in plans {
            let answer = match plan {
                Plan::Schedule { height, op } => ctx.schedule(height, "mark", &op).await.map(Some),
                Plan::Cancel(entry) => ctx.cancel(entry).await.map(|()| None),
            };
            answers.push(answer);
        }
        ctx.put("answers", &answers);
        Ok(())
    }
}

fn chain(bound: bool) -> MockChain {
    let mut roles = MockHost::roles();
    if !bound {
        roles = roles
            .iter()
            .filter(|(role, _)| *role != abi::role::SCHEDULER)
            .collect();
    }
    let mut chain = MockChain::founded(roles);
    chain.seat::<Scheduler>(PROGRAM);
    chain.seat_awaiting::<Planner>("planner");
    chain.seat_awaiting::<Planner>("other");
    chain.seat_awaiting::<Marker>("mark");
    chain.register("planner", PLANNER);
    chain.register("other", OTHER);
    chain.register("mark", MARK);
    chain.hold(KEY, 1);
    chain
}

fn plan(chain: &MockChain, program: &str, plans: Vec<Plan>) {
    chain
        .submit(Origin::Signed(KEY.to_vec()), program, &plans)
        .unwrap();
}

fn answers(chain: &MockChain, program: &str) -> Answers {
    chain
        .query::<(), Option<Answers>>(program, &())
        .unwrap()
        .expect("the program kept its answers")
}

fn scheduled(chain: &MockChain, program: &str) -> Vec<EntryId> {
    answers(chain, program)
        .into_iter()
        .map(|answer| answer.unwrap().expect("an entry id"))
        .collect()
}

fn entry(chain: &MockChain, id: EntryId) -> Option<Entry> {
    match chain.query(PROGRAM, &Query::Entry { id }).unwrap() {
        Reply::Entry(entry) => entry,
        other => panic!("{other:?}"),
    }
}

fn kept<T: BorshDeserialize>(chain: &MockChain, key: &str) -> Option<T> {
    let bytes: Option<Vec<u8>> = chain.query("mark", &key.to_owned()).unwrap();
    bytes.map(|bytes| abi::decode(&bytes).unwrap())
}

fn noted(chain: &MockChain, key: &str) -> Option<Env> {
    kept(chain, key)
}

fn note(height: u64, key: &str) -> Plan {
    Plan::Schedule {
        height,
        op: Mark::Note(key.into()),
    }
}
