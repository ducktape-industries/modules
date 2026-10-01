//! The host a native test runs a module over: maps for state and blobs, and
//! what the module sent (`output`, `response`, `emissions`, `events`) kept
//! for the test to read. Sibling modules answer through `siblings`
//! ([`MockHost::sibling`] seats a module there); signatures verify through
//! `verifier` (none set: verification is refused). A `MockHost` is a shared
//! handle: the contexts made over it and the test see one host.
//! [`MockHost::env`] is the env a test starts from.

use std::cell::{Ref, RefCell, RefMut};
use std::collections::BTreeMap;
use std::rc::Rc;

use abi::role::identity::Profile;
use abi::{
    Blob, BlobHeader, BlobId, CryptoOp, CryptoReply, Entry, HashKind, HostOp, HostReply, Message,
    Scheme,
};
use sha1::Digest as _;

use crate::{
    AccountNumber, Cause, Env, Error, ExecCtx, Module, ModuleId, Order, Origin, Principal,
    QueryCtx, Range, Roles, code,
};

/// A module another one queries: the env the host hands the answering
/// module (the asker's chain, height, time and roles; `module` the one
/// asked, `origin` the asker, no sender) and the request's bytes.
pub type Sibling = Box<dyn Fn(Env, &[u8]) -> Result<Vec<u8>, Error>>;

/// The identity role over a fixed roster, for a native test of a module
/// that names accounts: `Profile` and `Profiles` (paged by number) out of
/// `profiles`, in any order given. `Account` and `OfModule` are the
/// kernel's to ask, so a module asking them is refused, as a sibling
/// answers only what its role says.
pub fn identity_role(profiles: &[Profile], request: &[u8]) -> Result<Vec<u8>, Error> {
    use abi::role::identity::{Query, Reply};
    let reply = match abi::decode(request).map_err(crate::kernel::error_from)? {
        Query::Profile(number) => {
            Reply::Profile(profiles.iter().find(|p| p.number == number).cloned())
        }
        Query::Profiles { after, limit } => {
            let mut page: Vec<Profile> = profiles
                .iter()
                .filter(|p| after.is_none_or(|after| p.number > after))
                .cloned()
                .collect();
            page.sort_by_key(|p| p.number);
            let limit = limit.max(1) as usize;
            let more = page.len() > limit;
            page.truncate(limit);
            let next = page.last().filter(|_| more).map(|p| p.number);
            Reply::Profiles {
                profiles: page,
                next,
            }
        }
        Query::Account(_) | Query::OfModule(_) => {
            return Err(Error::new(
                code::UNSUPPORTED,
                "the kernel alone asks identity who holds a key or a module",
            ));
        }
    };
    Ok(abi::encode(&reply))
}
pub type Verifier = Box<dyn Fn(Scheme, &[u8], &[u8], &[u8], &[u8]) -> bool>;

#[derive(Default)]
pub struct MockState {
    pub state: BTreeMap<Vec<u8>, Vec<u8>>,
    pub blobs: BTreeMap<BlobId, Blob>,
    /// The last `set_return_data`.
    pub output: Vec<u8>,
    /// The last query's response.
    pub response: Vec<u8>,
    pub emissions: Vec<Message>,
    /// The number the next emit gets: 0 after a [`MockHost::take_emissions`];
    /// a [`MockChain`](crate::MockChain) carries it across a frame.
    pub next_message: u64,
    pub events: Vec<Vec<u8>>,
    pub siblings: BTreeMap<ModuleId, Sibling>,
    pub verifier: Option<Verifier>,
}

#[derive(Clone, Default)]
pub struct MockHost(Rc<RefCell<MockState>>);

/// What a write leaves on a host: its state, its blobs and what it sent
/// (output, emissions, events).
pub(crate) struct Written {
    state: BTreeMap<Vec<u8>, Vec<u8>>,
    blobs: BTreeMap<BlobId, Blob>,
    sent: (Vec<u8>, Vec<Message>, Vec<Vec<u8>>),
}

