//! A node the explorer reads, played by the fake host: a short chain, three
//! accounts, one validator and `programs`, each answered as the program
//! would. Tests by what they cover: `access` (what a reader changes on the
//! Programs page), `describe`, `window`, `index` (the transaction lists),
//! `pages`, `search`.
mod access;
mod describe;
mod follow;
mod index;
mod pages;
mod search;
mod window;

pub(crate) use std::cell::RefCell;
pub(crate) use std::rc::Rc;

pub(crate) use crate::*;
pub(crate) use abi::BlobId;
pub(crate) use activity::Activity;
pub(crate) use identity::Identity;
pub(crate) use programs::Programs;
pub(crate) use valset::Valset;
pub(crate) use view::host::Error;
pub(crate) use view::methods::{
    Block, BlockList, BlockPage, BlockRef, ChainBlock, ChainBlocks, ChainHeads, ChainStatus,
    ChainTx, Changes, ClipboardWrite, Description, Head, HostRoute, HostSession, Located,
    NodeStatus, Outcome, ProgramDescribe, Query, Receipt, Session, Tip, Tx, TxAsk, TxRef, Value,
};
pub(crate) use view::testing::TestAppContext;

pub(crate) const ADA: [u8; 32] = [1; 32];
pub(crate) const STRANGER: [u8; 32] = [2; 32];
pub(crate) const VALIDATOR: [u8; 32] = [9; 32];
/// Block times, milliseconds: block `h` lands at `T0 + h` seconds.
pub(crate) const T0: u64 = 1_790_000_000_000;

pub(crate) fn post(channel: &str, text: &str) -> Vec<u8> {
    borsh::to_vec(&chat::Op::PostMessage {
        channel_id: channel.into(),
        message_id: "m1".into(),
        blocks: chat::parse_message(text),
        thread: None,
    })
    .unwrap()
}

pub(crate) fn tx(seed: u8, signer: [u8; 32], target: &str, payload: Vec<u8>) -> Tx {
    Tx {
        hash: [seed; 32],
        signer: signer.to_vec(),
        anchor: Tip {
            height: 10,
            id: [10; 32],
        },
        target: target.into(),
        size: payload.len() as u32,
        payload: Some(payload),
        receipt: None,
    }
}

pub(crate) fn receipt(
    program: &str,
    refused: Option<(&str, &str)>,
    nested: Vec<Receipt>,
) -> Receipt {
    let outcome = match refused {
        None => Outcome::Applied { output: Vec::new() },
        Some((code, message)) => Outcome::Rejected(Error {
            code: code.into(),
            message: message.into(),
        }),
    };
    Receipt {
        program: program.into(),
        outcome,
        events: Vec::new(),
        nested,
    }
}

/// Blocks 0..=`tip`: 11 carries Ada's post and their DM to account 7, 12 a
/// stranger's op to a program that describes nothing.
pub(crate) fn chain(tip: u64) -> Vec<Block> {
    (0..=tip)
        .map(|height| Block {
            height,
            id: [(height as u8).wrapping_add(100); 32],
            parent: [(height as u8).wrapping_add(99); 32],
            time: T0 + height * 1000,
            epoch: height / 10,
            proposer: Some(VALIDATOR.to_vec()),
            txs: match height {
                // Ada's post applied and emitted a message whose own
                // message was refused; the stranger's op was rejected; the
                // DM ran where the node kept no receipt
                11 => vec![
                    Tx {
                        receipt: Some(receipt(
                            "chat",
                            None,
                            vec![receipt(
                                "notify",
                                None,
                                vec![receipt(
                                    "mail",
                                    Some(("capacity", "the inbox is full")),
                                    Vec::new(),
                                )],
                            )],
                        )),
                        ..tx(0xa1, ADA, "chat", post("design", "hello there"))
                    },
                    tx(0xc3, ADA, "chat", post(&chat::dm_channel_id(7, 3), "ping")),
                ],
                12 => vec![Tx {
                    receipt: Some(receipt(
                        "mystery",
                        Some(("unauthorized", "not a member")),
                        Vec::new(),
                    )),
                    ..tx(0xb2, STRANGER, "mystery", vec![1, 2, 3, 4])
                }],
                _ => Vec::new(),
            },
        })
        .collect()
}

pub(crate) const LISTED_BYTES: u32 = 4 << 10;

/// The most blocks the node answers a page (`noded::wire::MAX_BLOCKS`).
pub(crate) const NODE_PAGE: u32 = 100;

