//! A native chain of modules, run as the kernel runs them: several modules
//! each seated by name over its own [`MockHost`], querying each other
//! through the bytes, and a submission run as one frame, its messages after
//! it, depth first, in emit order, undone whole on a rejection it does not
//! absorb (the kernel's `run_frame`, `crates/kernel/host/src/lib.rs`).
//! [`MockChain`] is the kernel a native test lacks; a module alone still
//! runs over a bare [`MockHost`].
//!
//! Left out of the kernel's run, on purpose: fuel (a run never runs out),
//! blob rostering (a blob put stays put), state roots (`ctx.root` is
//! `None`), committed reads (they read the same map as a plain read), the
//! signer's sequence number (a submission is never out of sequence), and
//! the query stack's cycle check (a module asked again while a query of it
//! is being answered is refused as unknown rather than as a cycle), and
//! receipts (a submission is its output or its refusal; what ran nested is
//! read off the hosts, and a rejected run's events go with its writes,
//! where the kernel's receipt would keep them).

use std::cell::{Cell, Ref, RefCell, RefMut};
use std::collections::BTreeMap;
use std::rc::Rc;

use abi::role::identity::{Kind, Profile, Query, Reply, Standing};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::{
    AccountNumber, Cause, Env, Error, ExecCtx, MessageId, MockHost, Module, ModuleId, Origin,
    Outcome, Principal, QueryCtx, Roles, code, identity_role, unauthorized, unexpected_reply,
};

/// How deep messages nest in one frame: a submission runs at depth 0 and
/// each message, or reply, one deeper than the run that emitted it. The
/// kernel's `MAX_DEPTH` (`crates/kernel/host/src/lib.rs`).
pub const MAX_DEPTH: u32 = 8;

/// A module's entry point with its type erased: [`crate::execute`] or
/// [`crate::query`] of one `Module`, over the context the chain makes.
type Entry<Ctx> = Rc<dyn Fn(Ctx, &[u8]) -> Result<(), Error>>;

/// A module seated on the chain: its host and its entry points, typed away
/// so the chain holds any module.
struct Seat {
    host: MockHost,
    init: Entry<ExecCtx>,
    execute: Entry<ExecCtx>,
    query: Entry<QueryCtx>,
}