impl MockHost {
    /// The roles as the suite's genesis binds them, for a native test's
    /// env: a sibling registered under `identity` answers the identity role.
    pub fn roles() -> Roles {
        Roles {
            registry: "module-registry".into(),
            validators: "valset".into(),
            identity: "identity".into(),
        }
    }

    /// A direct call to `module` by the chain itself: chain `net`, height
    /// 1, time 0, the suite's [`roles`](MockHost::roles). Change what a
    /// test cares about with [`Env::signed`], [`Env::from_module`] or
    /// struct update: `Env { height: 7, ..MockHost::env("chat") }`.
    pub fn env(module: impl Into<ModuleId>) -> Env {
        Env {
            chain_id: b"net".to_vec(),
            height: 1,
            time: 0,
            module: module.into(),
            origin: Origin::Root,
            sender: Some(Principal::Root),
            roles: MockHost::roles(),
            cause: Cause::Direct,
        }
    }

    /// Seats the identity role over a fixed roster ([`identity_role`]), as
    /// the sibling at [`roles`](MockHost::roles)`().identity`.
    pub fn identity(&self, profiles: Vec<Profile>) {
        let answer: Sibling = Box::new(move |_, request| identity_role(&profiles, request));
        self.borrow_mut()
            .siblings
            .insert(MockHost::roles().identity, answer);
    }

    /// Seats `M`, over `host` (its own state), as the module `module`
    /// another one queries here: the request goes through `M`'s own
    /// decoding and the answer through its encoding, as on the host.
    pub fn sibling<M: Module>(&self, module: impl Into<ModuleId>, host: &MockHost) {
        let host = host.clone();
        let answer: Sibling = Box::new(move |env, request| {
            crate::query::<M>(&host.query(env), request)?;
            Ok(host.take_response())
        });
        self.borrow_mut().siblings.insert(module.into(), answer);
    }

    /// An execute's context over this host.
    pub fn exec(&self, env: Env) -> ExecCtx {
        ExecCtx::over(self.clone(), env)
    }

    /// A query's context over this host.
    pub fn query(&self, env: Env) -> QueryCtx {
        QueryCtx::over(self.clone(), env)
    }

