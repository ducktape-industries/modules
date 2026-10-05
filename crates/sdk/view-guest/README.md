# Writing a view

A view is a wasm32 cdylib on `ducktape-view-guest`. It builds a widget tree the host
lays out and draws, hears meaning-level events back, and asks the host for
data through a fixed table of methods. This page is the whole surface.

Read `crates/app/example-view/src/lib.rs` first: one file, the smallest view
that uses every core feature once (a stream followed, a listing read, a
bound field, a write, a refusal shown, a restore), with a comment on every
piece and its tests under it. It is built and tested with the workspace and
ships in no network. This page is what that file leans on.

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
rmp-serde = { git = "https://github.com/ducktape-industries/gpui-pre", rev = "3207fbe59e7badc80ab247c10314adf9f318ad5e" }
gpui-pre-scheduler = { git = "https://github.com/ducktape-industries/gpui-pre", rev = "3207fbe59e7badc80ab247c10314adf9f318ad5e" }
gpui-pre-zlog = { git = "https://github.com/ducktape-industries/gpui-pre", rev = "3207fbe59e7badc80ab247c10314adf9f318ad5e" }
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

`prelude` is every SDK name a view file writes, in one import: gpui's names
(`Render`, `Window`, `Context`, `IntoElement`, `div`, ...), the view's own
(`View`, `Loadable`, `Task`, `Host`, `Error`, `TextField`, `Theme`,
`export_view!`), the `design` module, and the `methods` module whole
(`HostSession`, `Changes`, `Submit`, `Capability`, `Session`, ...). A
program's types come from the program's crate, and an explicit import wins
over the glob (`use forge::Query;`). `View` asks for `Default`, `Serialize`
and `Deserialize`, which is why `serde` is a dependency. It builds with
`cargo build --target wasm32-unknown-unknown`. `crate-type = ["cdylib"]` is
all a view needs: no crate links a view as a library, and its unit tests
run without `rlib`.

## What a view is made of

What the six first-party views write, by how many of them write it. Start
with these; the rest of this page is reference.

| A view writes | Views | What it is |
|---|---|---|
| `impl View`: `NAME`, `CAPABILITIES`, `TARGETS`, `attach` | 6/6 | What the host reads before the view runs, and where what it follows starts ("The `View` trait"). |
| `cx.follow::<D>(request, \|view, item, cx\| ..).detach()` | 6/6 | A host stream: the session, a program's blocks, the clock. One call a stream, in `attach`. |
| `cx.log_refused(what, &refusal)` | 6/6 | A refusal nothing on screen waits for, to the host's log under the view's `NAME`. It asks `host.log`, so the view declares `Capability::Host`. |
| `Loadable<T>`; `cx.load(self, work, \|view\| &mut view.slot)` | 6/6; 5/6 | An answer and its states, and the read that fills it. |
| `cx.land(work, \|view, answer, cx\| ..)` | 6/6 | The answer to your own closure: a write, or a read that does more than fill a slot. |
| `host.query(ask)`, `host.query_all(\|after\| ask)` | 5/6 | A typed question to a program: one page, or the listing whole. |
| `cx.notify()` | 6/6 | The view decides when it renders ("When a view renders"). |
| `cx.listener(\|view, event, window, cx\| ..)` | 6/6 | A method of the view as a handler: a press (`&ClickEvent`), Enter in a field (`&()`), a key. gpui's. |
| `let theme = *cx.global::<Theme>();` | 6/6 | The host's colors, light or dark: gpui's `cx.theme()`. The first line of a render; `design`'s functions take `&theme`. |
| `design::space`, `design::size`, `design::text` | 6/6 | The spacing, control and type tokens every screen is laid out in. |
| `design::empty_state(id, title, detail, &theme)` | 6/6 | What a list says when it has no rows. |
| `design::badge(..)` | 6/6 | A small labelled state: "agent", "suspended". |
| `design::refused(name, message, &theme, retry)` | 5/6 | A refused read: the reason and a Retry, under the ids `<name>-refused` and `<name>-retry`. |
| `design::composite(..)` | 5/6 | A group one Tab stop holds and the arrows move in: a tab bar, a list of rows. |
| `TextField` and `Input::new(id, &self.field, label)` | 5/6 | A text field bound to the view's state ("What is gpui and what is ours"). |
| `.focusable()`, `.role(..)`, `.aria_disabled(..)` | 5/6 | What a pressable `div` owes the keyboard and a screen reader; the test host fails a frame that lacks one. |
| `cx.processor(\|view, item, window, cx\| ..)` | 5/6 | `cx.listener` for a callback that returns a value: a list's row builder. gpui's. |
| `design::button(id, label, &theme, click).enabled(..)` | 3/6 | A button with its role, Tab stop and disabled state done. |
| `cx.new(..)`, child entities | 1/6 | State and a render of its own inside a view ("Child entities"). |

