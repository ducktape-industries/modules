# Writing a view

A view is a wasm32 cdylib on `view-guest`. It builds a widget tree the host
lays out and draws, hears meaning-level events back, and asks the host for
data through a fixed table of methods. This page is the whole surface.

## What is gpui and what is ours

The rule: we re-implement only what touches the host or the wire; everything
else is the gpui fork `Cargo.toml` pins (`gpui-pre`, at the rev it names),
re-exported unchanged. The `pub use gpui::{…}` at the top of `src/lib.rs`
is the gpui list (`px`, `rems`, `Hsla`, `StyleRefinement`, `Styled`,
`ElementId`, `SharedString`, the `*Event` types, `Role`, ...).
`use ducktape_view_guest::prelude::*` brings in what a gpui view file
imports: elements, styles, events and the traits whose methods they call.
Ours, defined in this crate:

- `App`, `AsyncApp`, `Context`, `Entity`, `WeakEntity` (`src/context.rs`),
  `Task` (`src/executor.rs`), `Window` (`src/window.rs`): the entity graph
  and the tick loop run inside the guest, so the host never sees a closure.
- `div`, `uniform_list`, `list`, `Input`, `Textarea`, `img`/`Img`, `svg`/`Svg`,
  `canvas`/`Canvas`, `InteractiveText`/`StyledText`, `sensor`,
  `resize_handle`, `modal_overlay` (`src/element.rs`, `src/list.rs`,
  `src/primitives/`, `src/rich_text.rs`, `src/behavior.rs`):
  each lowers to a `view_wire::Node`, with handlers kept guest-side and
  crossed as per-frame indices. `list` is gpui's: styled, with nothing to
  press, name or hover (a `div` around it carries those); `canvas` is drawn
  by calls (`rect`, `circle`, `line`), since gpui's paint closures cannot
  cross.
- A control is named from birth: `Input::new(id, label)`,
  `Textarea::new(id, &field, label)`, `modal_overlay(id, label, …)`, and
  `design`'s `segmented`, `icon_button` and `divider` take the words
  assistive technology reads; the audit catches one given none.
- `InteractiveElement`, `StatefulInteractiveElement`, `FocusHandle`,
  `ScrollHandle` (`src/interactivity.rs`): listeners and focus become wire
  routes; `track_scroll` lets a handle move its scroller. `Window::focus(id)`
  names an element by its own id, and the SDK sends the host the whole path
  the frame it renders gives it (`focus_path` when two scopes hold the id);
  `focus_next`/`focus_prev` move as Tab does.

## The methods

`view-wire/src/methods.rs` is the one list of what a view may ask for
(`methods::ALL`): each kind a marker type naming its request and reply (borsh both ways;
`host.widget` alone is MessagePack, because it names tree ids). The trait is
sealed, so a view cannot invent a kind. Three verbs on `Host` (`src/host.rs`):

```rust
let status = cx.host().ask::<ChainStatus>(()).await?;
let mut live = cx.host().subscribe::<Changes<valset::Valset>>(());
cx.host().notify::<methods::HostBadge>(3);
```

A program's query is asked alone, typed by the reply that answers it:
`cx.host().query(identity::ask::List { page })` is `module.query` with the
program's own bytes (`identity::Query::List`), answered with the page of
accounts and nothing else. A program's `Query` derives `program::Ask`, which
writes those types into its `ask` module; a reply to another question is
refused as `unexpected_reply`, so a view has no arm for the others.

A node program is addressed by its own type, the one that implements
`program::Program` (`methods::Program` here) beside its `guest::Module`:
`Query<P>`, `Submit<P>` and `Changes<P>` are its three methods, and
`P::NAME` is the target they carry. A role a view follows without linking
its program is `program::role::Identity`. Every refusal is the module SDK's `Error`
(`code` token, `message`), one type end to end: a program's codes are
`error::code`, the host's own are `methods::refusal`. `Loadable<T>` + `cx.load`
and `cx.reload` (`src/view.rs`) hold an ask's states and snapshot `Loading`
as `Idle`.

`Session` (`methods.rs`, `subscribe::<HostSession>`) is what every view is handed:
`connected`, `chain_id`, `signer` (the seated key, hex), `account` (its
account number, `None` until the host resolves one), `endpoint`; an item per
change. Read "who am I" from `account`; no view asks identity for it.

## When a view renders, and who decides

A view renders on the tick after it called `cx.notify()`, once however many
times it called it, and at no other time but its first frame and a host that
lost its tree. The view decides: nothing in the SDK notifies on its behalf
for something only the view can judge.

- `cx.for_each(stream, each)` runs `each` per item and does not notify:
  `each` calls `cx.notify()` when the item changed what the view shows. An
  item that only starts a read draws nothing; the read draws when it lands.
- `cx.load(work, at)` fills a `Loadable` slot and notifies when it lands
  (`Loading` to `Ready` or `Failed` is always a change).
- `cx.reload(&mut slot, work, at)` reads a slot again. The value on screen
  stays (`Loadable::Reloading`) until the answer lands; it notifies only if
  the answer differs from it, or is a refusal, which lands `Failed`. The read
  lives in the slot: replacing or dropping the slot cancels it, so a newer
  read supersedes an older one.
- `cx.refresh(work, land)` hands the answer, the value or the refusal, to
  `land`, which notifies if it moved anything. Keep the task it returns
  beside what it reads: a newer one stored in its place cancels the older.
- A list the host scrolls renders when it needs rows it does not hold.