    pub fn borrow(&self) -> Ref<'_, MockState> {
        self.0.borrow()
    }

    pub fn borrow_mut(&self) -> RefMut<'_, MockState> {
        self.0.borrow_mut()
    }

    pub fn take_output(&self) -> Vec<u8> {
        std::mem::take(&mut self.borrow_mut().output)
    }

    pub fn take_emissions(&self) -> Vec<Message> {
        let mut mock = self.borrow_mut();
        mock.next_message = 0;
        std::mem::take(&mut mock.emissions)
    }

    /// The last query's response, as [`crate::query`] handed it over.
    pub fn take_response(&self) -> Vec<u8> {
        std::mem::take(&mut self.borrow_mut().response)
    }

    /// Runs `write` and, when it refuses, checks it left this host as it
    /// found it: no state, blob, emission, event or output. The attack
    /// check every module's harness shares: a rule checks before it
    /// writes, so a refusal needs no rollback.
    #[track_caller]
    pub fn attempt<T>(&self, write: impl FnOnce() -> Result<T, Error>) -> Result<T, Error> {
        let before = self.written();
        let result = write();
        if result.is_err() {
            let after = self.written();
            assert_eq!(after.state, before.state, "a refused write changed state");
            assert_eq!(after.blobs, before.blobs, "a refused write stored a blob");
            assert_eq!(after.sent, before.sent, "a refused write sent something");
        }
        result
    }

    /// [`MockHost::attempt`] a write that must refuse: its refusal.
    #[track_caller]
    pub fn refused<T: std::fmt::Debug>(&self, write: impl FnOnce() -> Result<T, Error>) -> Error {
        self.attempt(write).expect_err("the write was refused")
    }

    /// Everything a write leaves on this host, copied.
    pub(crate) fn written(&self) -> Written {
        let mock = self.borrow();
        Written {
            state: mock.state.clone(),
            blobs: mock.blobs.clone(),
            sent: (
                mock.output.clone(),
                mock.emissions.clone(),
                mock.events.clone(),
            ),
        }
    }

    /// Puts back what [`written`](MockHost::written) copied: the undo.
    pub(crate) fn restore(&self, written: Written) {
        let mut mock = self.borrow_mut();
        mock.state = written.state;
        mock.blobs = written.blobs;
        (mock.output, mock.emissions, mock.events) = written.sent;
    }

    /// One host call from a context whose env is `env`, as the real host
    /// answers it. An emitted message is kept, numbered from 0, for the
    /// test to run.
    pub(crate) fn serve(&self, env: &Env, op: HostOp) -> HostReply {
        if let HostOp::Query { program, request } = op {
            // The sibling leaves the map while it answers, so it may use
            // this host itself.
            let sibling = self.borrow_mut().siblings.remove(&program);
            return HostReply::Query(match sibling {
                Some(sibling) => {
                    let asked = Env {
                        module: program.clone(),
                        origin: Origin::Module(env.module.clone()),
                        sender: None,
                        cause: Cause::Direct,
                        ..env.clone()
                    };
                    let answer = sibling(asked, &request).map_err(crate::kernel::refusal_from);
                    self.borrow_mut().siblings.insert(program, sibling);
                    answer
                }
                None => Err(abi::Refusal::new(code::UNKNOWN_MODULE, program)),
            });
        }
        let mut mock = self.borrow_mut();
        match op {
            HostOp::Get(key) | HostOp::CommittedGet(key) => {
                HostReply::Value(mock.state.get(&key).cloned())
            }
            HostOp::Scan(scan) | HostOp::CommittedScan(scan) => {
                HostReply::Entries(mock.scan_state(&scan.into()))
            }
            HostOp::BlobGet(id) => HostReply::Blob(mock.blobs.get(&id).cloned()),
            HostOp::BlobStat(id) => {
                HostReply::BlobHeader(mock.blobs.get(&id).map(|blob| BlobHeader {
                    kind: blob.kind.clone(),
                    len: blob.body.len() as u64,
                }))
            }
            HostOp::BlobRead { id, offset, len } => {
                HostReply::Value(mock.blobs.get(&id).map(|b| {
                    let start = (offset as usize).min(b.body.len());
                    let end = start.saturating_add(len as usize).min(b.body.len());
                    b.body[start..end].to_vec()
                }))
            }
            HostOp::Root(_) => HostReply::Root(None),
            HostOp::Crypto(CryptoOp::Sha256(bytes)) => {
                HostReply::Crypto(CryptoReply::Digest(sha2::Sha256::digest(bytes).into()))
            }
            HostOp::Crypto(CryptoOp::Verify {
                scheme,
                key,
                namespace,
                message,
                signature,
            }) => match &mock.verifier {
                Some(verify) => HostReply::Crypto(CryptoReply::Verified(verify(
                    scheme, &key, &namespace, &message, &signature,
                ))),
                None => HostReply::Refused(abi::Refusal::new(
                    code::UNSUPPORTED,
                    "MockHost verifies nothing until a verifier is set",
                )),
            },
            HostOp::Set { key, value } => {
                mock.state.insert(key, value);
                HostReply::Done
            }
            HostOp::Delete(key) => {
                mock.state.remove(&key);
                HostReply::Done
            }
            HostOp::BlobPut { hash, kind, body } => match blob_id(hash, &kind, &body) {
                Ok(id) => {
                    mock.blobs.insert(id, Blob { kind, body });
                    HostReply::BlobId(id)
                }
                Err(error) => HostReply::Refused(crate::kernel::refusal_from(error)),
            },
            HostOp::Emit(message) => {
                let item = mock.next_message;
                mock.next_message += 1;
                mock.emissions.push(message);
                HostReply::Item(abi::ItemRef {
                    source: env.module.clone(),
                    item,
                })
            }
            HostOp::Event(payload) => {
                mock.events.push(payload);
                HostReply::Done
            }
            HostOp::Output(bytes) => {
                mock.output = bytes;
                HostReply::Done
            }
            HostOp::Respond(bytes) => {
                mock.response = bytes;
                HostReply::Done
            }
            HostOp::Query { .. } => unreachable!("answered above"),
        }
    }
}