Three patterns the views share:

- **Who is reading.** `cx.follow::<HostSession>` hands a `Session` per
  change; `account` is `None` both before the host resolved it and for a key
  that holds none. A view tells them apart by what it shows: its slot stays
  `Idle` until the first session item, and a read for no account answers an
  empty list. What a session change means is each view's (chat reopens,
  settings resets a form), so the view compares and decides.
- **A program's type in the state.** The snapshot is serde; a program's
  type may derive borsh only (identity's `Account` holds abi types that do).
  A view keeps a row struct of what the screen shows, folds the program's
  answer into it, and saves a borsh-only field as its bytes with
  `#[serde(with = "ducktape_view_guest::borsh_bytes")]`. The fold also keeps
  the snapshot to what is drawn.
- **A write.** `host.ask::<Submit<P>>(op)` sends the op; `cx.land` hands the
  answer to a closure that clears the form or keeps the program's reason.
  The view keeps the task (`Option<Task<()>>` under `#[serde(skip)]`) for as
  long as the button reads busy.

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
- A text field is bound from birth: `Input::new(id, &self.name, label)` and
  `Textarea::new(id, &self.body, label)` take the `TextField` of the view
  they show. The host owns the editing; what is typed lands in the field
  (read it with `self.name.text()`) and renders the view, with no listener
  written for it. `.on_change(cx.listener(..))` is for a view that does more
  on a change, and runs after the field took it. A `TextField` is a handle,
  as gpui's `Entity<Editor>` is: a `Clone` of it is the same field (so
  `vec![Form::default(); n]` is one field `n` times), while equality and the
  snapshot are by value. `reset(text)` starts a new document.
  A view that builds its own editor on a field has the rest: `tokens()` and
  `reset_with_tokens(text, tokens)` for its atomic spans, `generation()` and
  `apply(&change)` for the host's word, and `Window::dispatch(command)` to
  send what `replace` and `replace_all` ask.
- A control is named from birth: `Input::new(id, &field, label)`,
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
sealed, so a view cannot invent a kind. A view speaks to the host in four
ways:

```rust
// a stream: an item per change, for as long as the view runs
cx.follow::<Changes<valset::Valset>>((), |view, change, cx| ..).detach();
// one question, one answer
let status = cx.host().ask::<ChainStatus>(()).await?;
// a question to a program, typed by its reply
let accounts = cx.host().query(identity::ask::List { page }).await?;
// something said, with no answer waited for
cx.host().notify::<HostBadge>(3);
```

`cx.follow` is `host.subscribe::<D>(request)`, the raw stream, driven for
the view; a view that drives a stream itself (`cx.spawn`) asks for the
stream.

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
(`src/view.rs`) hold an ask's states and snapshot `Loading`
as `Idle`.

