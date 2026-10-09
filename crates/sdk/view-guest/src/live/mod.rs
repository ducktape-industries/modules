//! A fake node for one view: the real programs on a native chain
//! ([`guest::MockChain`]), run as the kernel runs them, behind every node
//! method the app forwards. [`run`] puts it behind the node's own HTTP
//! surface and opens the view in the real app (`cargo run -p <view>
//! --example live`); [`Network::back`] puts the same chain behind a test's
//! [`FakeHost`], so a test seeds state through real ops instead of
//! hand-writing replies.
//!
//! The chain is founded with the boot set seated (`identity`, `valset`,
//! `module-registry`, the owner's kept system-program exception) and a dev
//! key holding an account through identity's own `Create`; every
//! [`submit`](Network::submit) is one block of the archive, and the keys it
//! wrote are what `module.changes` carries. One entry,
//! [`answer`](Network::answer), serves both fronts at the view-wire level:
//! a typed override first ([`handle`](Network::handle),
//! [`refuse`](Network::refuse), FakeHost's signatures), else the chain.
//!
//! Left as constants, on purpose: one validator (the dev key), epoch 0, a
//! zero state root, no blob put, no program upgrade.

mod node;
mod pack;
mod run;
mod wire;

pub use run::run;

pub use crate::host::Error;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use abi::role::validators::{Genesis as Validators, Member};
use borsh::{BorshDeserialize, BorshSerialize};
use commonware_cryptography::{Signer as _, ed25519};
use guest::abi;
use guest::{AccountNumber, MockChain, MockHost, Module, ModuleId, Origin};
use module_registry::{Entry, Genesis as Roster, View};
use view_wire::methods::{self, Method};

use crate::testing::FakeHost;

/// What the status says a block takes; the app polls heads at half of it.
pub const BLOCK_TIME_MS: u64 = 2000;
/// The chain's name, and its id (its genesis yields no salt).
pub const CHAIN: &str = "live";
/// The seed the dev key is made from; the app's `--live` takes the same.
pub const DEV_SEED: u64 = 1;

type Override = Box<dyn FnMut(&[u8]) -> Result<Vec<u8>, Error>>;
type Writes = Vec<(Vec<u8>, Option<Vec<u8>>)>;

struct State {
    chain: MockChain,
    /// Every seated program and its host, in seating order.
    seats: Vec<(ModuleId, MockHost)>,
    keys: BTreeMap<AccountNumber, ed25519::PrivateKey>,
    names: BTreeMap<String, AccountNumber>,
    me: AccountNumber,
    /// The one validator's key: the dev key, which proposes every block.
    validator: Vec<u8>,
    /// The genesis block's time, unix ms.
    founded: u64,
    /// The next block's time, when pinned by [`Network::at`].
    pinned: Option<u64>,
    /// The archive: the genesis block first, one block per submission.
    blocks: Vec<methods::Block>,
    /// What each submission wrote, by program, until taken.
    changes: Vec<(String, wire::Change)>,
    /// The open `/v1/changes/<program>` sockets.
    subscribers: Vec<(String, std::sync::mpsc::Sender<Vec<u8>>)>,
    seqs: BTreeMap<Vec<u8>, u64>,
    /// Each program's packed code blob (framed) and its id; a program
    /// nothing packed gets a placeholder id the app never fetches.
    codes: BTreeMap<ModuleId, (abi::BlobId, Vec<u8>)>,
    /// A view-only roster entry: its name, and the view blob itself.
    bare: Option<(String, abi::BlobId, Vec<u8>)>,
    overrides: HashMap<(String, Option<String>), Override>,
    invites: u64,
    founded_roster: bool,
}

/// The fake node: shared, single-threaded.
#[derive(Clone)]
pub struct Network(Rc<RefCell<State>>);

impl Default for Network {
    fn default() -> Self {
        Self::new()
    }
}

/// The time now, unix ms.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or_default()
}

