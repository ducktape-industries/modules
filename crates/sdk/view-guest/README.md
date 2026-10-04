# Writing a view

A view is a wasm32 cdylib on `ducktape-view-guest`. It builds a widget tree the host
lays out and draws, hears meaning-level events back, and asks the host for
data through a fixed table of methods. This page is the whole surface.

## Depending on it

A view crate outside this repository, with this repository checked out
beside it as `modules`, has this `Cargo.toml`:

```toml
[package]
name = "hello-view"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
ducktape-view-guest = { path = "../modules/crates/sdk/view-guest" }
serde = { version = "1", features = ["derive"] }

[patch.crates-io]
rmp-serde = { git = "https://github.com/ducktape-industries/gpui-pre", rev = "ec44f1bba7cae8ec040ec8180e274e0718a30281" }
gpui-pre-scheduler = { git = "https://github.com/ducktape-industries/gpui-pre", rev = "ec44f1bba7cae8ec040ec8180e274e0718a30281" }
gpui-pre-zlog = { git = "https://github.com/ducktape-industries/gpui-pre", rev = "ec44f1bba7cae8ec040ec8180e274e0718a30281" }
```

The `[patch.crates-io]` lines are this workspace's own, at the rev its
`Cargo.toml` pins: the SDK builds against the gpui fork's copies of those
three crates (crates.io's `rmp-serde` has no `typed` feature, and without
the fork's scheduler a view does not build for wasm32), and cargo reads
`[patch]` only from the manifest being built, so a crate outside the
workspace repeats them. A view inside the workspace
writes `ducktape-view-guest.workspace = true` and no patch.

The smallest view, `src/lib.rs`:

```rust
use ducktape_view_guest::prelude::*;
use ducktape_view_guest::{View, export_view};
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
struct Hello;

impl View for Hello {
    const NAME: &'static str = "hello";
}

impl Render for Hello {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child("hello")
    }
}

export_view!(Hello);
```

`prelude` holds what a render body names (`Render`, `Window`, `Context`,
`IntoElement`, `div`, ...); `View` and `export_view!` are imported beside
it. `View` asks for `Default`, `Serialize` and `Deserialize`, which is why
`serde` is a dependency. It builds with
`cargo build --target wasm32-unknown-unknown`.

## What is gpui and what is ours

The rule: we re-implement only what touches the host or the wire; everything
else is the gpui fork `Cargo.toml` pins (`gpui-pre`, at the rev it names),
re-exported unchanged. The `pub use gpui::{…}` at the top of `src/lib.rs`
is the gpui list (`px`, `rems`, `Hsla`, `StyleRefinement`, `Styled`,
`ElementId`, `SharedString`, the `*Event` types, `Role`, ...).
`use ducktape_view_guest::prelude::*` brings in what a gpui view file
imports: elements, styles, events and the traits whose methods they call.
Ours, defined in this crate:

- `App`, `AppContext`, `AsyncApp`, `Context`, `Entity`, `WeakEntity`
  (`src/context.rs`), `Task` (`src/executor.rs`), `Window`
  (`src/window.rs`): the entity graph and the tick loop run inside the
  guest, so the host never sees a closure. Its `EventEmitter` and
  `Subscription` are gpui's.
- `div`, `uniform_list`, `list`, `Input`, `Textarea`, `img`/`Img`, `svg`/`Svg`,
  `canvas`/`Canvas`, `InteractiveText`/`StyledText`, `sensor`,
  `resize_handle`, `modal_overlay` (`src/element.rs`, `src/list.rs`,
  `src/primitives/`, `src/rich_text.rs`, `src/behavior.rs`):
  each lowers to a `view_wire::Node`, with handlers kept guest-side and
  crossed as routes: a listener's number, kept while its element is lowered
  (`view_wire::Event`). `list` is gpui's but for its first argument:
  styled, with nothing to press or hover (a `div` around it carries those),
  and `list(id, state, ..)` takes an id where gpui's `list(state, ..)` takes
  none, since the host files a list's state and its rows by path (below);
  `canvas` is drawn by calls (`rect`, `circle`, `line`), since gpui's paint
  closures cannot cross.
- An id is unique among the ids under its nearest identified ancestor, as
  gpui's are (`view_wire::identity`). A `list`, a `uniform_list` and each of
  their rows are scopes of their own: a list is filed under the id its
  author gives it (`list(id, state, ..)`, `uniform_list(id, ..)`), a row
  under its own id, else its index, and the ids inside it under the row, so
  rows need no index formatted into their ids. The host keeps what it holds
  for a view by that path (a list's scroll, a field's text and selection,
  focus), so a list's id is what brings the state in its rows back when the
  view is instantiated again, and what a widget target is named through
  (`focus_path(["thread", "rows", key, "edit"])`). Children are not scopes:
  a row of data built with `children(..)` carries its own id (its item's
  key). Two rows named by one key, two lists given one id, or any id written
  twice in a scope are refused by the host, naming the id and its scope; a
  view's test fails in the same words, since `TestAppContext` holds every
  frame to the host's sanitizer.
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
`P::NAME` is the target they carry. A `Changes<P>` item is the block's
`Change`: its height and the keys it wrote; `change.touches::<P, _>(&query)`
says whether a read moved, from the tables the program's `Query` declares
with `#[reads(..)]` (`program::Reads`; another program's tables it reads are
declared with that program named first), so a view re-reads what the block
touched and maps no key itself (`None` is a reopened link: read everything
again). A
role a view follows without linking
its program is `program::role::Identity`. Every refusal is the module SDK's `Error`
(`code` token, `message`), one type end to end: a program's codes are
`error::code`, the host's own are `methods::refusal`. `Loadable<T>` + `cx.load`
and `cx.reload` (`src/view.rs`) hold an ask's states and snapshot `Loading`
as `Idle`.

A cursored listing is read one of two ways. `host::all_pages` follows it to its
end: for a list that is whole by nature (a roster, a settings list), which a
screen searches, counts or draws all of. `Paged<T>` (`src/paged.rs`) holds it
a page at a time: for a history. It is an entity built from the same
page-asking closure; the `uniform_list` that draws it hands it the rows it
lowers (`show`), and the next page is asked for when they reach past the rows
held, one read out at a time. The list has `count()` rows: the rows held and,
while the listing goes on, one more that the view draws as loading. When a
`Change` touches the listing, `reread` reads the pages held again from the
first (cursors do not outlive a write), not the whole listing. Dropping the
entity cancels the read on its way.

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
- A `Paged` notifies when a page lands, and when a re-read lands rows that
  differ from the ones held or a refusal.

Native tests catch the one-way mistake: a change to the view's serialized
state without `cx.notify()` panics (debug builds). They cannot see a
`#[serde(skip)]` field or a child entity's state: notify for those yourself.
`TestAppContext::renders()` and `ticks()` count what the view did since it
opened, so a test can pin that an event draws nothing.

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
as named MessagePack (`src/snapshot.rs`), refused while work is
pending; a host holds it to `view_wire::MAX_SNAPSHOT_BYTES` (8 MiB,
`view-wire/src/snapshot.rs`). Derive `Default`/`Serialize`/`Deserialize`
and keep `Task`s out of the state (`Loadable` does). A module's type that
derives only borsh goes in the state as its bytes:
`#[serde(with = "ducktape_view_guest::borsh_bytes")]` on the field
(`src/borsh_bytes.rs`).

## Child entities

A view composes as a gpui view does. `cx.new(|cx| Sidebar::new(cx))` builds
a child entity from any `'static` state, with no `View` of its own. A child
that implements `Render` is a child element (`.child(self.sidebar.clone())`)
and a tooltip's content (`.tooltip(|_, cx| cx.new(|_| Tip("Help")).into())`).
`entity.update(cx, |sidebar, cx| sidebar.select(i, cx))` runs on it from a
listener's `App`, an entity's `Context` or a task's `AsyncApp` (each an
`AppContext`); a test does it between ticks with
`TestAppContext::update(&entity, ..)`. A child's `cx.notify()` renders the
view, as the root's does. A child that implements `EventEmitter<E>` tells
what `cx.subscribe(&child, ..)`s with `cx.emit(event)`, and
`cx.observe(&child, ..)` hears its `cx.notify()`: both are heard once the
update that raised them is done, so a parent may update the child back, and
only while the `Subscription` they return is kept.