pub(crate) fn page(chain: &[Block], ask: &BlockPage) -> BlockList {
    let top = chain.len() as u64 - 1;
    let start = ask.before.map_or(Some(top), |before| before.checked_sub(1));
    let blocks = start.map_or_else(Vec::new, |start| {
        (0..=start.min(top))
            .rev()
            .take(ask.limit.min(NODE_PAGE) as usize)
            .map(|height| listed(&chain[height as usize]))
            .collect()
    });
    BlockList {
        blocks,
        listed_from: Some(0),
    }
}

pub(crate) fn listed(block: &Block) -> Block {
    Block {
        txs: block
            .txs
            .iter()
            .map(|tx| capped(tx, LISTED_BYTES))
            .collect(),
        ..block.clone()
    }
}

fn capped(tx: &Tx, most: u32) -> Tx {
    let within = tx.size <= most;
    Tx {
        payload: tx.payload.clone().filter(|_| within),
        ..tx.clone()
    }
}

fn located(chain: &[Block], ask: TxAsk) -> Option<Located> {
    chain.iter().find_map(|block| {
        let position = match ask.tx {
            TxRef::At { height, position } => (block.height == height).then_some(position)?,
            TxRef::Hash(hash) => block.txs.iter().position(|tx| tx.hash == hash)? as u32,
        };
        let tx = block.txs.get(position as usize)?;
        Some(Located {
            height: block.height,
            position,
            time: block.time,
            tx: capped(tx, ask.most),
        })
    })
}

pub(crate) fn serve(cx: &TestAppContext, chain: impl Fn() -> Vec<Block> + Clone + 'static) {
    let host = cx.host();
    let held = chain.clone();
    host.handle::<ChainBlocks>(move |ask| Ok(page(&held(), &ask)));
    let held = chain.clone();
    host.handle::<ChainBlock>(move |by| {
        let held = held();
        let found = match by {
            BlockRef::Height(height) => held.get(height as usize),
            BlockRef::Id(id) => held.iter().find(|block| block.id == id),
        };
        Ok(found.map(listed))
    });
    host.handle::<ChainTx>(move |ask| Ok(located(&chain(), ask)));
}

pub(crate) fn status(height: u64) -> NodeStatus {
    NodeStatus {
        chain_id: "test#1".into(),
        block_time_ms: 1000,
        epoch_length: 10,
        height,
        epoch: (height + 1) / 10,
        ..NodeStatus::default()
    }
}

pub(crate) fn account(number: u64, name: &str, control: identity::Control) -> identity::Account {
    identity::Account {
        number,
        card: identity::Card {
            name: name.into(),
            avatar: None,
            bio: None,
            updated_at: 0,
        },
        control,
    }
}

pub(crate) fn ada() -> identity::Account {
    let keys = vec![identity::Key {
        scheme: abi::Scheme::Ed25519,
        key: ADA.to_vec(),
        label: Some("laptop".into()),
        added_at: 0,
    }];
    account(3, "Ada", identity::Control::Person { keys })
}

/// The agent Ada manages, suspended: no keys yet.
pub(crate) fn scout() -> identity::Account {
    account(
        5,
        "Scout",
        identity::Control::Managed {
            runner: None,
            manager: 3,
            category: identity::Category::Agent,
            life: identity::Life::Suspended { keys: Vec::new() },
        },
    )
}

/// forge's own account.
pub(crate) fn forge() -> identity::Account {
    account(
        6,
        "forge",
        identity::Control::Program {
            program: "forge".into(),
        },
    )
}
/// The account a key acts as: Ada's laptop is account 3, a stranger none.
pub(crate) fn holder(signer: &[u8]) -> Option<u64> {
    (signer == ADA).then_some(3)
}

/// One page of `items` as a program answers it: the cursor is the offset
/// of the page after, and a page with no limit is every row.
fn paged<T>(items: Vec<T>, asked: &activity::PageRequest) -> activity::PageResponse<T> {
    let offset = asked
        .after
        .as_deref()
        .map_or(0, |after| usize::from_le_bytes(after.try_into().unwrap()));
    let limit = usize::try_from(asked.limit()).unwrap_or(usize::MAX);
    let next = (offset.saturating_add(limit) < items.len())
        .then(|| (offset + limit).to_le_bytes().to_vec());
    activity::PageResponse {
        height: 1,
        items: items.into_iter().skip(offset).take(limit).collect(),
        next,
    }
}

