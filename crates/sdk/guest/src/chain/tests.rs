//! Each of the kernel's frame rules, run natively. `Bot` is one module
//! seated under several names; its op is a script of steps, and every run
//! logs the env it ran in on its host, so a test reads who ran, sent by
//! whom, caused by what.

use abi::role::identity::{Category, Kind, Profile, Query as Asked, Reply as Answer, Standing};
use borsh::{BorshDeserialize, BorshSerialize};

use super::*;
use crate::{HashKind, Program, Reply};

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
enum Step {
    Put(String, String),
    Emit {
        target: String,
        steps: Vec<Step>,
        reply: bool,
    },
    /// Emits `Recurse(n - 1)` to itself while `n > 0`.
    Recurse(u32),
    /// Records what `target` holds under `key`, as `peek:<key>`.
    Peek(String, String),
    Person(AccountNumber),
    Blob,
    Event,
    Output(Vec<u8>),
    Refuse,
}

struct Bot;

impl Program for Bot {
    const NAME: &'static str = "bot";
    type Op = Vec<Step>;
    type Query = String;
    type Reply = Option<Vec<u8>>;
}

impl Module for Bot {
    /// Genesis runs a script too.
    fn init(ctx: &ExecCtx, params: &[u8]) -> Result<(), Error> {
        Bot::execute(ctx, crate::decoded("bot", "init", params)?)
    }

    fn execute(ctx: &ExecCtx, steps: Vec<Step>) -> Result<(), Error> {
        log(ctx)?;
        for step in steps {
            match step {
                Step::Put(key, value) => ctx.set(key, value),
                Step::Emit {
                    target,
                    steps,
                    reply,
                } => {
                    let reply = if reply { Reply::Wanted } else { Reply::None };
                    let id = ctx.emit(target, &steps, reply);
                    let mut emitted: Vec<MessageId> = ctx.record("emitted")?.unwrap_or_default();
                    emitted.push(id);
                    ctx.put("emitted", &emitted);
                }
                Step::Recurse(n) if n > 0 => {
                    let me = ctx.env().module.clone();
                    ctx.emit(me, &vec![Step::Recurse(n - 1)], Reply::None);
                }
                Step::Recurse(_) => {}
                Step::Peek(target, key) => {
                    let seen: Option<Vec<u8>> = ctx.query(target, &key)?;
                    ctx.set(format!("peek:{key}"), seen.unwrap_or_default());
                }
                Step::Person(number) => ctx.require_person_or_agent(number)?,
                Step::Blob => {
                    ctx.blob_put(HashKind::Sha256, "text", b"body".to_vec())?;
                }
                Step::Event => ctx.event(b"happened".to_vec()),
                Step::Output(bytes) => ctx.set_return_data(bytes),
                Step::Refuse => return Err(Error::new(code::WRONG_STATE, "as scripted")),
            }
        }
        Ok(())
    }

    /// Logged like a run; absorbed when the host holds `absorb`, else the
    /// default (a refusal comes back).
    fn reply(ctx: &ExecCtx, id: &MessageId, outcome: &Outcome) -> Result<(), Error> {
        log(ctx)?;
        if ctx.get("absorb").is_some() {
            return Ok(());
        }
        match outcome {
            Outcome::Applied { .. } => Ok(()),
            Outcome::Rejected(refusal) => Err(refusal.clone()),
        }
        .map_err(|refusal| Error::new(refusal.code, format!("{id:?}: {}", refusal.message)))
    }

    fn query(ctx: &QueryCtx, key: String) -> Result<Option<Vec<u8>>, Error> {
        Ok(ctx.get(key))
    }
}

fn log(ctx: &ExecCtx) -> Result<(), Error> {
    let mut log: Vec<Env> = ctx.record("log")?.unwrap_or_default();
    log.push(ctx.env().clone());
    ctx.put("log", &log);
    Ok(())
}

/// The identity module the chain asks when one is seated: a key of one
/// byte holds that account, an empty key none; a module's account is 100
/// past its name's first byte; module `bad` has none it will name. Asked
/// any other way than the kernel's (origin `Root`, no sender), it answers
/// account 0, so the env the chain asks with shows.
struct Ident;