/// What a run of a frame calls: the module's init with its params (the
/// kernel's admission) or its execute with a payload.
#[derive(Clone, Copy)]
enum Call<'a> {
    Init(&'a [u8]),
    Execute(&'a [u8]),
}

/// Who holds what, for a chain with no identity module seated: each key
/// the account it holds, and each account's profile (a module's account is
/// the profile whose kind names the module). The identity role over it
/// answers modules ([`identity_role`]) and the chain (`Account`,
/// `OfModule`).
///
/// ```ignore
/// let chain = MockChain::default();
/// chain.hold(b"ada-key", 1); // a person, unless a profile says otherwise
/// chain.register("forge", 900);
/// assert_eq!(chain.roster().keys[b"ada-key".as_slice()], 1);
/// ```
#[derive(Default)]
pub struct Roster {
    pub keys: BTreeMap<Vec<u8>, AccountNumber>,
    pub profiles: BTreeMap<AccountNumber, Profile>,
}

impl Roster {
    /// What the kernel asks identity: the account a key holds (refused
    /// while it does not act) or a module's own.
    fn account(&self, asked: &Query) -> Result<Option<AccountNumber>, Error> {
        let number = match asked {
            Query::Account(key) => self.keys.get(key).copied(),
            Query::OfModule(module) => self
                .profiles
                .values()
                .find(|profile| profile.kind == Kind::Module(module.clone()))
                .map(|profile| profile.number),
            Query::Profile(_) | Query::Profiles { .. } => unreachable!("the chain asks who acts"),
        };
        if let Some(number) = number
            && let Some(profile) = self.profiles.get(&number)
            && let Kind::Managed { standing, .. } = profile.kind
            && standing != Standing::Active
        {
            // identity's own words (`of_key`), revoked or suspended
            return Err(unauthorized(format!("account {number} is suspended")));
        }
        Ok(number)
    }

    fn profiles(&self) -> Vec<Profile> {
        self.profiles.values().cloned().collect()
    }
}

/// Several modules over one clock, run as the kernel runs them. The chain
/// is founded with the roles genesis binds ([`founded`](MockChain::founded);
/// [`MockHost::roles`] by default) and is at a height and time
/// ([`at`](MockChain::at) moves it); every module is seated
/// by name ([`seat`](MockChain::seat)) over a host of its own
/// ([`host`](MockChain::host)); a signed submission is one frame
/// ([`submit`](MockChain::submit)). Who a key or a module acts as: the
/// module seated at the identity role's binding, asked as the kernel
/// asks it, or, when none is, the [`Roster`].
///
/// ```ignore
/// let mut chain = MockChain::default();
/// chain.seat::<Forge>("forge");
/// chain.seat::<Chat>("chat");
/// chain.register("forge", 900);
/// chain.hold(b"ada-key", 1);
/// chain.init("forge", &bounds())?;
/// // forge's op, and what it emitted to chat, as one frame
/// let output = chain.submit(Origin::Signed(b"ada-key".to_vec()), "forge", &op)?;
/// let reply: chat::Reply = chain.query("chat", &chat::Query::Roots { .. })?;
/// ```
pub struct MockChain {
    /// What genesis bound each role to: every frame's env carries it.
    roles: Roles,
    seats: BTreeMap<ModuleId, Seat>,
    roster: Rc<RefCell<Roster>>,
    /// The frame-wide number the next emitted message gets.
    next_message: Cell<u64>,
    /// The block every run is in: height and time.
    clock: Cell<(u64, u64)>,
}

impl Default for MockChain {
    fn default() -> MockChain {
        MockChain::founded(MockHost::roles())
    }
}

impl MockChain {
    /// A chain whose genesis binds the roles to `roles`, nothing seated.
    pub fn founded(roles: Roles) -> MockChain {
        MockChain {
            roles,
            seats: BTreeMap::new(),
            roster: Rc::default(),
            next_message: Cell::new(0),
            clock: Cell::new((1, 0)),
        }
    }

    /// Seats `M` as `module`, over a fresh host of its own, which every
    /// module seated may query and which is returned for the test to read
    /// and write (`chain.host(module)` finds it again). Seated at the
    /// identity role's binding, `M` is the identity the chain asks.
    pub fn seat<M: Module>(&mut self, module: impl Into<ModuleId>) -> MockHost {
        let module = module.into();
        let host = MockHost::default();
        let seat = Seat {
            host: host.clone(),
            init: Rc::new(|ctx, params| M::init(&ctx, params)),
            execute: Rc::new(|ctx, payload| crate::execute::<M>(&ctx, payload)),
            query: Rc::new(|ctx, request| crate::query::<M>(&ctx, request)),
        };
        let roster = self.roster.clone();
        host.borrow_mut().siblings.insert(
            self.roles.identity.clone(),
            Box::new(move |_, request| identity_role(&roster.borrow().profiles(), request)),
        );
        // a module may query itself, as the kernel answers it
        host.borrow_mut()
            .siblings
            .insert(module.clone(), seat.sibling());
        for (name, other) in &self.seats {
            other
                .host
                .borrow_mut()
                .siblings
                .insert(module.clone(), seat.sibling());
            host.borrow_mut()
                .siblings
                .insert(name.clone(), other.sibling());
        }
        self.seats.insert(module, seat);
        host
    }

    /// The host `module` is seated over.
    #[track_caller]
    pub fn host(&self, module: &str) -> &MockHost {
        &self.seated(module).expect("a seated module").host
    }

    fn seated(&self, module: &str) -> Result<&Seat, Error> {
        self.seats
            .get(module)
            .ok_or_else(|| Error::new(code::UNKNOWN_MODULE, module))
    }

    /// The roster, to read or edit ([`hold`](MockChain::hold) and
    /// [`profile`](MockChain::profile) are the usual edits).
    pub fn roster(&self) -> Ref<'_, Roster> {
        self.roster.borrow()
    }

    fn roster_mut(&self) -> RefMut<'_, Roster> {
        self.roster.borrow_mut()
    }

    /// Seats `key` in `account`, as identity would; an account no profile
    /// names yet is a person. The roster answers only while no module is
    /// seated at the identity role: then identity's own ops seat keys.
    pub fn hold(&self, key: impl Into<Vec<u8>>, account: AccountNumber) {
        let mut roster = self.roster_mut();
        roster.keys.insert(key.into(), account);
        roster.profiles.entry(account).or_insert_with(|| Profile {
            number: account,
            name: format!("account {account}"),
            kind: Kind::Person,
        });
    }

    /// Adds or replaces an account's profile: a module's account, an agent
    /// and its standing (the roster's, like [`hold`](MockChain::hold)).
    pub fn profile(&self, profile: Profile) {
        self.roster_mut().profiles.insert(profile.number, profile);
    }

    /// Moves the chain to `height` and `time`: the block every later
    /// submission, init and query runs in (height 1, time 0 at first).
    pub fn at(&self, height: u64, time: u64) {
        self.clock.set((height, time));
    }

    /// `module`'s env for a direct call by the chain itself at this height
    /// and time: what [`Module::init`] runs in.
    pub fn env(&self, module: impl Into<ModuleId>) -> Env {
        let (height, time) = self.clock.get();
        Env {
            height,
            time,
            roles: self.roles.clone(),
            ..MockHost::env(module).root()
        }
    }

    /// `module`'s [`init`](Module::init) with `params` (borsh), as the
    /// kernel admits a module: one frame run by the chain itself at this
    /// height, what init emits run after it, and a refusal undoing it all.
    pub fn init(&self, module: &str, params: &impl BorshSerialize) -> Result<(), Error> {
        self.next_message.set(0);
        let params = abi::encode(params);
        self.run_frame(self.env(module), Call::Init(&params), 0)
            .map(drop)
    }

    /// Gives `module` the account `number`, as identity does when the
    /// kernel admits a module: what its messages act as (the roster's,
    /// like [`hold`](MockChain::hold)).
    pub fn register(&self, module: &str, number: AccountNumber) {
        self.profile(Profile {
            number,
            name: module.into(),
            kind: Kind::Module(module.into()),
        });
    }

    /// Runs `op` (`module`'s own `Op`, borsh) as one frame, as the kernel
    /// runs a submission: `origin`'s sender resolved through identity (a
    /// key that holds no account still runs, as no one; a refusal there
    /// rejects the frame), then the module, then what it emitted, depth
    /// first. The module's output, or the refusal that rejected the frame,
    /// which left every host as it was. A `Signed` origin is a submission
    /// as the kernel admits one; `Module` and `Root` are the chain's
    /// convenience, a message or a genesis call sent by hand.
    pub fn submit<O: BorshSerialize>(
        &self,
        origin: Origin,
        module: impl Into<ModuleId>,
        op: &O,
    ) -> Result<Vec<u8>, Error> {
        self.submit_raw(origin, module, abi::encode(op))
    }

    /// [`submit`](MockChain::submit) with the payload's bytes as they are.
    pub fn submit_raw(
        &self,
        origin: Origin,
        module: impl Into<ModuleId>,
        payload: impl AsRef<[u8]>,
    ) -> Result<Vec<u8>, Error> {
        let sender = match &origin {
            Origin::Signed(key) => self.account(Query::Account(key.clone()))?,
            Origin::Module(module) => self.account(Query::OfModule(module.clone()))?,
            Origin::Root => Some(Principal::Root),
        };
        let env = Env {
            origin,
            sender,
            ..self.env(module)
        };
        self.next_message.set(0);
        self.run_frame(env, Call::Execute(payload.as_ref()), 0)
    }

    /// `module`'s answer to `request`, asked by the chain itself (origin
    /// `Root`, no sender), borsh both ways.
    pub fn query<Q: BorshSerialize, R: BorshDeserialize>(
        &self,
        module: &str,
        request: &Q,
    ) -> Result<R, Error> {
        let env = Env {
            sender: None,
            ..self.env(module)
        };
        let answer = self.query_raw(&env, &abi::encode(request))?;
        abi::decode(&answer).map_err(crate::kernel::error_from)
    }

    fn query_raw(&self, env: &Env, request: &[u8]) -> Result<Vec<u8>, Error> {
        let seat = self.seated(&env.module)?;
        (seat.query)(seat.host.query(env.clone()), request)?;
        Ok(seat.host.take_response())
    }

    /// The account the identity role says a frame acts as, asked as the
    /// kernel asks: the seated identity module with the chain's own env, or
    /// the roster. A reply that is not `Account` is refused.
    fn account(&self, asked: Query) -> Result<Option<Principal>, Error> {
        let identity = self.roles.identity.clone();
        let account = if self.seats.contains_key(&identity) {
            let env = Env {
                sender: None,
                ..self.env(identity.clone())
            };
            let answered = self.query_raw(&env, &abi::encode(&asked))?;
            match abi::decode(&answered) {
                Ok(Reply::Account(account)) => account,
                Ok(other) => {
                    return Err(unexpected_reply(&identity, &format!("{asked:?}"), &other));
                }
                Err(fault) => {
                    return Err(unexpected_reply(
                        &identity,
                        &format!("{asked:?}"),
                        &fault.sentence,
                    ));
                }
            }
        } else {
            self.roster().account(&asked)?
        };
        Ok(account.map(Principal::Account))
    }

    /// The kernel's `run_frame`: `env.module` runs `payload`, then the
    /// messages it emitted in order, each at `depth + 1` and depth first,
    /// and the whole run is undone on a rejection it does not absorb: a
    /// rejected message without a reply, or a rejected reply run. A message
    /// with a reply wanted comes back as a [`Cause::Reply`] run of the
    /// emitter, the target's writes undone when it was rejected.
    fn run_frame(&self, env: Env, call: Call, depth: u32) -> Result<Vec<u8>, Error> {
        if depth > MAX_DEPTH {
            return Err(Error::new(
                code::CAPACITY,
                format!("messages nest deeper than {MAX_DEPTH}"),
            ));
        }
        let checkpoint = self.checkpoint();
        let seat = self.seated(&env.module)?;
        let host = &seat.host;
        let first = self.next_message.get();
        host.borrow_mut().next_message = first;
        let ran = match call {
            Call::Init(params) => (seat.init)(host.exec(env.clone()), params),
            Call::Execute(payload) => (seat.execute)(host.exec(env.clone()), payload),
        };
        // this run's output and messages, before a nested run of the same
        // module (a reply) overwrites them
        let output = host.take_output();
        let emitted = host.take_emissions();
        self.next_message.set(first + emitted.len() as u64);
        if let Err(refusal) = ran {
            self.restore(checkpoint);
            return Err(refusal);
        }
        // the emitter's account, asked once for all its messages
        let emitter = if emitted.is_empty() {
            Ok(None)
        } else {
            self.account(Query::OfModule(env.module.clone()))
        };
        let nested = |me: &ModuleId, origin: &ModuleId, sender, cause| Env {
            module: me.clone(),
            origin: Origin::Module(origin.clone()),
            sender,
            cause,
            ..env.clone()
        };
        for (seq, message) in emitted.into_iter().enumerate() {
            let id = MessageId {
                module: env.module.clone(),
                seq: first + seq as u64,
            };
            let ran = match emitter.clone() {
                Err(refusal) => Err(refusal),
                Ok(sender) => {
                    let env = nested(
                        &message.target,
                        &env.module,
                        sender,
                        Cause::Message(id.clone()),
                    );
                    self.run_frame(env, Call::Execute(&message.payload), depth + 1)
                }
            };
            let refusal = match (ran, message.reply) {
                (Ok(_), false) => continue,
                (Err(refusal), false) => refusal,
                (ran, true) => {
                    let outcome = match ran {
                        Ok(output) => Outcome::Applied { output },
                        Err(refusal) => Outcome::Rejected(refusal),
                    };
                    let replied = match self.account(Query::OfModule(message.target.clone())) {
                        Err(refusal) => Err(refusal),
                        Ok(sender) => {
                            let cause = Cause::Reply { id, outcome };
                            let env = nested(&env.module, &message.target, sender, cause);
                            self.run_frame(env, Call::Execute(&[]), depth + 1)
                        }
                    };
                    match replied {
                        Ok(_) => continue,
                        Err(refusal) => refusal,
                    }
                }
            };
            self.restore(checkpoint);
            return Err(refusal);
        }
        Ok(output)
    }

    fn checkpoint(&self) -> Vec<crate::mock::Written> {
        self.seats
            .values()
            .map(|seat| seat.host.written())
            .collect()
    }

    fn restore(&self, checkpoint: Vec<crate::mock::Written>) {
        for (seat, written) in self.seats.values().zip(checkpoint) {
            seat.host.restore(written);
        }
    }
}

impl Seat {
    /// This module as another one's sibling: asked over its own host.
    fn sibling(&self) -> crate::Sibling {
        let (host, query) = (self.host.clone(), self.query.clone());
        Box::new(move |env, request| {
            query(host.query(env), request)?;
            Ok(host.take_response())
        })
    }
}

#[cfg(test)]
mod tests;
