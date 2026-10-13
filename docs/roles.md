# Roles, accounts and updates

The kernel knows three modules by what they do, never by their ids. Genesis
binds each role to a founding module; every call the kernel makes goes to the
module bound to that role, speaking the role's interface in
`crates/sdk/abi` (`abi::role::{registry, validators, identity}`). The
modules in `crates/system/` are one implementation of each.

## The three roles

| Role | Bound here to | What the kernel does with it |
|---|---|---|
| `registry` | `module-registry` | at founding, writes its params (`role::registry::Genesis`: every founding module and view, and the founding hooks); at each block, before any frame, asks `At(height)` and admits, swaps or drops modules to match the answer, then asks `Hooks(height)` for the modules it tells of each frame the block carries ("Hooks", below) |
| `validators` | `valset` | at founding, writes its params (`role::validators::Genesis`: the founding validators and the member cap, the most members it may ever hold, 1 to `MAX_MEMBERS` = 128); at founding and on the last block of each epoch, asks `Validators` and `Members`: it seats the Validators for the next epoch and tracks the Members (a Resident follows the chain but never votes or proposes); a Members answer past the cap halts the network there. The node (`ducktape join`, `invite`, `vote`, `leave`) signs the role's `Op` with its identity key |
| `identity` | `identity` | as it admits a module (at founding, and later off the registry), executes `RegisterModule { module }` with the `System` origin; for each signed frame asks `Account(key)`; for each message and reply asks `OfModule(sender)` |

The founding file binds them (ducktape's `[roles]` table; qa's
`founding.toml` is a working one):

```toml
[roles]
registry = "module-registry"
validators = "valset"
identity = "identity"
```

All three are required. Founding refuses a role bound to a module the
founding does not list, and a node refuses a network that records no roles.
Every frame's `Env` carries the bindings (`env.roles`), so a module that asks
a role asks `ctx.env().roles.identity`, not a name it assumes; chat's roster
does this (`crates/app/chat/src/origin.rs`).

## Who a frame acts as

The host resolves the sender once per frame, through the identity role, and
hands it to the module as `Env.sender`:

- a signed frame acts as the account its key holds (`Account(key)`); a key
  that holds none still runs, as no one (`sender: None`), which is how
  identity's own `Create` works; a key whose account does not act (a
  suspended agent) is refused, and the host rejects the frame;
- a message or a reply from another module acts as that module's account
  (`OfModule(module)`);
- founding and the kernel's own calls act as the chain (`Root`).

A module reads it as `ctx.sender()`, which is `Principal::Account(number)`
or `Principal::Root` and refuses a frame that acts as no one, so no row a
module writes names a bare key. `Env::signer`, `sending_module` and
`sent_by` check the raw origin instead, where a module needs the key or the
module itself (forge requires a signed frame; chat's `:` namespace belongs to
the module its prefix names).

## Accounts

Everything that acts is an account in `identity`:
`Account { number, card, control }`. The `Card` is what it shows (name,
avatar, bio); the `Control` is who acts as it:

- `Person { keys }`: a person and the keys they hold.
- `Managed { manager, category, life }`: an agent. Its manager, a person,
  gives it keys and decides its `Life`: `Active { keys }`,
  `Suspended { keys }` (keys kept, frames refused) or `Revoked` (keys
  dropped, final).
- `Module { module }`: a module's account, registered by the kernel as it
  admits the module. It holds no keys; the module alone acts as it.

Who may do what (the acting account is `ctx.sender()`):

| Op | Who |
|---|---|
| `RegisterModule { module }` | the system (`Root` origin) alone; a module that has an account keeps it |
| `Create { name, scheme }` | a key that holds no account; it becomes a person's first key |
| `CreateAgent { name }` | a person; they become its manager |
| `AddKey` | a person's account: the new key signs the frame and a key already on the account consents. An agent's: its manager signs and the new key consents to itself. Never a module's or a revoked agent's |
| `RemoveKey` | a person removes their own key or a junior one, never their last. An agent's manager removes any of its keys |
| `SetName`, `SetProfile` | a person or a module on its own account; an agent's manager on the agent's (not once revoked) |
| `Suspend`, `Resume`, `Revoke` | the agent's manager |
| `SetAudience { account, audience }` | the agent's manager, not once revoked. It records whom the agent answers: `Manager`, `Accounts` (the manager and at most `MAX_AUDIENCE` = 64 others) or `Anyone`. The agent's runtime reads it with `Query::Audience`; nothing on the chain enforces it. An agent with no setting, or a revoked one, answers `Manager` |