The snapshot is the root's serde alone. Keep a child `#[serde(skip)]` in an
`Option`, beside its subscriptions, and build both in `attach`, from the
root's state, so a restore builds them again:

```rust
#[derive(Default, Serialize, Deserialize)]
struct Shell {
    picked: Option<usize>,
    #[serde(skip)]
    sidebar: Option<Entity<Sidebar>>,
    #[serde(skip)]
    subscriptions: Vec<Subscription>,
}
impl View for Shell {
    const NAME: &'static str = "Shell";
    fn attach(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let picked = self.picked;
        let sidebar = cx.new(|cx| Sidebar::new(picked, cx));
        self.subscriptions.push(cx.subscribe(&sidebar, |shell, _, Selected(row): &Selected, cx| {
            shell.picked = Some(*row);
            cx.notify();
        }));
        self.sidebar = Some(sidebar);
    }
}
```

`tests/gpui_entities.rs` is the whole of it, `Sidebar` included.

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
node off that path fails the test. A click goes out from the node pressed
through every node around it with a click, as gpui passes it, until one
consumes it (`consumes_click`), hides what is behind it (`occlude`), or the
press leaves a dialog's layer. The view opens in a pane the size the
app gives a new window (`testing::VIEWPORT`; `simulate_resize` moves it,
before or after the open), reads it as `Window::viewport_size()`, and a
list's first frame holds the rows that fill it; `simulate_viewport(rows)`
or `simulate_range(key, range)` then show it the rows the host shows.
The reader's offset is UTC until `simulate_offset(minutes)`. A frame
carries at most
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
`crates/app/chat-view/src/tests/mod.rs`) writes each screen's whole frame (its
tree and the styles it names, `TestAppContext::whole_frame`) as JSON under
`target/`; the app renders those with `dev/screens/chat-screens.sh
FIXTURES_DIR OUTPUT_DIR` (`ducktape-app --render-tree`, debug builds).
