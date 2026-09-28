# Writing a module

One walk from `make new-module` to a module founded on a local network. The
example is `poll`: a question with options, opened in a chat channel, voted
on by people and agents, closed by its opener. Its snippets are the real
thing; nothing here is generated. The crate is what a reader follows: plain
types, plain functions, one host call per context method.

Read [README.md](../README.md) for the map of the repository and
[roles.md](roles.md) for the three roles and the account model first.

## 1. The crate

```sh
make new-module NAME=poll
```

writes `crates/app/poll/` in valset's shape, registers it (`PROGRAMS` and
`VIEW_LINKABLE` in the Makefile, the workspace members and dependencies in
`Cargo.toml`) and prints what to do next. The files, in reading order:

| File | What |
|---|---|
| `src/lib.rs` | the types on the wire (`Op`, `Query`, `Reply`, the rows they carry), `MODULE`, `describe()` and `describe::export!` |
| `src/program.rs` | `pub struct Poll; impl guest::Module for Poll`: one match over every op, one over every query, `reply` if it emits with a reply wanted; `guest::export!` behind `module` |
| `src/rules.rs` | the tables (`store`), and what each op checks and writes |
| `src/view.rs` | behind `view`: the marker a view names this module by |
| `src/tests.rs` | the module natively over `MockHost` (add a `MockChain` test once it talks to another module) |

`Cargo.toml` declares three features, all off by default:

- `module` adds the two wasm exports (`guest::export!`). Only the module's
  own wasm build turns it on (`make wasm-programs` builds each program in a
  target dir of its own with `--features module`). Everything else, the
  types, the rules, the `Poll` type, is always built: a view links the crate
  with the feature off and gets the types with no host import and no export,
  and a native test runs the real `execute` and `query`.
- `view` adds `src/view.rs`, the `ducktape_view_guest::methods::Module`
  marker `poll-view` names the module by: `poll::view::PollApi`, named apart
  from the module's `Poll` and the view's own `Poll`.
- `describe` makes the crate's wasm build the `ducktape.describe` module: the
  `describe` export alone, no module, no imports (section 7).

A module another module links (forge links chat, poll links chat) is why
the exports sit behind a feature: the exports are global symbols, and a
wasm crate has one `export!`.

## 2. Op, Query, Reply

All three are borsh enums in `lib.rs`; the same types a view and a sibling
module link. A write is an `Op`, a read a `Query` answered by a `Reply`.

```rust
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Op {
    Open { poll_id: String, channel: String, question: String, options: Vec<String> },
    Vote { poll_id: String, option: u32 },
    Close { poll_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Query {
    Poll { poll_id: String },
    Polls { page: PageRequest },
    Tally { poll_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Reply {
    Poll(Option<PollRow>),
    Polls(PageResponse<PollRow>),
    /// One count per option, in option order.
    Tally(Vec<u64>),
}
```

**The append-only rule.** Borsh encodes a variant by its index, and a host
describes an op that landed at any height with the module's *current*
code. So `Op` only grows at its end: append a variant, never insert,
remove or reorder one. The scaffold's test holds you to it:

```rust
#[test]
fn op_variants_only_append() {
    assert_eq!(describe::variants::<Op>(), ["Open", "Vote", "Close"]);
}
```

A field added to an existing variant changes its bytes too; that is a new
variant. `Query` and `Reply` are looser (a view and its module ship
together), but a sibling module that queries you links your types, so
treat them the same.

Every list a client asks for takes a `store::PageRequest` and answers a
`store::PageResponse` (section 3); the row types (`PollRow`) live beside
the enums so a view renders what the module keeps.

## 3. State: tables over `store`

A module's state is its own key space of bytes; nothing reads another
module's keys (section 6 is how you ask). `store` puts typed tables over
it, declared once as `const`s in `rules.rs`:

```rust
use store::{Map, Set};

pub(crate) const POLLS: Map<String, PollRow> = Map::new("poll/");
/// Votes per option: `(poll, option)`.
pub(crate) const TALLY: Map<(String, u32), u64> = Map::new("tally/");
/// Who voted, so each account votes once.
pub(crate) const VOTERS: Set<(String, AccountNumber)> = Set::new("voter/");
```