impl Program for Ident {
    const NAME: &'static str = "ident";
    type Op = ();
    type Query = Asked;
    type Reply = Answer;
}

impl Module for Ident {
    fn execute(_: &ExecCtx, (): ()) -> Result<(), Error> {
        Ok(())
    }

    fn query(ctx: &QueryCtx, asked: Asked) -> Result<Answer, Error> {
        let as_the_kernel = ctx.env().origin == Origin::Root && ctx.env().sender.is_none();
        Ok(match asked {
            _ if !as_the_kernel => Answer::Account(Some(0)),
            Asked::Account(key) => Answer::Account(key.first().map(|byte| *byte as u64)),
            Asked::OfModule(module) if module == "bad" => {
                return Err(Error::new(code::NOT_FOUND, "bad has no account"));
            }
            Asked::OfModule(module) => {
                Answer::Account(module.bytes().next().map(|byte| 100 + byte as u64))
            }
            Asked::Profile(_) => Answer::Profile(None),
            Asked::Profiles { .. } => Answer::Profiles {
                profiles: Vec::new(),
                next: None,
            },
        })
    }
}

/// `a`, `b` and `c` seated, each with a module account in the roster (100,
/// 101, 102), and `ada-key` holding account 1.
fn chain() -> MockChain {
    let mut chain = MockChain::default();
    for (name, number) in [("a", 100), ("b", 101), ("c", 102)] {
        chain.seat::<Bot>(name);
        chain.register(name, number);
    }
    chain.hold(b"ada-key".to_vec(), 1);
    chain
}

fn ada() -> Origin {
    Origin::Signed(b"ada-key".to_vec())
}

fn emit(target: &str, steps: Vec<Step>, reply: bool) -> Step {
    Step::Emit {
        target: target.into(),
        steps,
        reply,
    }
}

fn put(key: &str, value: &str) -> Step {
    Step::Put(key.into(), value.into())
}

fn logged(chain: &MockChain, module: &str) -> Vec<Env> {
    let host = chain.host(module);
    host.query(chain.env(module))
        .record("log")
        .unwrap()
        .unwrap_or_default()
}

fn id(module: &str, seq: u64) -> MessageId {
    MessageId {
        module: module.into(),
        seq,
    }
}

fn account(number: AccountNumber) -> Option<Principal> {
    Some(Principal::Account(number))
}

/// A host's state, blobs, output, emissions and events, for a before/after.
#[allow(clippy::type_complexity)]
fn left(
    chain: &MockChain,
    module: &str,
) -> (Vec<(Vec<u8>, Vec<u8>)>, usize, Vec<u8>, usize, usize) {
    let host = chain.host(module).borrow();
    (
        host.state.clone().into_iter().collect(),
        host.blobs.len(),
        host.output.clone(),
        host.emissions.len(),
        host.events.len(),
    )
}

#[test]
fn messages_run_after_the_emitter_in_emit_order_depth_first_as_the_emitters_account() {
    let chain = chain();
    chain.at(9, 90);
    // a emits twice to b; b's first run emits to c, b's second peeks at c
    let script = vec![
        emit(
            "b",
            vec![emit("c", vec![put("seen", "by b")], false)],
            false,
        ),
        emit("b", vec![Step::Peek("c".into(), "seen".into())], false),
        put("own", "kept"),
    ];
    chain.submit(ada(), "a", &script).unwrap();
    let (a, b, c) = (
        logged(&chain, "a"),
        logged(&chain, "b"),
        logged(&chain, "c"),
    );
    assert_eq!(a.len(), 1, "a ran once");
    assert_eq!(a[0].origin, ada());
    assert_eq!(
        (a[0].sender.clone(), a[0].cause.clone()),
        (account(1), Cause::Direct)
    );
    let message = |cause| Env {
        module: "b".into(),
        origin: Origin::Module("a".into()),
        sender: account(100),
        cause: Cause::Message(cause),
        ..chain.env("b")
    };
    assert_eq!(b, [message(id("a", 0)), message(id("a", 1))]);
    assert_eq!((b[0].height, b[0].time), (9, 90));
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].origin, Origin::Module("b".into()));
    assert_eq!(c[0].sender, account(101));
    // ids are numbered frame-wide, each naming its emitter
    assert_eq!(c[0].cause, Cause::Message(id("b", 2)));
    let emitted = |module: &str| -> Vec<MessageId> {
        chain
            .host(module)
            .query(chain.env(module))
            .record("emitted")
            .unwrap()
            .unwrap()
    };
    assert_eq!(emitted("a"), [id("a", 0), id("a", 1)]);
    assert_eq!(emitted("b"), [id("b", 2)]);
    // depth first: b's second run saw what c wrote in b's first
    assert_eq!(
        chain.host("b").borrow().state[b"peek:seen".as_slice()],
        b"by b"
    );
    assert_eq!(chain.host("a").borrow().state[b"own".as_slice()], b"kept");
    // the next submission numbers from 0 again
    chain
        .submit(ada(), "a", &vec![emit("b", vec![], false)])
        .unwrap();
    assert_eq!(logged(&chain, "b")[2].cause, Cause::Message(id("a", 0)));
}