The role's queries, which views and other modules ask too, are
`Account(key)`, `OfModule(module)`, `Profile(number)` and
`Profiles { after, limit }`. A `Profile` is `{ number, name, kind }`, where
`Kind` is `Person`, `Managed { manager, category, standing }` or
`Module(id)`. `Kind::badge` and `Kind::note` in `abi` turn it into what a view
shows beside a name ("Agent · managed by Dev", "Module · chat", "suspended"),
the one place that happens.

## Updating a module

`module-registry` holds the roster and the changes scheduled to it:

1. `Publish { body }` stores the new code as a blob and returns its id.
2. `Schedule(Scheduled { height, change: Change::Set(Entry { program, code, params }) })`
   lands it at a later height.
3. At that height the kernel's `At(height)` sees the new code. Under an id
   already running, the code is swapped: the state, the account and the id
   stay, and `init` does not run again. Under a new id, the module is
   admitted (its `init` gets `params`) and registered with identity.

`Change::Remove` drops a module; `Cancel` withdraws a scheduled change. A
role's module updates the same way, under its bound id; the binding itself
is fixed at genesis.

## Hooks

A hook is a module the kernel tells of every frame a block carries. The
registry lists the hooks: the founding file's top-level `hooks = [...]`
seeds them, `Change::SetHook(id)` makes a running module one from its
height, and `Change::RemoveHook(id)` stops it there. `Change::Remove` of a
module drops its hook with it. At each block the kernel asks
`Hooks(height)`; a registry that refuses or answers something else names
no hooks.

After each frame runs, the kernel calls each hook with a `hook::Record`:
the block height, the frame's place in the block, its signer, the account
it acted as, its target and whether it applied. A refused frame is told
too. The call's origin is the kernel itself (`Origin::Root`, from
`Env::kernel()` in `guest`), and `guest::hook` refuses any other. A hook
never changes a frame: its receipt stays as it ran, a hook that refuses,
traps, runs out of fuel or emits a message keeps none of its writes, and
the block goes on. `activity` (`crates/app/activity`) is the hook this
repo founds: it indexes who signed what, by key and by account, and keeps
the newest 1,000 frames of each.

Who may schedule or cancel (publishing a blob is open to anyone):

- The validators' vote, any change: `Env::authority()` in `guest` admits
  only a message from the program bound to the validators role, and
  `valset` sends one only when the vote passes a `Motion::Registry { op }`:
  the enacting vote emits `op` to the registry in the same frame, and a
  refusal there fails that vote.
- A maintainer: a signed frame whose account holds a `Grant` of the id
  (`Scope::Module(id)`, which may run nothing yet) or of every module
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
made to that id with it, and its removal of an id the id's `Module`
grants too. The vote's removal of an id that holds nothing at its height
takes those along and waits as no entry, and its cancel of a change
already gone is no error, so a motion never sits one vote short of a
done thing.

## Membership

`valset` takes `abi::role::validators::Op`, signed by a node's identity
key (`env.signer()`); every other origin is refused.

- `Enroll { address, invite }`: `address` is where peers dial the node,
  or none for a node that only dials out. A member enrolling again only
  moves its address. A new key joins as a Resident while the door is open, or with an
  `Invite`: a `Grant { network, nonce, expires }` that names this network,
  has not expired, and is signed under `INVITE_NAMESPACE` over
  `grant.preimage()` by a current validator. Each (issuer, nonce) admits
  once. The door starts closed, and the member cap holds.
- `Leave`: the signer leaves; the last validator cannot.
- `Vote(motion)`: a validator votes for `Promote` (a resident),
  `Demote` (a validator), `Remove` (a member), `Door { open }` (the other
  state) or `Registry { op }`. A motion passes at `n - (n-1)/3` of the
  current validators; a voter no longer a validator is dropped at the
  tally. The output is `Voted::Counted { votes, needed }` or
  `Voted::Enacted`. Removing a member clears the open motions that name
  it, and the last validator is neither demoted nor removed.

## Filling a role with another module

A role is an interface, not a module: any module whose `Op`, `Query` and
`Reply` start with the role's variants, byte for byte, can be bound to it
in the founding's `[roles]`. `crates/sdk/conformance` checks one (its
README has the steps). What the kernel does with the answers: identity's
`RegisterModule` is idempotent and only the system sends it, and refusing
it fails a founding or undoes a later admission; a refusal of
`Account(key)` or `OfModule(module)` rejects the frame or the message.
Registry's `At(height)` answers every module that runs at that height; one
it leaves out is dropped. A refusal of validators' `Members` stops the
node.
