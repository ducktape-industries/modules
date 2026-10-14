# Roles, accounts and updates

The kernel knows four programs by what they do, never by their ids. Genesis
binds each role to a founding program; every call the kernel makes goes to the
program bound to that role, speaking the role's interface in
`crates/sdk/abi` (`abi::role::{programs, validators, identity}`) or, for
blobs, ducktape's `blob::contract`. The programs in `crates/system/` are one
implementation of each.

## The four roles

| Role | Bound here to | What the kernel does with it |
|---|---|---|
| `programs` | `programs` | at founding, writes its params (`role::programs::Genesis`: every founding program and view, and the founding hooks); at each block, before any frame, asks `At(height)` and admits, swaps or drops programs to match the answer, then asks `Hooks(height)` for the programs it tells of each frame the block carries ("Hooks", below) |
| `validators` | `valset` | at founding, writes its params (`role::validators::Genesis`: the founding validators and the member cap, the most members it may ever hold, 1 to `MAX_MEMBERS` = 128); at founding and on the last block of each epoch, asks `Validators` and `Members`: it seats the Validators for the next epoch and tracks the Members (a Resident follows the chain but never votes or proposes); a Members answer past the cap halts the network there. The node (`ducktape join`, `invite`, `vote`, `halo`, `leave`) signs the role's `Op` with its identity key. Each member registers the halo address other members dial it at (`Member.halo`, host:port): `Enroll` and `SetHalo` refuse an unspecified or loopback host as `invalid_input` |
| `identity` | `identity` | as it admits a program (at founding, and later off the programs role), executes `RegisterProgram { program }` with the `System` origin; for each signed frame asks `Account(key)`; for each message and reply asks `OfProgram(sender)`, and for a message a program sends as another account asks `Runs { program, account }` |
| `blobs` | `blobs` | when a program reads a blob (`blob_get`) that no program put, and when the programs role admits code, asks `Held { blob }`: the blob is served only once a bundle naming it is held, which it is from the receipt that brings two thirds of the current validators. The node watches the role's writes: it gathers every bundle put, and a validator's node receipts a bundle once it holds every blob |

The founding file binds them (ducktape's `[roles]` table; qa's
`founding.toml` is a working one):

```toml
[roles]
programs = "programs"
validators = "valset"
identity = "identity"
blobs = "blobs"
```

All four are required. Founding refuses a role bound to a program the
founding does not list, and a node refuses a network that records no roles.
Every frame's `Env` carries the bindings (`env.roles`), so a program that asks
a role asks `ctx.env().roles.identity`, not a name it assumes; chat's roster
does this (`crates/app/chat/src/origin.rs`).

## Who a frame acts as

The host resolves the sender once per frame, through the identity role, and
hands it to the program as `Env.sender`:

- a signed frame acts as the account its key holds (`Account(key)`); a key
  that holds none still runs, as no one (`sender: None`), which is how
  identity's own `Create` works; a key whose account does not act (a
  suspended agent) is refused, and the host rejects the frame;
- a message or a reply from another program acts as that program's account
  (`OfProgram(program)`);
- a message a program sends as another account (`ctx.emit_as(account, ..)`,
  `Message.acting`) acts as that account when identity answers
  `Runs { program, account }` with it: an agent whose manager handed it to
  that program, while the agent is active. Any other answer, or a refusal,
  rejects the message, so no key is held anywhere for an agent a program
  runs;
- founding and the kernel's own calls act as the chain (`Root`).

A program reads it as `ctx.sender()`, which is `Principal::Account(number)`
or `Principal::Root` and refuses a frame that acts as no one, so no row a
program writes names a bare key. `Env::signer`, `sending_program` and
`sent_by` check the raw origin instead, where a program needs the key or the
program itself (forge requires a signed frame; chat's `:` namespace belongs to
the program its prefix names).

## Accounts

Everything that acts is an account in `identity`:
`Account { number, card, control }`. The `Card` is what it shows (name,
avatar, bio); the `Control` is who acts as it:

- `Person { keys }`: a person and the keys they hold.
- `Managed { manager, category, life, runner }`: an agent. Its manager, a
  person, gives it keys, hands it to a program to run (`runner`), and
  decides its `Life`: `Active { keys }`, `Suspended { keys }` (keys kept,
  frames refused) or `Revoked` (keys dropped, final).
- `Program { program }`: a program's account, registered by the kernel as it
  admits the program. It holds no keys; the program alone acts as it.

Who may do what (the acting account is `ctx.sender()`):

| Op | Who |
|---|---|
| `RegisterProgram { program }` | the system (`Root` origin) alone; a program that has an account keeps it |
| `Create { name, scheme }` | a key that holds no account; it becomes a person's first key |
| `CreateAgent { name }` | a person; they become its manager |
| `AddKey` | a person's account: the new key signs the frame and a key already on the account consents. An agent's: its manager signs and the new key consents to itself. Never a program's or a revoked agent's |
| `RemoveKey` | a person removes their own key or a junior one, never their last. An agent's manager removes any of its keys |
| `SetName`, `SetProfile` | a person or a program on its own account; an agent's manager on the agent's (not once revoked) |
| `Suspend`, `Resume`, `Revoke` | the agent's manager |
| `SetRunner { account, runner }` | the agent's manager, not once revoked. `Some(program)` names a program that has an account; that program then sends messages as the agent (`Runs`). `None` takes it back |
| `SetAudience { account, audience }` | the agent's manager, not once revoked. It records whom the agent answers: `Manager`, `Accounts` (the manager and at most `MAX_AUDIENCE` = 64 others) or `Anyone`. The agent's runtime reads it with `Query::Audience`; nothing on the chain enforces it. An agent with no setting, or a revoked one, answers `Manager` |

The role's queries, which views and other modules ask too, are
`Account(key)`, `OfModule(module)`, `Profile(number)`,
`Profiles { after, limit }` and `Runs { program, account }`. A `Profile` is
`{ number, name, kind }`, where `Kind` is `Person`,
`Managed { manager, category, standing, runner }` or `Program(id)`. `Kind::badge` and `Kind::note` in `abi` turn it into what a view
shows beside a name ("Agent · managed by Dev", "Program · chat", "suspended"),
the one place that happens.

## Updating a program

`programs` holds the roster and the changes scheduled to it:

1. `ducktape blob put FILE --kind program --wait` uploads the code to the
   node, which submits the blobs role's `Put` naming it. Every member gathers
   it; each validator's node receipts it once it holds it; the command
   returns once two thirds of the current validators have receipted it, and
   the blobs role answers `Held` for it. The programs role refuses to schedule code
   the blobs role does not hold, and a held blob of any kind but
   `"program"`.
2. `Schedule(Scheduled { height, change: Change::Set(Entry { program, code, params }) })`
   lands it at a later height.
3. At that height the kernel's `At(height)` sees the new code. Under an id
   already running, the code is swapped: the state, the account and the id
   stay, and `init` does not run again. Under a new id, the program is
   admitted (its `init` gets `params`) and registered with identity.

A change lands at its height whether or not an op runs on the programs role
after it: the kernel asks `At(height)` for the block it applies, and a
client (the app's rail, Explorer) asks `Roster`, the programs and views at
the query's own height, with that height.

`Change::Remove` drops a program; `Cancel` withdraws a scheduled change. A
role's program updates the same way, under its bound id; the binding itself
is fixed at genesis.

## Hooks

A hook is a program the kernel tells of every frame a block carries. The
programs role lists the hooks: the founding file's top-level `hooks = [...]`
seeds them, `Change::SetHook(id)` makes a running program one from its
height, and `Change::RemoveHook(id)` stops it there. `Change::Remove` of a
program drops its hook with it. At each block the kernel asks
`Hooks(height)`; a programs role that refuses or answers something else names
no hooks.

After each frame runs, the kernel calls each hook with a `hook::Record`:
the block height, the frame's place in the block, its signer, the account
it acted as, its target and whether it applied. A refused frame is told
too. The call's origin is the kernel itself (`Origin::Root`, from
`Env::kernel()` in `program`), and `program::hook` refuses any other. A hook
never changes a frame: its receipt stays as it ran, a hook that refuses,
traps, runs out of fuel or emits a message keeps none of its writes, and
the block goes on. `activity` (`crates/app/activity`) is the hook this
repo founds: it indexes who signed what, by key and by account, and keeps
the newest 1,000 frames of each.