- **Prefixes.** Every key of a table starts with its prefix, and two
  prefixes must not be prefixes of each other (`"m/"` beside `"m/x/"` would
  read each other's rows). One word and a slash is the convention.
- **Keys** encode through `store::KeyCodec` so byte order is value order:
  integers big-endian, strings and byte vectors NUL-terminated (a shorter
  name sorts before its extensions), `Principal` as its borsh, tuples
  concatenated. A tuple key scans by its leading elements: `TALLY.prefix_of(&poll_id)`
  is every `(poll_id, _)` row, which is how every "in this poll" read works.
  A key part that should list newest first is stored as `u64::MAX - n`
  (chat's `newest_first`).
- **Values** are borsh. A row that does not decode is refused `corrupt`,
  never a panic.
- **Reads take `&QueryCtx`**, which an `&ExecCtx` derefs to, so a rule that
  only reads is written once and serves both. Writes take `&ExecCtx`.

The methods: `get`, `has`, `put`, `remove`; `scan(ctx, range)` over a
`Range` the table builds (`prefix_of`, `below`); `all` (unbounded: for a
module's own rules, never a client's query); `range`/`range_of` for a page.
`Item<T>` is one value at one key (`COUNT.update(ctx, |n| *n += 1)`).

**Pages.** A client's listing answers a bounded, resumable page:

```rust
pub(crate) fn polls(ctx: &QueryCtx, page: &PageRequest) -> Result<PageResponse<PollRow>, Error> {
    Ok(POLLS
        .range(ctx, page, ctx.env().height)?
        .map(|(_, row)| row))
}
```

`PageRequest { after, limit }` is what a client sends (`PageRequest::first(n)`,
then `PageRequest::resume(reply.next, n)`); the limit is clamped to
`PageRequest::MAX_LIMIT` (256), or to a module's own bound with
`page.bounded(max)`. `PageResponse { height, items, next }` answers with the
height that answered and an opaque cursor for the page after. A cursor is
bound to the listing it came from (a cursor from one listing handed to
another is refused `stale`) and cannot escape its prefix.

Reads see the frame's own writes so far. `committed_get`/`committed_scan`
read as of the last committed block, which a module rarely wants.

## 4. Rules, errors and codes

A rule checks, then writes. A refused op's writes are undone by the host,
so nothing needs rolling back; but writing only after every check keeps a
rule readable and the native `attempt` check (section 8) trivially true.

```rust
pub(crate) fn vote(ctx: &ExecCtx, voter: AccountNumber, id: &str, option: u32) -> Result<(), Error> {
    let row = poll(ctx, id)?;
    if row.closed {
        return Err(wrong_state(format!("poll {id} is closed")));
    }
    if option as usize >= row.options.len() {
        return Err(invalid(format!("poll {id} has options 0..{}", row.options.len())));
    }
    ctx.require_person_or_agent(voter)?;
    let voted = (id.to_owned(), voter);
    if VOTERS.has(ctx, &voted) {
        return Err(already_exists(format!("account {voter} voted in {id}")));
    }
    let key = (id.to_owned(), option);
    let count = TALLY.get(ctx, &key)?.unwrap_or(0);
    TALLY.put(ctx, &key, &(count + 1));
    VOTERS.insert(ctx, &voted);
    Ok(())
}
```

An `Error { code, message }` is a code token and one sentence for a person.
The code says what a caller does about it; the constructors in `guest` are
named by it:

| Constructor | Code | The caller |
|---|---|---|
| `invalid(..)` | `invalid_input` | fixes the request; retrying it unchanged never succeeds |
| `not_found(..)` | `not_found` | names a thing that exists |
| `already_exists(..)` | `already_exists` | creates under another id, or treats the create as done |
| `wrong_state(..)` | `wrong_state` | changes the thing's state first (a closed poll takes no vote) |
| `unauthorized(..)` | `unauthorized` | acts as someone else |
| `capacity(..)` | `capacity` | sends less or removes something |
| `stale(..)` | `stale` | re-reads and retries (a cursor from another listing) |
| `corrupt(table, key, what)` | `corrupt` | an operator: stored state failed an invariant |
| `unexpected_reply(module, asked, &reply)` | `unexpected_reply` | an operator: a sibling answered another shape |

`guest::code` has the full list; `unknown_program`, `trap`, `protocol` and
`sequence` are the host's, which a module meets only as a sibling's
answer. Word the
sentence for the person who reads it in a refusal: what was asked and why
not, no internals.

Bytes that do not decode as your `Op` or `Query` are refused `invalid_input`
before `execute` runs ("poll: Op did not decode: …"); `init`'s params go
through the same `guest::decoded` if you take any.

## 5. Sender, origin, identity

Every context carries the call's `Env`: `chain_id`, `height`, `time`,
`module` (this module's own id), `origin`, `sender`, `roles`, `cause`.

The host resolves **who a frame acts as** once per frame through the
identity role and hands it over as `sender`: a signed frame acts as the
account its key holds, a message as the emitting module's account, genesis
and the kernel as the chain (`Root`). A key that holds no account still
runs, as no one. A write reads it first thing:

```rust
fn execute(ctx: &ExecCtx, op: Op) -> Result<(), Error> {
    // every op acts as an account: a person, an agent or a module
    let account = ctx.sender_account()?;
    match op {
        Op::Open { poll_id, channel, question, options } => open(ctx, account, poll_id, channel, question, options),
        Op::Vote { poll_id, option } => vote(ctx, account, &poll_id, option),
        Op::Close { poll_id } => close(ctx, account, &poll_id),
    }
}
```

`ctx.sender()` is the `Principal` (`Account(n)` or `Root`) and refuses a
frame that acts as no one; `ctx.sender_account()` is the number and refuses
the chain too. No row a module writes ever names a bare key.

The **origin** is the raw caller (`Origin::Signed(key)`, `Module(id)`,
`Root`), for the few rules that need it rather than the account:
`env.signer()` (forge takes repository ops from a signed frame only),
`env.sending_module()`, `env.sent_by("forge")`. Chat's `poll:<name>` ids
belong to the module the prefix names, checked on the origin.

The **identity role** answers who an account is. `ctx.require_person_or_agent(n)`
asks it (`Profile(n)`) and refuses an absent account, a module's, or an
agent that is suspended or revoked: poll's voters, chat's dm counterpart,
forge's reviewers. Ask a role at the module genesis bound to it,
`ctx.env().roles.identity`, never by a name you assume. The profiles
themselves (`abi::role::identity::{Profile, Kind, Standing}`) are what a
view shows beside a name.

`Env::authority()` is the stub that admits anyone until the chain has an
authority; valset and the registry call it before a write.

## 6. Talking to other modules

**A query is a read.** `ctx.query::<Q, R>(module, &q)` runs the sibling's
`query` in this frame, borsh both ways, and moves no state. poll asks chat
whether the channel exists before it opens:

```rust
fn room(ctx: &QueryCtx, id: &str) -> Result<Option<chat::ChannelInfo>, Error> {
    let asked = chat::Query::Channel { channel_id: id.to_owned() };
    match ctx.query::<chat::Query, chat::Reply>(chat::MODULE, &asked)? {
        chat::Reply::Channel(room) => Ok(room),
        other => Err(guest::unexpected_reply(chat::MODULE, "Channel", &other)),
    }
}
```

Link the sibling's crate (`chat = { workspace = true }`, its `module`
feature off) for its types. The sibling's refusal is your `Err`.

**An emit is a write** the target runs in this frame, as a message from
this module, once your handler returns Ok; messages run in emit order,
each before the next, depth first, at most 8 deep. `ctx.emit(target, &op,
reply)` names the target's `Op` and answers a `MessageId` (this module and
the message's number in the frame). What the target's refusal does is
`reply`:

- `Reply::None`: the refusal fails this whole frame. Every host is left as
  it was, your writes included. poll's closing line must land, so the
  close is posted this way: a channel that refuses it un-closes the poll.
- `Reply::Wanted`: the target's writes are undone, and its outcome comes
  back to you in the same frame as `Module::reply(ctx, id, outcome)`, a
  run of its own whose `cause` is `Cause::Reply`. Return Ok to absorb the
  refusal, Err to fail the frame after all. The default `reply` absorbs an
  applied outcome and returns a refusal, so a module that never wants a
  reply leaves it alone.

A reply names only the message's id. A module that wants to know *what* it
was waiting for keeps that itself:

```rust
/// The poll each announcement was posted for, by the message's number in
/// its frame; the reply comes in the same frame and removes it.
const ANNOUNCED: Map<u64, String> = Map::new("announced/");

// in open(): the room may refuse the line (members only); the poll opens anyway
let posted = post(ctx, &channel, &format!("{id}:open"), &line, Reply::Wanted);
ANNOUNCED.put(ctx, &posted.seq, &id);

/// What chat did with an announcement: the poll records whether its line
/// landed; a refusal is absorbed, not raised, so the open stands.
pub(crate) fn announced(ctx: &ExecCtx, id: &MessageId, outcome: &Outcome) -> Result<(), Error> {
    let Some(poll_id) = ANNOUNCED.get(ctx, &id.seq)? else {
        return Err(wrong_state(format!("no announcement is message {}", id.seq)));
    };
    ANNOUNCED.remove(ctx, &id.seq);
    if let Outcome::Applied { .. } = outcome {
        let mut row = poll(ctx, &poll_id)?;
        row.announced = true;
        POLLS.put(ctx, &poll_id, &row);
    }
    Ok(())
}
```

and `program.rs` points `reply` at it:

```rust
fn reply(ctx: &ExecCtx, id: &MessageId, outcome: &Outcome) -> Result<(), Error> {
    announced(ctx, id, outcome)
}
```

The message acts as *your* account (identity's `OfModule`), so the target
sees `sender: Account(poll's number)` and `origin: Module("poll")`. Chat
lets a module post under its own `<module>:<name>` ids
(`chat::namespace::id(MODULE, name)`) and nowhere else.

`ctx.event(bytes)` adds to the call's receipt, for readers of the chain;
`ctx.set_return_data(bytes)` is the execute's output, which a `Reply::Wanted`
emitter gets back as `Outcome::Applied { output }`.

## 7. Describe

A program ships, beside its code, a pure wasm module that turns an op's
bytes into what a person reads: a title and typed fields. Explorer shows
it for every transaction. `lib.rs` has the function and the one export
line; `make wasm-describes` builds the module (`--features describe`) and
qa packs it into the program.

```rust
pub fn describe(op: &Op) -> describe::Description {
    use describe::{Value, field};
    let (title, fields) = match op {
        Op::Open { poll_id, channel, question, options } => (
            format!("Open poll {poll_id} in #{channel}"),
            vec![
                field("question", Value::text(question)),
                field("options", Value::List(options.iter().map(Value::text).collect())),
            ],
        ),
        Op::Vote { poll_id, option } => (
            format!("Vote in {poll_id}"),
            vec![field("option", Value::count(u64::from(*option)))],
        ),
        Op::Close { poll_id } => (format!("Close poll {poll_id}"), vec![]),
    };
    describe::Description { title, fields }
}

describe::export!(Op, describe);
```

`Value` is a small vocabulary a reader renders each its own way: `Text`,
`Account(n)` (shown as a name), `Key`, `Module(name)` (a link), `Hash`,
`Amount { value, decimals }` (`Value::count(n)` for a plain integer),
`Time` (ms since the epoch), `Bytes` (`Value::bytes(..)` keeps a preview),
`List`. Title first, as a sentence a person would say ("Post in #design").

## 8. Native tests

A module's tests run its real `execute` and `query` natively. There are two
harnesses in `guest`, and a third in `conformance` for a module that fills a
role.

**`MockHost`: the module alone.** State, blobs and what the module sent
(output, emissions, events) in maps a test reads. The contexts a test makes
over it are the same types the host makes:

```rust
let host = MockHost::default();
let ada = MockHost::env(MODULE).signed(key(ADA), Some(ADA));
Poll::execute(&host.exec(ada), op)?;
let reply = Poll::query(&host.query(MockHost::env(MODULE)), query)?;
```

`MockHost::env(module)` is a direct call by the chain itself (`Root`); a
test moves what it cares about with `Env::signed(key, account)`,
`Env::from_module(module, account)` or struct update
(`Env { height: 7, ..MockHost::env(MODULE) }`). Siblings answer through
`host.sibling::<chat::Chat>("chat", &chat_host)` (a second host holding
chat's state, asked through chat's own decoding) and the identity role
through `host.identity(profiles)` over a fixed roster. An emitted message
is kept, not run: `host.take_emissions()` shows what left and whether a
reply was wanted. The attack check every module shares:

```rust
let refusal = host.refused(|| Poll::execute(&host.exec(bo), vote("lunch", 0)));
assert_eq!(refusal.code, code::ALREADY_EXISTS);
```

`refused` (and `attempt` for a write that may succeed) asserts a refusal
left the host exactly as it found it: no state, blob, emission, event or
output.

**`MockChain`: several modules, a submission as one frame.** Each module
is seated by name over a host of its own; every seat queries every other;
a submission is run as the kernel runs it: the signer resolved through
identity, the module, then what it emitted, depth first, replies included,
undone whole on a rejection nobody absorbs.

```rust
let mut chain = MockChain::default();
chain.seat::<Poll>(MODULE);
chain.seat::<chat::Chat>("chat");
chain.register(MODULE, POLL);        // poll's account, as identity registers it
chain.hold(key(ADA), ADA);           // Ada's key holds account 1, a person
chain.submit(Origin::Signed(key(ADA)), "chat", &channel("general", PostPolicy::Open))?;
chain.submit(Origin::Signed(key(ADA)), MODULE, &open("lunch", "general"))?;
let reply: Reply = chain.query(MODULE, &Query::Poll { poll_id: "lunch".into() })?;
```

With no identity module seated, the chain's roster answers as the role
would (`hold`, `register`, `profile` for an agent and its standing); seat
`identity::Identity` at `MockHost::roles().identity` and the chain asks it
instead, and the roster's edits do nothing (identity's own ops seat keys
then). `chain.init(module, &params)` runs `init` as the kernel admits a
module: one frame, what it emits run after it, a refusal undoing it all;
`chain.at(height, time)` moves the chain to another block. The two tests that matter
for poll: the announcement lands in the frame that opened the poll, and a
room that refuses the line still gets its poll while a refused closing
line fails the close whole.

`crates/app/forge/tests/common/sandbox.rs` is a larger harness on the same
chain (forge and chat, agents, heights).

**`conformance`: a role.** A module bound to `registry`, `validators` or
`identity` at genesis speaks that role's bytes: the role's `Op`, `Query`
and `Reply` variants are your enums' first, in order, with the same
fields. Take `conformance` as a dev-dependency, implement the role's
`Fixture` (the few things the role leaves to your own ops) and call
`conformance::identity::run(&fixture)` in a test; its
[README](../crates/sdk/conformance/README.md) lists what each role is held
to.

`cargo test -p poll` runs one module's tests; `make test` everything (the
founding suite under `crates/system/module-registry/tests` builds the
boot set itself and founds ducktape's host over it). The founding suite
also links the kernel's own `guest`, so `-p guest` beside `-p
module-registry` is ambiguous to cargo; alone it picks the workspace's, and
`-p "path+file://$PWD/crates/sdk/guest"` names it anywhere.

## 9. The edit loop

```sh
make dev P=poll            # the module's wasm, its native tests
make dev P=poll V=poll-view   # and the view's wasm, gated
```

`make dev` rebuilds what cargo finds stale, one line per artifact
(`poll  165,724 B` or `unchanged`), gates a rebuilt view (ABI) and runs the
native tests of the crates whose test binaries cargo rebuilt. A module
that fails to build for wasm32 fails here; the usual cause is a dependency
that reaches the host or a signing library (`make view-wasm-check` names
them for the crates a view may link).

`make new-view NAME=poll-view` scaffolds the screen over the module's
marker; replace its `count()` with what your `Query` answers before its
first `make dev`. Where a view's bytes go: `make wasm-why V=poll-view`
(`twiggy top` over a build that keeps its names; `cargo install twiggy`).

`make wasm-modules` builds every program, its describe module and every
view under `$CARGO_TARGET_DIR/wasm32-unknown-unknown/release/`, unpacked;
nothing built is committed.

## 10. Founding

Packing a view and a describe module into their program, and founding a
network on the result, is the qa repository's job (`./kit`). To add poll to
its local network:

1. `founding.toml`: a `[[programs]]` entry, `id = "poll"`,
   `code = "@PROGRAMS@/poll.wasm"` (and `params = …` if `init` takes any).
   The three `[roles]` stay bound to the boot set.
2. `crates/kit/src/main.rs`, `pack()`: `"poll"` among the programs it
   copies from modules' release dir, `("poll", "poll_view")` among the
   views it embeds, `"poll"` among the describe modules it embeds.
3. `kit build NAME && kit up NAME`: modules' `make wasm-modules`, the pack,
   a fresh network with the program admitted at genesis, its account
   registered with identity, and the app open on it.

A module added to a running network goes through the registry instead
(`Publish`, then `Schedule` at a height; [roles.md](roles.md), "Updating a
module"): its `init` runs with the scheduled params when the height comes.