#[test]
fn a_wanted_reply_brings_the_outcome_back_as_the_targets_message() {
    let chain = chain();
    // applied: the target's output comes back
    let script = vec![emit("b", vec![Step::Output(b"hi".to_vec())], true)];
    chain.submit(ada(), "a", &script).unwrap();
    let a = logged(&chain, "a");
    assert_eq!(a.len(), 2);
    assert_eq!(
        a[1],
        Env {
            origin: Origin::Module("b".into()),
            sender: account(101),
            cause: Cause::Reply {
                id: id("a", 0),
                outcome: Outcome::Applied {
                    output: b"hi".to_vec()
                },
            },
            ..chain.env("a")
        }
    );
    // rejected and absorbed: the target's writes are undone, the emitter's stand
    chain
        .host("a")
        .borrow_mut()
        .state
        .insert(b"absorb".to_vec(), Vec::new());
    let script = vec![
        put("mine", "stays"),
        emit("b", vec![put("theirs", "gone"), Step::Refuse], true),
    ];
    chain.submit(ada(), "a", &script).unwrap();
    let a = logged(&chain, "a");
    assert_eq!(a.len(), 4);
    let Cause::Reply {
        id: replied,
        outcome,
    } = &a[3].cause
    else {
        panic!("{:?}", a[3].cause)
    };
    assert_eq!(*replied, id("a", 0));
    assert!(matches!(outcome, Outcome::Rejected(refusal) if refusal.code == code::WRONG_STATE));
    assert!(
        chain
            .host("a")
            .borrow()
            .state
            .contains_key(b"mine".as_slice())
    );
    assert!(
        !chain
            .host("b")
            .borrow()
            .state
            .contains_key(b"theirs".as_slice())
    );
    assert_eq!(
        logged(&chain, "b").len(),
        1,
        "b's rejected run was undone, its log too"
    );
}

#[test]
fn a_rejection_not_absorbed_undoes_the_emitter_whole_and_propagates_up() {
    let chain = chain();
    let before = left(&chain, "a");
    // no reply wanted: c's refusal fails b, which fails a
    let script = vec![
        put("mine", "undone"),
        emit(
            "b",
            vec![
                put("theirs", "undone"),
                emit("c", vec![put("deep", "undone"), Step::Refuse], false),
            ],
            false,
        ),
    ];
    let refusal = chain.submit(ada(), "a", &script).unwrap_err();
    assert_eq!(refusal.code, code::WRONG_STATE);
    assert_eq!(left(&chain, "a"), before);
    for module in ["a", "b", "c"] {
        assert!(logged(&chain, module).is_empty(), "{module} was undone");
    }
    // a reply run that refuses (the default, on a rejected outcome) does the same
    let script = vec![put("mine", "undone"), emit("b", vec![Step::Refuse], true)];
    let refusal = chain.submit(ada(), "a", &script).unwrap_err();
    assert_eq!(refusal.code, code::WRONG_STATE);
    assert!(refusal.message.contains("seq: 0"), "{refusal}");
    assert_eq!(left(&chain, "a"), before);
    // the emitter's own refusal after emitting: nothing it emitted runs
    let script = vec![emit("b", vec![], false), Step::Refuse];
    chain.submit(ada(), "a", &script).unwrap_err();
    assert!(logged(&chain, "b").is_empty());
}