fn digest(parts: &[&[u8]]) -> [u8; 32] {
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

fn key_from_seed(seed: u64) -> ed25519::PrivateKey {
    ed25519::PrivateKey::from_seed(seed)
}

impl Network {
    /// A chain with the boot set seated and the dev key holding account
    /// `me`, at its genesis block.
    pub fn new() -> Self {
        let founded = now_ms();
        let genesis = methods::Block {
            height: 1,
            id: digest(&[b"live-block", &1u64.to_le_bytes()]),
            parent: [0; 32],
            time: founded,
            epoch: 0,
            proposer: None,
            txs: Vec::new(),
        };
        let net = Network(Rc::new(RefCell::new(State {
            chain: MockChain::default(),
            seats: Vec::new(),
            keys: BTreeMap::new(),
            names: BTreeMap::new(),
            me: 0,
            validator: key_from_seed(DEV_SEED).public_key().as_ref().to_vec(),
            founded,
            pinned: None,
            blocks: vec![genesis],
            changes: Vec::new(),
            subscribers: Vec::new(),
            seqs: BTreeMap::new(),
            codes: BTreeMap::new(),
            bare: None,
            overrides: HashMap::new(),
            invites: 0,
            founded_roster: false,
        })));
        net.seat::<identity::Identity>(identity::MODULE);
        net.seat::<valset::Valset>(valset::MODULE);
        net.seat::<module_registry::Modules>(module_registry::MODULE);
        let dev = key_from_seed(DEV_SEED);
        net.0
            .borrow()
            .chain
            .init(
                valset::MODULE,
                &Validators {
                    validators: vec![Member {
                        key: dev.public_key().as_ref().to_vec(),
                        address: "127.0.0.1:1".into(),
                    }],
                    member_cap: 8,
                },
            )
            .expect("valset founds with the dev validator");
        let me = net.account(dev, "dev");
        net.0.borrow_mut().me = me;
        // the founding is history, not a change a view hears
        net.take_changes();
        net
    }

    /// Seats `M` as `module` and gives it its account, as admission does;
    /// its host is returned for a seed to read or write directly.
    pub fn seat<M: Module>(&self, module: &str) -> MockHost {
        let mut state = self.0.borrow_mut();
        let host = state.chain.seat::<M>(module);
        state.seats.push((module.to_owned(), host.clone()));
        state
            .chain
            .submit(
                Origin::Root,
                identity::MODULE,
                &identity::Op::RegisterModule {
                    module: module.to_owned(),
                },
            )
            .expect("identity registers a seated module");
        host
    }

    /// Account `name`, made through identity's `Create` on its first
    /// mention, held by a key of its own.
    pub fn person(&self, name: &str) -> AccountNumber {
        if let Some(number) = self.0.borrow().names.get(name) {
            return *number;
        }
        let seed = u64::from_le_bytes(
            digest(&[b"live-person", name.as_bytes()])[..8]
                .try_into()
                .unwrap(),
        );
        self.account(key_from_seed(seed), name)
    }

    fn account(&self, key: ed25519::PrivateKey, name: &str) -> AccountNumber {
        let receipt = self.run(
            Origin::Signed(key.public_key().as_ref().to_vec()),
            identity::MODULE.to_owned(),
            methods::encode(&identity::Op::Create {
                name: name.to_owned(),
                scheme: abi::Scheme::Ed25519,
            }),
        );
        let methods::Outcome::Applied { output } = receipt.outcome else {
            panic!("identity refuses the account {name}: {:?}", receipt.outcome);
        };
        let number: AccountNumber = methods::decode(&output).expect("an account number");
        let mut state = self.0.borrow_mut();
        state.keys.insert(number, key);
        state.names.insert(name.to_owned(), number);
        number
    }

    /// The dev account: the one the app signs in as.
    pub fn me(&self) -> AccountNumber {
        self.0.borrow().me
    }

    /// The public key `who` signs with.
    pub fn key_of(&self, who: AccountNumber) -> Vec<u8> {
        self.0
            .borrow()
            .keys
            .get(&who)
            .unwrap_or_else(|| panic!("account {who} holds no key here"))
            .public_key()
            .as_ref()
            .to_vec()
    }

    /// Pins the next block's time (unix ms); unpinned, a block is at the
    /// later of now and the block before.
    pub fn at(&self, time_ms: u64) {
        self.0.borrow_mut().pinned = Some(time_ms);
    }

    /// `op` to `program`, signed by `who`, as one block: the program's
    /// output, or the refusal that rejected it.
    pub fn submit<O: BorshSerialize>(
        &self,
        who: AccountNumber,
        program: &str,
        op: &O,
    ) -> Result<Vec<u8>, Error> {
        self.applied(self.run(
            Origin::Signed(self.key_of(who)),
            program.to_owned(),
            methods::encode(op),
        ))
    }

    /// `op` to `program` from the system itself (`Origin::Root`), as one
    /// block: what genesis and the registry role do (a valset membership,
    /// a scheduled change).
    pub fn system<O: BorshSerialize>(&self, program: &str, op: &O) -> Result<Vec<u8>, Error> {
        self.applied(self.run(Origin::Root, program.to_owned(), methods::encode(op)))
    }

    /// `program`'s `init` with `params`, as admission runs it (no block).
    pub fn init<P: BorshSerialize>(&self, program: &str, params: &P) -> Result<(), Error> {
        self.0.borrow().chain.init(program, params)
    }

    fn applied(&self, receipt: methods::Receipt) -> Result<Vec<u8>, Error> {
        match receipt.outcome {
            methods::Outcome::Applied { output } => Ok(output),
            methods::Outcome::Rejected(refusal) => Err(refusal),
        }
    }

    /// `program`'s answer to `request`, borsh both ways.
    pub fn query<Q: BorshSerialize, R: BorshDeserialize>(
        &self,
        program: &str,
        request: &Q,
    ) -> Result<R, Error> {
        self.0.borrow().chain.query(program, request)
    }

    /// The host `program` is seated over.
    pub fn host(&self, program: &str) -> MockHost {
        self.0
            .borrow()
            .seats
            .iter()
            .find(|(name, _)| name == program)
            .map(|(_, host)| host.clone())
            .unwrap_or_else(|| panic!("{program} is not seated"))
    }

    /// Every seated program, in seating order.
    pub fn seats(&self) -> Vec<ModuleId> {
        self.0
            .borrow()
            .seats
            .iter()
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Answers each ask of `C` with what `handler` returns, in place of
    /// the chain.
    pub fn handle<C: Method>(
        &self,
        mut handler: impl FnMut(C::Request) -> Result<C::Reply, Error> + 'static,
    ) {
        self.0.borrow_mut().overrides.insert(
            (C::KIND.to_owned(), C::TARGET.map(str::to_owned)),
            Box::new(move |payload| {
                let request = C::decode_request(payload).map_err(crate::host::malformed)?;
                handler(request).map(|reply| C::encode_reply(&reply))
            }),
        );
    }

    /// Refuses every ask of `C`.
    pub fn refuse<C: Method>(&self, code: &str, message: &str) {
        let refusal = Error::new(code, message);
        self.0.borrow_mut().overrides.insert(
            (C::KIND.to_owned(), C::TARGET.map(str::to_owned)),
            Box::new(move |_| Err(refusal.clone())),
        );
    }

    /// The session the app hands the view: signed in as `me`.
    pub fn session(&self) -> methods::Session {
        let me = self.me();
        methods::Session {
            connected: true,
            chain_id: CHAIN.into(),
            signer: abi::hex(&self.key_of(me)),
            account: Some(me),
            endpoint: "http://127.0.0.1:0".into(),
        }
    }

    /// The chain behind `host`: every node method a view asks that the
    /// test did not answer itself is answered here, and each submission's
    /// writes reach the view's `module.changes` subscriptions after the
    /// frame. A typed `handle` on `host` for one program still wins.
    pub fn back(&self, host: &FakeHost) {
        self.found_roster();
        // what was seeded before the view opened is what it reads first,
        // not a change
        self.take_changes();
        for kind in [
            "module.query",
            "op.submit",
            "chain.status",
            "chain.network",
            "chain.blocks",
            "chain.block",
            "blob.get",
            "invite.create",
            "module.describe",
        ] {
            let net = self.clone();
            host.answer_with(kind, None, move |payload| {
                net.answer(kind, target_of(kind, payload).as_deref(), payload)
            });
        }
        let net = self.clone();
        host.after_frame(move |host| {
            for (program, change) in net.take_changes() {
                let keys = change.writes.into_iter().map(|(key, _)| key).collect();
                let item = Some(methods::Change {
                    height: change.height,
                    keys,
                });
                host.send_raw("module.changes", Some(&program), methods::encode(&item));
            }
        });
    }

    /// One node method at the view-wire level: an override, else the chain.
    /// A submission runs as the dev account, the one a test's session
    /// signs in as.
    pub fn answer(
        &self,
        kind: &str,
        target: Option<&str>,
        payload: &[u8],
    ) -> Result<Vec<u8>, Error> {
        self.answer_as(self.key_of(self.me()), kind, target, payload)
    }

    /// [`answer`](Self::answer), asked by `signer`: the key a submission
    /// runs under. Every node method either front serves comes through
    /// here, so an override answers the live window as it answers a test.
    pub(crate) fn answer_as(
        &self,
        signer: Vec<u8>,
        kind: &str,
        target: Option<&str>,
        payload: &[u8],
    ) -> Result<Vec<u8>, Error> {
        let exact = (kind.to_owned(), target.map(str::to_owned));
        let any = (kind.to_owned(), None);
        let mut state = self.0.borrow_mut();
        let key = [exact, any]
            .into_iter()
            .find(|key| state.overrides.contains_key(key));
        if let Some(handler) = key.and_then(|key| state.overrides.get_mut(&key)) {
            return handler(payload);
        }
        drop(state);
        let malformed = |error: &str| crate::host::malformed(error.to_owned());
        match kind {
            "module.query" => {
                let call: methods::Call =
                    methods::decode(payload).map_err(|error| malformed(&error))?;
                self.0.borrow().chain.query_bytes(&call.target, &call.body)
            }
            "op.submit" => {
                let call: methods::Call =
                    methods::decode(payload).map_err(|error| malformed(&error))?;
                self.applied(self.run(Origin::Signed(signer), call.target, call.body))
            }
            "chain.status" => Ok(methods::encode(&self.status())),
            "chain.network" => {
                let members = match self.query(valset::MODULE, &valset::Query::Members)? {
                    valset::Reply::Members(members) => members,
                    other => {
                        return Err(guest::unexpected_reply(valset::MODULE, "Members", &other));
                    }
                };
                let height = self.height();
                Ok(methods::encode(&methods::NetworkStatus {
                    height,
                    members: members
                        .into_iter()
                        .map(|member| methods::Peer {
                            key: member.key,
                            signed: Some(height),
                        })
                        .collect(),
                }))
            }
            "chain.blocks" => {
                let page: methods::BlockPage =
                    methods::decode(payload).map_err(|error| malformed(&error))?;
                let state = self.0.borrow();
                let before = page.before.unwrap_or(u64::MAX);
                let blocks: Vec<&methods::Block> = state
                    .blocks
                    .iter()
                    .rev()
                    .filter(|block| block.height < before)
                    .take(page.limit.min(100) as usize)
                    .collect();
                Ok(methods::encode(&blocks))
            }
            "chain.block" => {
                let by: methods::BlockRef =
                    methods::decode(payload).map_err(|error| malformed(&error))?;
                let state = self.0.borrow();
                let block = state.blocks.iter().find(|block| match &by {
                    methods::BlockRef::Height(height) => block.height == *height,
                    methods::BlockRef::Id(id) => block.id == *id,
                });
                Ok(methods::encode(&block))
            }
            "blob.get" => {
                let id: String = methods::decode(payload).map_err(|error| malformed(&error))?;
                let (kind, hex) = id
                    .split_once(':')
                    .ok_or_else(|| malformed("id is `sha256:<hex>` or `sha1:<hex>`"))?;
                let digest = hex_decode(hex).map_err(|error| malformed(&error))?;
                let id = match (kind, digest.len()) {
                    ("sha256", 32) => abi::BlobId::Sha256(digest.try_into().unwrap()),
                    ("sha1", 20) => abi::BlobId::Sha1(digest.try_into().unwrap()),
                    _ => return Err(malformed("id is `sha256:<hex>` or `sha1:<hex>`")),
                };
                let body = self
                    .blob(&id)
                    .map(|framed| unframe(&framed).unwrap_or_default().to_vec());
                Ok(methods::encode(&body))
            }
            "invite.create" => {
                let invite: methods::CreateInvite =
                    methods::decode(payload).map_err(|error| malformed(&error))?;
                if invite.ttl_days == 0 {
                    return Err(malformed("ttl_days must be a positive integer"));
                }
                let mut state = self.0.borrow_mut();
                state.invites += 1;
                Ok(methods::encode(&methods::Invite {
                    invite: format!("live-invite-{}", state.invites),
                    notes: Vec::new(),
                }))
            }
            // the app answers it from the program's own describe module; a
            // test that wants one answers it itself
            "module.describe" => Ok(methods::encode(&None::<methods::Description>)),
            other => Err(Error::new(
                methods::refusal::UNKNOWN_REQUEST,
                format!("the fake node does not answer `{other}`"),
            )),
        }
    }

    /// `chain.status` as the archive stands.
    pub fn status(&self) -> methods::NodeStatus {
        let state = self.0.borrow();
        let tip = state.blocks.last().expect("the genesis block");
        methods::NodeStatus {
            chain_id: CHAIN.into(),
            time: state.founded,
            block_time_ms: BLOCK_TIME_MS,
            epoch_length: 100,
            height: tip.height,
            tip: tip.id,
            root: [0; 32],
            epoch: 0,
            // this node is the one validator, as a real node names its own key
            identity: state.validator.clone(),
            contract: wire::NODE_CONTRACT,
        }
    }

    pub fn height(&self) -> u64 {
        self.0.borrow().blocks.len() as u64
    }

    /// The genesis block's id.
    pub(crate) fn genesis(&self) -> [u8; 32] {
        self.0.borrow().blocks[0].id
    }

    /// The sequence the node expects next from `signer`.
    pub(crate) fn seq(&self, signer: &[u8]) -> u64 {
        self.0.borrow().seqs.get(signer).copied().unwrap_or(0)
    }

    /// `program`'s value under `key`, as `/v1/get` reads it; `$signers`
    /// holds each signer's next sequence.
    pub(crate) fn get(&self, program: &str, key: &[u8]) -> Option<Vec<u8>> {
        if program == "$signers" {
            return Some(methods::encode(&self.seq(key)));
        }
        let state = self.0.borrow();
        let (_, host) = state.seats.iter().find(|(name, _)| name == program)?;
        host.borrow().state.get(key).cloned()
    }

    /// A blob by id, framed: a program's packed code, or one a program put.
    pub(crate) fn blob(&self, id: &abi::BlobId) -> Option<Vec<u8>> {
        let state = self.0.borrow();
        if let Some((_, framed)) = state.codes.values().find(|(code, _)| code == id) {
            return Some(framed.clone());
        }
        if let Some((_, bare, framed)) = &state.bare
            && bare == id
        {
            return Some(framed.clone());
        }
        state.seats.iter().find_map(|(_, host)| {
            let host = host.borrow();
            let blob = host.blobs.get(id)?;
            Some(wire::framed(&blob.kind, &blob.body))
        })
    }

    /// `program`'s code blob is `body`: the roster lists it by the blob's
    /// id, and `/v1/blob/get` serves it.
    pub(crate) fn code(&self, program: &str, body: &[u8]) {
        let framed = wire::framed(module_registry::CODE_KIND, body);
        let id = wire::blob_id(&framed);
        self.0
            .borrow_mut()
            .codes
            .insert(program.to_owned(), (id, framed));
    }

    /// `name` is a view-only entry whose view is `body`.
    pub(crate) fn code_bare(&self, name: &str, body: &[u8]) {
        let framed = wire::framed("view", body);
        let id = wire::blob_id(&framed);
        self.0.borrow_mut().bare = Some((name.to_owned(), id, framed));
    }

    /// Founds the registry with every seated program and the view-only
    /// entry, once: what the app reads as the roster.
    pub(crate) fn found_roster(&self) {
        let mut state = self.0.borrow_mut();
        if std::mem::replace(&mut state.founded_roster, true) {
            return;
        }
        let programs = state
            .seats
            .iter()
            .map(|(name, _)| Entry {
                program: name.clone(),
                code: state
                    .codes
                    .get(name)
                    .map(|(id, _)| *id)
                    .unwrap_or(abi::BlobId::Sha256(digest(&[
                        b"live-code",
                        name.as_bytes(),
                    ]))),
                params: Vec::new(),
            })
            .collect();
        let views = state
            .bare
            .iter()
            .map(|(name, id, _)| View {
                name: name.clone(),
                view: *id,
            })
            .collect();
        state
            .chain
            .init(module_registry::MODULE, &Roster { programs, views })
            .expect("the registry founds with the seated programs");
    }

    /// A socket on `module.changes` of `program`: each block that writes
    /// to it is sent, encoded, until the receiver is gone.
    pub(crate) fn subscribe(&self, program: &str, sender: std::sync::mpsc::Sender<Vec<u8>>) {
        self.0
            .borrow_mut()
            .subscribers
            .push((program.to_owned(), sender));
    }

    /// What was written since the last take, by program.
    pub(crate) fn take_changes(&self) -> Vec<(String, wire::Change)> {
        std::mem::take(&mut self.0.borrow_mut().changes)
    }

    /// One submission as one block: `payload` to `target`, signed by
    /// `signer` (an account's key, or none: the program hears nobody), at
    /// the next height; the block archived with the tx and its receipt
    /// whether the program applied or refused it, the signer's sequence
    /// moved, and what the run wrote recorded as the block's changes.
    pub(crate) fn run(&self, origin: Origin, target: String, payload: Vec<u8>) -> methods::Receipt {
        let signer = match &origin {
            Origin::Signed(key) => key.clone(),
            Origin::Module(_) | Origin::Root => Vec::new(),
        };
        let mut state = self.0.borrow_mut();
        let height = state.blocks.len() as u64 + 1;
        let last = state
            .blocks
            .last()
            .map_or(state.founded, |block| block.time);
        let time = state
            .pinned
            .take()
            .unwrap_or_else(|| now_ms().max(last + 1));
        state.chain.at(height, time);
        let before: Vec<BTreeMap<Vec<u8>, Vec<u8>>> = state
            .seats
            .iter()
            .map(|(_, host)| host.borrow().state.clone())
            .collect();
        let ran = state.chain.submit_raw(origin, target.clone(), &payload);
        let events = state
            .seats
            .iter()
            .find(|(name, _)| *name == target)
            .map(|(_, host)| std::mem::take(&mut host.borrow_mut().events))
            .unwrap_or_default();
        let outcome = match ran {
            Ok(output) => methods::Outcome::Applied { output },
            Err(refusal) => methods::Outcome::Rejected(refusal),
        };
        let seq = state.seqs.entry(signer.clone()).or_default();
        let tx = methods::Tx {
            hash: digest(&[b"live-tx", &height.to_le_bytes()]),
            signer: signer.clone(),
            seq: *seq,
            target: target.clone(),
            payload,
            receipt: Some(methods::Receipt {
                program: target,
                outcome: outcome.clone(),
                events: events.clone(),
                nested: Vec::new(),
            }),
        };
        *seq += 1;
        let parent = state.blocks.last().map_or([0; 32], |block| block.id);
        let proposer = state.validator.clone();
        state.blocks.push(methods::Block {
            height,
            id: digest(&[b"live-block", &height.to_le_bytes()]),
            parent,
            time,
            epoch: 0,
            proposer: Some(proposer),
            txs: vec![tx.clone()],
        });
        let mut changed = Vec::new();
        for ((name, host), before) in state.seats.iter().zip(before) {
            let after = &host.borrow().state;
            let writes: Writes = before
                .keys()
                .chain(after.keys())
                .filter(|key| before.get(*key) != after.get(*key))
                .map(|key| (key.clone(), after.get(key).cloned()))
                .collect();
            if !writes.is_empty() {
                changed.push((
                    name.clone(),
                    wire::Change {
                        height,
                        root: abi::Root([0; 32]),
                        writes,
                    },
                ));
            }
        }
        state.changes.extend(changed);
        tx.receipt.expect("the receipt just made")
    }

    /// Every change since the last flush to the sockets following its
    /// program, and gone. A node preconfirms a submission before the
    /// block that carries it lands, so the front flushes only after the
    /// answer that caused a change is on its way.
    pub(crate) fn flush(&self) {
        let mut state = self.0.borrow_mut();
        for (program, change) in std::mem::take(&mut state.changes) {
            let item = abi::encode(&change);
            state
                .subscribers
                .retain(|(name, sender)| *name != program || sender.send(item.clone()).is_ok());
        }
    }
}

/// The program a node method addresses, read off its payload as the
/// app reads it.
fn target_of(kind: &str, payload: &[u8]) -> Option<String> {
    match kind {
        "module.query" | "op.submit" => methods::decode::<methods::Call>(payload)
            .ok()
            .map(|call| call.target),
        _ => None,
    }
}

/// `kind len\0body` → body.
pub(crate) fn unframe(framed: &[u8]) -> Option<&[u8]> {
    let nul = framed.iter().position(|byte| *byte == 0)?;
    Some(&framed[nul + 1..])
}

fn hex_decode(value: &str) -> Result<Vec<u8>, String> {
    if !value.bytes().all(|byte| byte.is_ascii_hexdigit()) || !value.len().is_multiple_of(2) {
        return Err("not hex".into());
    }
    (0..value.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&value[at..at + 2], 16).map_err(|error| error.to_string()))
        .collect()
}

#[cfg(test)]
mod tests;