impl MockState {
    fn scan_state(&self, range: &Range) -> Vec<Entry> {
        let admitted =
            self.state
                .iter()
                .filter(|(key, _)| range.admits(key))
                .map(|(key, value)| Entry {
                    key: key.clone(),
                    value: value.clone(),
                });
        let ordered: Vec<Entry> = if range.order == Order::Descending {
            admitted.rev().collect()
        } else {
            admitted.collect()
        };
        match range.limit {
            Some(limit) => ordered.into_iter().take(limit as usize).collect(),
            None => ordered,
        }
    }
}

/// The origins a test sends from, each with the sender the host would
/// resolve for it (an account, or `None` when the key or module holds none).
impl Env {
    /// Signed by `key`, acting as `account`: the account identity says the
    /// key holds, or `None` for a key that holds none (a write refuses it).
    pub fn signed(self, key: impl Into<Vec<u8>>, account: Option<AccountNumber>) -> Env {
        Env {
            origin: Origin::Signed(key.into()),
            sender: account.map(Principal::Account),
            ..self
        }
    }

    /// Sent by `module` (a message it emitted), acting as `account`, the
    /// account identity registered for it.
    pub fn from_module(self, module: impl Into<ModuleId>, account: Option<AccountNumber>) -> Env {
        Env {
            origin: Origin::Module(module.into()),
            sender: account.map(Principal::Account),
            ..self
        }
    }
}

/// The host's framing: `<kind> <len>\0<body>`, hashed whole.
pub fn blob_id(hash: HashKind, kind: &str, body: &[u8]) -> Result<BlobId, Error> {
    let kind_is_a_word = !kind.is_empty() && !kind.contains([' ', '\0']);
    if !kind_is_a_word {
        return Err(Error::new(
            code::INVALID_INPUT,
            "a blob kind is one non-empty word without spaces or NUL",
        ));
    }
    let mut framed = format!("{kind} {}\0", body.len()).into_bytes();
    framed.extend_from_slice(body);
    Ok(match hash {
        HashKind::Sha256 => BlobId::Sha256(sha2::Sha256::digest(&framed).into()),
        HashKind::Sha1 => BlobId::Sha1(sha1::Sha1::digest(&framed).into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Program;

    /// Answers who asked it, and at what height.
    struct Echo;

    impl Program for Echo {
        const NAME: &'static str = "echo";
        type Op = ();
        type Query = ();
        type Reply = (Origin, u64);
    }

    impl Module for Echo {
        fn execute(_: &ExecCtx, (): ()) -> Result<(), Error> {
            Ok(())
        }

        fn query(ctx: &QueryCtx, (): ()) -> Result<(Origin, u64), Error> {
            Ok((ctx.env().origin.clone(), ctx.env().height))
        }
    }

    #[test]
    fn a_sibling_answers_as_the_host_asks_it() {
        let (asker, echo) = (MockHost::default(), MockHost::default());
        asker.sibling::<Echo>("echo", &echo);
        let ctx = asker.query(Env {
            height: 5,
            ..MockHost::env("chat")
        });
        let answer: (Origin, u64) = ctx.query("echo", &()).unwrap();
        assert_eq!(answer, (Origin::Module("chat".into()), 5));
        let unknown = ctx.query_raw("nobody", Vec::new()).unwrap_err();
        assert_eq!(unknown.code, code::UNKNOWN_MODULE);
    }
}