#[test]
fn undo_covers_every_hosts_state_blobs_output_events_and_emissions() {
    let chain = chain();
    let all = || [left(&chain, "a"), left(&chain, "b"), left(&chain, "c")];
    let before = all();
    let script = vec![
        put("k", "v"),
        Step::Blob,
        Step::Event,
        Step::Output(b"out".to_vec()),
        emit(
            "b",
            vec![
                put("k", "v"),
                Step::Blob,
                Step::Event,
                emit("c", vec![Step::Event, Step::Refuse], false),
            ],
            false,
        ),
    ];
    chain.submit(ada(), "a", &script).unwrap_err();
    assert_eq!(all(), before);
    // and the same script, its refusal gone, leaves all of it
    let script = vec![
        put("k", "v"),
        Step::Blob,
        Step::Event,
        Step::Output(b"out".to_vec()),
    ];
    assert_eq!(chain.submit(ada(), "a", &script).unwrap(), b"out");
    let (state, blobs, output, _, events) = left(&chain, "a");
    assert_eq!((state.len(), blobs, events), (2, 1, 1));
    assert!(
        output.is_empty(),
        "the output was handed over, not left on the host"
    );
}

#[test]
fn messages_nest_no_deeper_than_max_depth() {
    let chain = chain();
    chain
        .submit(ada(), "a", &vec![Step::Recurse(MAX_DEPTH)])
        .unwrap();
    assert_eq!(logged(&chain, "a").len() as u32, MAX_DEPTH + 1);
    let refusal = chain
        .submit(ada(), "a", &vec![Step::Recurse(MAX_DEPTH + 1)])
        .unwrap_err();
    assert_eq!(refusal.code, code::CAPACITY);
    assert_eq!(
        logged(&chain, "a").len() as u32,
        MAX_DEPTH + 1,
        "undone whole"
    );
}

#[test]
fn a_submission_acts_as_the_account_its_key_holds_or_as_no_one() {
    let chain = chain();
    chain.hold(b"agent-key".to_vec(), 5);
    chain.profile(Profile {
        number: 5,
        name: "agent".into(),
        kind: Kind::Managed {
            manager: 1,
            category: Category::Agent,
            standing: Standing::Suspended,
        },
    });
    let nothing = abi::encode(&Vec::<Step>::new());
    chain.submit_raw(ada(), "a", &nothing).unwrap();
    chain
        .submit_raw(Origin::Signed(b"loose".to_vec()), "a", &nothing)
        .unwrap();
    let refused = chain
        .submit_raw(Origin::Signed(b"agent-key".to_vec()), "a", &nothing)
        .unwrap_err();
    // identity's own refusal of a key whose account does not act
    assert_eq!(refused.code, code::UNAUTHORIZED);
    // the chain's own conveniences: a message by hand, a root call
    chain
        .submit_raw(Origin::Module("b".into()), "a", &nothing)
        .unwrap();
    chain.submit_raw(Origin::Root, "a", &nothing).unwrap();
    let senders: Vec<Option<Principal>> = logged(&chain, "a")
        .into_iter()
        .map(|env| env.sender)
        .collect();
    assert_eq!(
        senders,
        [account(1), None, account(101), Some(Principal::Root)]
    );
    let unknown = chain.submit_raw(ada(), "nobody", &nothing).unwrap_err();
    assert_eq!(unknown.code, code::UNKNOWN_MODULE);
}

#[test]
fn the_roster_answers_modules_as_the_identity_role_does() {
    let chain = chain();
    chain.profile(Profile {
        number: 6,
        name: "idle".into(),
        kind: Kind::Managed {
            manager: 1,
            category: Category::Agent,
            standing: Standing::Revoked,
        },
    });
    chain.submit(ada(), "a", &vec![Step::Person(1)]).unwrap();
    let refused = chain
        .submit(ada(), "a", &vec![Step::Person(6)])
        .unwrap_err();
    assert_eq!(refused.code, code::WRONG_STATE);
    let refused = chain
        .submit(ada(), "a", &vec![Step::Person(100)])
        .unwrap_err();
    assert_eq!(
        refused.code,
        code::INVALID_INPUT,
        "a module's account is no person"
    );
    // the chain reads a module's account off its profile
    assert_eq!(
        chain
            .roster()
            .account(&Asked::OfModule("b".into()))
            .unwrap(),
        Some(101)
    );
    assert_eq!(
        chain
            .roster()
            .account(&Asked::OfModule("zed".into()))
            .unwrap(),
        None
    );
}