Who may schedule or cancel (putting a blob is open to any account):

- The validators' vote, any change: `Env::authority()` in `program` admits
  only a message from the program bound to the validators role, and
  `valset` sends one only when the vote passes a `Motion::Programs { op }`:
  the enacting vote emits `op` to the programs role in the same frame, and a
  refusal there fails that vote.
- A maintainer: a signed frame whose account holds a `Grant` of the id
  (`Scope::Program(id)`, which may run nothing yet) or of every program
  (`Scope::All`). It sets, removes and cancels changes of the ids it
  covers, never a role's id, and its change lands at least `NOTICE` (60)
  blocks after the block it is made in, so the vote can cancel it first.
  It may not change an id the vote has a change waiting on, nor claim an
  id a maintainer's waiting removal frees (a program under a view being
  removed, or the reverse) before that removal lands; it keeps at most
  `MAX_PENDING` (8) changes waiting, and cancels only a grant's change.
  `Scheduled` answers who made each change (`Pending.by`, `None` for the
  vote).
- Hooks: a maintainer may replace a hook's code (`Set` of a hooked id).
  The hook keeps its status: it still cannot emit and runs on the kernel's
  capped hook fuel. Only the vote sets, unsets or removes a hook
  (`SetHook`, `RemoveHook`, `Remove` of a hooked id).

Only the vote grants (`Op::Grant`, to a person or an agent identity
profiles, never a program's account); `Op::Revoke` is the vote's or the
holder's own, and drops every change the account made that it no longer
covers. The vote wins: its change to an id takes every change a grant
made to that id with it, and its removal of an id the id's `Program`
grants too. The vote's removal of an id that holds nothing at its height
takes those along and waits as no entry, and its cancel of a change
already gone is no error, so a motion never sits one vote short of a
done thing.

## Membership

`valset` takes `abi::role::validators::Op`, signed by a node's identity
key (`env.signer()`); every other origin is refused.

- `Enroll { address, halo, invite }`: `address` is where peers dial the
  node, or none for a node that only dials out; `halo` is the host:port
  other members dial it at over QUIC, refused as `invalid_input` when its
  host is unspecified (`0.0.0.0`, `::`) or loopback. A member enrolling
  again only moves its addresses. A new key joins as a Resident while the door is open, or with an
  `Invite`: a `Grant { network, nonce, expires }` that names this network,
  has not expired, and is signed under `INVITE_NAMESPACE` over
  `grant.preimage()` by a current validator. Each (issuer, nonce) admits
  once. The door starts closed, and the member cap holds.
- `SetHalo { halo }`: a member moves its halo address, under the same rule.
- `Leave`: the signer leaves; the last validator cannot.
- `Vote(motion)`: a validator votes for `Promote` (a resident),
  `Demote` (a validator), `Remove` (a member), `Door { open }` (the other
  state) or `Programs { op }`. A motion passes at `n - (n-1)/3` of the
  current validators; a voter no longer a validator is dropped at the
  tally. The output is `Voted::Counted { votes, needed }` or
  `Voted::Enacted`. Removing a member clears the open motions that name
  it, and the last validator is neither demoted nor removed.

## Filling a role with another program

A role is an interface, not a program: any program whose `Op`, `Query` and
`Reply` start with the role's variants, byte for byte, can be bound to it
in the founding's `[roles]`. `crates/sdk/conformance` checks one (its
README has the steps). What the kernel does with the answers: identity's
`RegisterProgram` is idempotent and only the system sends it, and refusing
it fails a founding or undoes a later admission; a refusal of
`Account(key)` or `OfProgram(program)` rejects the frame or the message.
The programs role's `At(height)` answers every program that runs at that height; one
it leaves out is dropped. A refusal of validators' `Members` stops the
node.