Native tests catch the one-way mistake: a change to the view's serialized
state without `cx.notify()` panics (debug builds). They cannot see a
`#[serde(skip)]` field or state outside the view (`design::set_utc_offset`):
notify for those yourself. `TestAppContext::renders()` and `ticks()` count
what the view did since it opened, so a test can pin that an event draws
nothing.

## The `View` trait

`View` (`src/view.rs`) is everything the host reads about a view, how it is
built and how it joins the host: `NAME` (the tab and catalog name), `DESCRIPTION` (one
catalog line, `""` by default), `CAPABILITIES` (the `methods::Capability`
halves of the kinds it asks through, `&[]` by default: a method whose
capability is not listed is refused `undeclared_capability`), `TARGETS` (the
programs it addresses with `op.submit`, `module.query` and `module.changes`,
by each `Program`'s `NAME`, `&[]` by default: a method naming another is
refused `undeclared_target`, and a view with `op` or `module` that names
none does not compile), `MIN_WINDOW_WIDTH` (the narrowest width in px it works at, default 480,
`1..=8192` or it does not compile: the app never lays it out narrower, and
a narrower window scrolls it sideways), `new(window, cx)`, the state on
first mount (`Default::default()` unless the view says otherwise), and
`attach(window, cx)`, which runs on every mount, first or restored from a
snapshot, and starts what the view follows and reads. Subscribe in `attach`,
never in `new`: a restored view is not built again, and only what `attach`
starts follows the host after a redeploy. The snapshot is the view's own serde
as the wire's named MessagePack (`src/snapshot.rs`), refused while work is
pending; a host holds it to `view_wire::MAX_SNAPSHOT_BYTES` (8 MiB,
`view-wire/src/snapshot.rs`). Derive `Default`/`Serialize`/`Deserialize`
and keep `Task`s out of the state (`Loadable` does). A module's type that
derives only borsh goes in the state as its bytes:
`#[serde(with = "ducktape_view_guest::borsh_bytes")]` on the field
(`src/borsh_bytes.rs`).

## Exporting

`export_view!(View)` (`src/lib.rs`) writes the five wasm exports and the
manifest section `ducktape.view.manifest` from the trait's consts
(`view-wire/src/manifest.rs`: header, `NAME`, `DESCRIPTION`,
`CAPABILITIES`, `MIN_WINDOW_WIDTH` in decimal, `WIRE_ID`, `TARGETS`; the
line list is `manifest::LINES`, part of `schema.txt` and so of `WIRE_ID`).

The ABI gate (`view-wire/src/abi.rs`, `tools/check-view-abi.py`): exactly one
import, `ducktape_view.panicked`, and exactly five function exports,
`alloc`/`init`/`tick`/`snapshot`/`restore`. `make wasm-views` builds every
view in `VIEWS`, runs `wasm-opt`, checks the ABI and prints the size;
`make view-wasm-check`
proves nothing in `VIEW_LINKABLE` reaches `VIEW_FORBIDDEN`. Wire bytes are
pinned by `view-wire/tests/golden.rs`: a shape change fails it until the
fixtures are regenerated with `WIRE_GOLDEN_WRITE=1`, which moves `WIRE_ID`
(`view-wire/build.rs` hashes them); a host refuses a view built against
another.

## Testing

`testing::TestAppContext` (`src/testing/context.rs`) opens a view over a
`FakeHost` (`src/testing/fake_host.rs`) that answers by a request's shape.
An ask waits for its answer, so a test says what comes back:
`handle::<Method>`, `refuse`, or `never` for one that stays out; an ask
nothing answers fails the test, naming it. A subscription nothing feeds
stays open; `stream::<Method>()` is its feed, and a feed nobody hears
fails the test. `requests::<Method>()` is everything the view sent,
notifies too (`HostLog`, `LinkOpen`, `HostWidget`). Then
`simulate_click`, `simulate_input`, `simulate_event` and a typed
`simulate_*` per event; `texts`, `find`, `node(key).style()`/`.text()`/
`.children()`, `interactivity`, `last_frame`, and a `TickReport` per tick.
Keys go where the host sends them: `simulate_focus` (or a click, or the
view's own `Window::focus`) puts the keyboard on a node, and a key goes
down its focus path to the capture listeners and back up it; a key at a
node off that path fails the test. A list shows the rows the host shows:
its one measured row at first, then `simulate_viewport(rows)` or
`simulate_range(key, range)`. A frame carries at most
`view_wire::MAX_REQUESTS` requests; the rest go in the next.
Every frame the view sends is held to `view_wire::audit`: a fault panics with
its kind and key path, so each screen a test reaches is gated.
It holds the view to its `View::CAPABILITIES` and `View::TARGETS` as the app
does: a method whose capability the manifest leaves out panics with
`methods::refusal::UNDECLARED_CAPABILITY`, a node method naming a program
the targets leave out with `methods::refusal::UNDECLARED_TARGET`, the codes
the app refuses them with.
Screen export: a test gated on `*_SCREEN_EXPORT=1` (`FORGE_SCREEN_EXPORT`,
`crates/app/forge-view/src/screen_tests.rs`; `CHAT_SCREEN_EXPORT`,
`crates/app/chat-view/src/tests/mod.rs`) writes each screen's tree as JSON under
`target/`; the app renders those with `dev/screens/chat-screens.sh
FIXTURES_DIR OUTPUT_DIR` (`ducktape-app --render-tree`, debug builds).