A cursored listing is read one of two ways. `host.query_all(|after| ask)`
follows it to its end: the closure is handed each page's cursor (`None`
first) and puts it in the question, and the rows of every page come back as
one list. It is for a list that is whole by nature (a roster, a settings
list), which a screen searches, counts or draws all of, and for an ask
answered with a `PageResponse`; `host::all_pages` is the same walk over a
page-asking closure of your own, for a listing in another shape. `Paged<T>` (`src/paged.rs`) holds it
a page at a time: for a history. It is an entity built from the same
page-asking closure; the `uniform_list` that draws it hands it the rows it
lowers (`show`), and the next page is asked for when they reach past the rows
held, one read out at a time. The list has `count()` rows: the rows held and,
while the listing goes on, one more that the view draws as loading. When a
`Change` touches the listing, `reread` reads the pages held again from the
first (cursors do not outlive a write), not the whole listing. Dropping the
entity cancels the read on its way. Both follow one rule for a cursor the
program refuses `stale` (it was handed out before a write): the read starts
over from the first page, and the refusal is never shown. Any other refusal
is the answer.

`Session` (`methods.rs`, `cx.follow::<HostSession>`) is what every view is handed:
`connected`, `chain_id`, `signer` (the seated key, hex), `account` (its
account number, `None` until the host resolves one), `endpoint`; an item per
change. Read "who am I" from `account`; no view asks identity for it.

## When a view renders, and who decides

A view renders on the tick after it called `cx.notify()`, once however many
times it called it, and at no other time but its first frame and a host that
lost its tree. The view decides: nothing in the SDK notifies on its behalf
for something only the view can judge.

- `cx.follow::<D>(request, each)` subscribes to the host stream `D` and runs
  `each` per item, a refused item included, and does not notify: `each` calls
  `cx.notify()` when the item changed what the view shows. An item that only
  starts a read draws nothing; the read draws when it lands. The `Task` it
  returns is the subscription, as gpui's `Subscription` is: `.detach()` it to
  follow for as long as the view runs, or keep it and drop it to stop.
  `cx.log_refused(what, &refusal)` writes a refusal nothing on screen waits
  for to the host's log, under the view's `NAME`.
- `cx.load(self, work, |view| &mut view.slot)` reads `work` into a `Loadable`
  slot. What the slot shows stays until the answer lands: a value stays on
  screen (`Loadable::Reloading`), a slot with nothing to show (`Idle`,
  `Failed`) shows `Loading`. It notifies when, and only when, it changes what
  the slot shows: a read that lands the value already there draws nothing. A
  read that must blank what is shown first says so in its own line,
  `self.slot = Loadable::Idle;`. The read lives in the slot: replacing or
  dropping the slot cancels it, so a newer read supersedes an older one.
- `cx.land(work, |view, answer, cx| ..)` hands the answer, the value or the
  refusal, to the closure and does nothing else; the closure notifies if it
  moved anything. It is for a write, or a read that does more than fill a
  slot: in gpui's words, `cx.spawn(async move |this, cx| { let answer =
  work.await; this.update(cx, |view, cx| ..) })` in one line. `.detach()` the
  task it returns, or keep it beside what it reads: a newer one stored in its
  place cancels the older.
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

A view composes as a gpui view does, and one of the six does it.
`cx.new(|cx| Sidebar::new(cx))` builds a child entity from any `'static`
state, with no `View` of its own; a child that implements `Render` is a
child element (`.child(self.sidebar.clone())`) or a tooltip's content.
`entity.update(cx, |sidebar, cx| ..)` runs on it from a listener, another
entity or a task, and its `cx.notify()` renders the view. `cx.emit(event)`
tells what `cx.subscribe(&child, ..)`s, and `cx.observe(&child, ..)` hears
its notify, for as long as the `Subscription` is kept. The snapshot is the
root's serde alone: keep a child `#[serde(skip)]` in an `Option`, beside its
subscriptions, and build both in `attach`, so a restore builds them again.
A child is not a `View`: it logs with `cx.host().log(..)`.
`tests/gpui_entities.rs` is the whole of it.

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
press leaves a dialog's layer. A field's text is the host's: it takes a
frame's text when the field is new or its generation moved, and tells the
view when what it took differs; `simulate_input` types into that text; a
field drawn from a `TextField` made in `render` is a new generation every
frame, and loses what is typed. The view opens in a pane the size the
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