#[test]
fn a_seated_identity_module_is_asked_as_the_kernel_asks_it() {
    let mut chain = chain();
    chain.seat::<Ident>(&MockHost::roles().identity);
    chain.seat::<Bot>("bad");
    let script = vec![emit("b", vec![], false)];
    chain.submit(Origin::Signed(vec![7]), "a", &script).unwrap();
    chain.submit(Origin::Signed(vec![]), "a", &script).unwrap();
    let a = logged(&chain, "a");
    assert_eq!(
        (a[0].sender.clone(), a[1].sender.clone()),
        (account(7), None)
    );
    assert_eq!(
        logged(&chain, "b")[0].sender,
        account(100 + u64::from(b'a'))
    );
    // ada's key in the roster means nothing now: identity answers
    chain.submit(ada(), "a", &script).unwrap();
    assert_eq!(logged(&chain, "a")[2].sender, account(u64::from(b'a')));
    // an emitter identity refuses to name: its message is rejected...
    let refused = chain.submit(Origin::Root, "bad", &script).unwrap_err();
    assert_eq!(refused.code, code::NOT_FOUND);
    assert!(logged(&chain, "b").len() == 3 && logged(&chain, "bad").is_empty());
    // ...which, with a reply wanted, comes back as its outcome
    chain
        .host("bad")
        .borrow_mut()
        .state
        .insert(b"absorb".to_vec(), Vec::new());
    let script = vec![emit("b", vec![], true)];
    chain.submit(Origin::Root, "bad", &script).unwrap();
    let bad = logged(&chain, "bad");
    assert_eq!(bad.len(), 2);
    assert!(
        matches!(&bad[1].cause, Cause::Reply { outcome: Outcome::Rejected(refusal), .. } if refusal.code == code::NOT_FOUND),
        "{:?}",
        bad[1].cause
    );
    // identity answering anything but Account is an unexpected reply
    let mut wrong = MockChain::default();
    wrong.seat::<Bot>("a");
    wrong.seat::<Wrong>(&MockHost::roles().identity);
    let refused = wrong.submit(ada(), "a", &script).unwrap_err();
    assert_eq!(refused.code, code::UNEXPECTED_REPLY);
}

/// An identity that answers every question with a profile.
struct Wrong;

impl Program for Wrong {
    const NAME: &'static str = "wrong";
    type Op = ();
    type Query = Asked;
    type Reply = Answer;
}

impl Module for Wrong {
    fn execute(_: &ExecCtx, (): ()) -> Result<(), Error> {
        Ok(())
    }

    fn query(_: &QueryCtx, _: Asked) -> Result<Answer, Error> {
        Ok(Answer::Profile(None))
    }
}

#[test]
fn init_is_a_frame_its_messages_run_after_it_and_a_refusal_undoes_it() {
    let chain = chain();
    let script = vec![
        put("init", "wrote"),
        emit("b", vec![put("from", "a")], false),
    ];
    chain.init("a", &script).unwrap();
    assert_eq!(logged(&chain, "a")[0].origin, Origin::Root);
    assert_eq!(logged(&chain, "b")[0].origin, Origin::Module("a".into()));
    let before = (left(&chain, "a"), left(&chain, "b"));
    let refused = vec![put("again", "x"), emit("b", vec![Step::Refuse], false)];
    assert_eq!(
        chain.init("a", &refused).unwrap_err().code,
        code::WRONG_STATE
    );
    assert_eq!((left(&chain, "a"), left(&chain, "b")), before);
}

#[test]
fn a_module_queries_itself() {
    let chain = chain();
    chain.submit(ada(), "a", &vec![put("k", "v")]).unwrap();
    let peek = vec![Step::Peek("a".into(), "k".into())];
    chain.submit(ada(), "a", &peek).unwrap();
    let peeked = chain
        .host("a")
        .borrow()
        .state
        .get(b"peek:k".as_slice())
        .cloned();
    assert_eq!(peeked, Some(b"v".to_vec()));
}