/// The `activity` program over `chain`, as the kernel's hook would have
/// told it of every frame.
pub(crate) fn index(cx: &TestAppContext, chain: impl Fn() -> Vec<Block> + 'static) {
    cx.host().handle::<Query<Activity>>(move |query| {
        let rows = chain().into_iter().rev().flat_map(|block| {
            let txs = block.txs.into_iter().enumerate().rev();
            txs.map(move |(position, tx)| activity::Row {
                frame: activity::Frame {
                    height: block.height,
                    position: position as u32,
                    time: block.time,
                    target: tx.target,
                    applied: true,
                },
                account: holder(&tx.signer),
                signer: tx.signer,
            })
        });
        Ok(match query {
            activity::Query::Recent { program, page } => {
                let recent = rows
                    .filter(|row| program.as_ref().is_none_or(|p| row.frame.target == *p))
                    .collect();
                activity::Reply::Recent(paged(recent, &page))
            }
            activity::Query::Acted { account, page } => {
                let acted = rows
                    .filter(|row| row.account == Some(account))
                    .map(|row| row.frame)
                    .collect();
                activity::Reply::Frames(paged(acted, &page))
            }
            activity::Query::Signed { key, page } => {
                let signed = rows
                    .filter(|row| row.signer == key)
                    .map(|row| row.frame)
                    .collect();
                activity::Reply::Frames(paged(signed, &page))
            }
            activity::Query::Used { page } => {
                let mut used: Vec<activity::Used> = Vec::new();
                for row in rows {
                    let Some(account) = row.account else {
                        continue;
                    };
                    let program = row.frame.target;
                    let counted = used
                        .iter_mut()
                        .find(|used| (used.account, &used.program) == (account, &program));
                    match counted {
                        Some(used) => used.frames += 1,
                        None => used.push(activity::Used {
                            account,
                            program,
                            frames: 1,
                        }),
                    }
                }
                used.sort_by(|a, b| (a.account, &a.program).cmp(&(b.account, &b.program)));
                activity::Reply::Used(paged(used, &page))
            }
        })
    });
}

/// A node at `tip`, whose tip the test may move.
pub(crate) fn node(cx: &mut TestAppContext, tip: Rc<RefCell<u64>>) {
    let host = cx.host();
    let head = tip.clone();
    host.handle::<ChainStatus>(move |()| Ok(status(*head.borrow())));
    let head = tip.clone();
    index(cx, move || chain(*head.borrow()));
    serve(cx, move || chain(*tip.borrow()));
    identities(cx, vec![ada(), scout(), forge()]);
    host.handle::<Query<Valset>>(|_| Ok(valset::Reply::Validators(vec![VALIDATOR.to_vec()])));
    respond(cx);
    describes(cx);
}

pub(crate) fn entry(program: &str, code: u8) -> programs::Entry {
    programs::Entry {
        program: program.into(),
        code: BlobId::Sha256([code; 32]),
        params: vec![1, 2, 3],
    }
}

/// The programs role: chat and identity running, a view-only entry, and forge's
/// removal scheduled at 120.
pub(crate) fn respond(cx: &mut TestAppContext) {
    programs_with(
        cx,
        vec![entry("chat", 0xab), entry("identity", 0xcd)],
        vec![programs::View {
            name: "explorer".into(),
            view: BlobId::Sha256([0xef; 32]),
        }],
        vec![programs::Pending {
            height: 120,
            change: programs::Change::Remove("forge".into()),
            by: None,
        }],
    );
}

/// The programs role answering what it runs, the views it lists and what is
/// scheduled; no grants, no hooks.
pub(crate) fn programs_with(
    cx: &TestAppContext,
    programs: Vec<programs::Entry>,
    views: Vec<programs::View>,
    scheduled: Vec<programs::Pending>,
) {
    programs_granting(cx, programs, views, scheduled, Vec::new(), Vec::new());
}

/// The programs role, its grants and its hooks too, under the suite's roles.
pub(crate) fn programs_granting(
    cx: &TestAppContext,
    programs: Vec<programs::Entry>,
    views: Vec<programs::View>,
    scheduled: Vec<programs::Pending>,
    grants: Vec<programs::Grant>,
    hooks: Vec<String>,
) {
    fn whole<T>(items: Vec<T>) -> programs::PageResponse<T> {
        programs::PageResponse {
            height: 1,
            items,
            next: None,
        }
    }
    cx.host().handle::<Query<Programs>>(move |query| {
        Ok(match query {
            programs::Query::Roster => programs::Reply::Roster {
                height: 1,
                programs: programs.clone(),
                views: views.clone(),
            },
            programs::Query::Scheduled { .. } => {
                programs::Reply::Scheduled(whole(scheduled.clone()))
            }
            programs::Query::Hooks(_) => programs::Reply::Hooks(hooks.clone()),
            programs::Query::Grants { .. } => programs::Reply::Grants(whole(grants.clone())),
            programs::Query::Roles => programs::Reply::Roles(abi::Roles {
                programs: programs::PROGRAM.into(),
                validators: valset::PROGRAM.into(),
                identity: identity::PROGRAM.into(),
                blobs: "blobs".into(),
                awaits: "awaits".into(),
            }),
            other => panic!("unexpected query: {other:?}"),
        })
    });
}

pub(crate) fn ready() -> (TestAppContext, Rc<RefCell<u64>>) {
    let mut cx = TestAppContext::new();
    let tip = Rc::new(RefCell::new(12));
    node(&mut cx, tip.clone());
    cx.open::<Explorer>();
    cx.run_until_parked();
    (cx, tip)
}

/// The host's `program.describe`, as each program's describe module would
/// answer: its own `describe` over the op, `None` for a program without one.
pub(crate) fn describes(cx: &mut TestAppContext) {
    fn with<T: borsh::BorshDeserialize>(
        op: &[u8],
        describe: fn(&T) -> Description,
    ) -> Option<Description> {
        borsh::from_slice(op).ok().map(|op| describe(&op))
    }
    cx.host().handle::<ProgramDescribe>(|(program, op)| {
        Ok(match program.as_str() {
            "chat" => with(&op, chat::describe),
            "forge" => with(&op, forge::describe),
            "identity" => with(&op, identity::describe),
            "valset" => with(&op, valset::describe),
            programs::PROGRAM => with(&op, programs::describe),
            _ => None,
        })
    });
}

/// A full window: 1,000 blocks, each with a post, and a 1 MB push at the tip.
pub(crate) fn heavy(cx: &mut TestAppContext) {
    let tip = WINDOW as u64;
    let chain: Vec<Block> = (0..=tip)
        .map(|height| {
            let seed = (height % 250) as u8;
            let mut tx = if height == tip {
                let push = forge::Op::Push {
                    repo: "app".into(),
                    request: vec![0x50; 1 << 20],
                    pack: None,
                };
                tx(0xfe, ADA, forge::PROGRAM, borsh::to_vec(&push).unwrap())
            } else {
                let text = format!("message {height}");
                tx(seed, ADA, chat::PROGRAM, post("design", &text))
            };
            tx.hash[..8].copy_from_slice(&height.to_le_bytes());
            Block {
                height,
                id: [seed; 32],
                parent: [seed.wrapping_sub(1); 32],
                time: T0 + height * 1000,
                epoch: height / 10,
                proposer: Some(VALIDATOR.to_vec()),
                txs: vec![tx],
            }
        })
        .collect();
    let host = cx.host();
    host.handle::<ChainStatus>(move |()| Ok(status(tip)));
    let indexed = chain.clone();
    index(cx, move || indexed.clone());
    serve(cx, move || chain.clone());
    identities(cx, vec![ada()]);
    host.handle::<Query<Valset>>(|_| Ok(valset::Reply::Validators(vec![VALIDATOR.to_vec()])));
    respond(cx);
    describes(cx);
}

/// identity listing `accounts`, and answering each by number.
pub(crate) fn identities(cx: &TestAppContext, accounts: Vec<identity::Account>) {
    cx.host()
        .handle::<Query<Identity>>(move |query| match query {
            identity::Query::List { .. } => Ok(identity::Reply::Accounts(programs::PageResponse {
                height: 1,
                items: accounts.clone(),
                next: None,
            })),
            identity::Query::Get { number } => Ok(identity::Reply::Account(
                accounts
                    .iter()
                    .find(|account| account.number == number)
                    .cloned(),
            )),
            other => panic!("unexpected identity query: {other:?}"),
        });
}

/// A restore that must read nothing: every ask left unanswered.
pub(crate) fn quiet_host(cx: &TestAppContext) {
    let host = cx.host();
    host.never::<ChainStatus>();
    host.never::<ChainBlocks>();
    host.never::<ChainTx>();
    host.never::<Query<Identity>>();
    host.never::<Query<Valset>>();
    host.never::<Query<Programs>>();
    host.never::<Query<Activity>>();
    host.never::<ProgramDescribe>();
}
