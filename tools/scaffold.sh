#!/bin/sh
# `make new-module NAME=x` / `make new-view NAME=x-view`: a module in
# valset's shape (types and `describe` in lib.rs, the module in program.rs,
# its rules over `store` in rules.rs, the marker a view names it by in
# view.rs behind `view`, a native test over `guest::MockHost` in tests.rs;
# its wasm exports behind `module`, its describe module behind `describe`)
# or a view in members-view's shape (links its module with `view` on and
# `module` off, `export_view!`, one screen test), registered in the Makefile
# and the workspace. `make scaffold-check` builds and tests both. Run from
# the repo root.
#   tools/scaffold.sh module <name> | view <name>-view
set -eu
kind=$1
name=$2
dir=crates/app/$name
case "$name" in
    *[!a-z0-9-]* | -* | *-) echo "$name: a crate name is lowercase words joined by '-'" >&2; exit 1 ;;
esac
test ! -e "$dir" || { echo "$dir exists" >&2; exit 1; }
snake=$(echo "$name" | tr - _)

# The lines registration edits, each of which must be there exactly once;
# every one is checked before anything is written, so a scaffold never
# half-registers.
MEMBERS='^    "crates/lib/gitcore",$'
DEPS='^forge = { path = "crates/app/forge" }$'
anchor() { # <file> <pattern>
    test "$(grep -c "$2" "$1")" = 1 || { echo "$1: no single line matches '$2'; nothing was written. Fix the anchor in $0." >&2; exit 1; }
}
# <file> <sed script>: in place, the same with GNU and BSD sed (macOS has
# no bare `-i`, nor `\n` in a replacement: a new line there is `\` and a
# real line break).
edit() {
    sed -e "$2" "$1" > "$1.new" || { rm -f "$1.new"; exit 1; }
    mv "$1.new" "$1"
}
register() { # <Makefile list> <name>
    edit Makefile "s/^$1 := .*/& $2/"
    edit Cargo.toml "s|$MEMBERS|    \"crates/app/$2\",\\
&|"
}

module() {
    anchor Makefile '^PROGRAMS := '
    anchor Makefile '^VIEW_LINKABLE := '
    anchor Cargo.toml "$MEMBERS"
    anchor Cargo.toml "$DEPS"
    # The module type: TitleCase of the module name.
    title=$(echo "$name" | awk -F- '{ for (i = 1; i <= NF; i++) printf "%s%s", toupper(substr($i, 1, 1)), substr($i, 2) }')
    mkdir -p "$dir/src"
    cat > "$dir/Cargo.toml" <<EOF
[package]
name = "$name"
version.workspace = true
edition.workspace = true

# The types and rules are always built; a view links them with \`module\`
# off. \`module\` adds its wasm exports (\`guest::export!\`), which only its
# own wasm build turns on.
[lib]
crate-type = ["cdylib", "rlib"]

[features]
module = []
# \`view\` adds the marker a view names this module by.
view = ["dep:ducktape-view-guest"]
# \`describe\` makes this crate's wasm build the \`ducktape.describe\` module:
# the \`describe\` export alone, no module, no imports.
describe = []

[dependencies]
ducktape-view-guest = { workspace = true, optional = true }
abi = { workspace = true }
borsh = { workspace = true }
describe = { workspace = true }
guest = { workspace = true }
store = { workspace = true }
EOF
    cat > "$dir/src/lib.rs" <<EOF
//! The \`$name\` module: one count, to be replaced by what it keeps. The
//! types, rules and [\`$title\`] module are always built; a view links them
//! with \`module\` off. The layout, in reading order:
//!
//! - \`lib.rs\` (here): the types on the wire, all borsh, and [\`describe()\`]
//! - \`program.rs\`: [\`$title\`], the module: one match over every op and
//!   one over every query
//! - \`rules.rs\`: what each op checks and writes, over \`store\`
//! - \`view.rs\` (feature \`view\`): the marker \`$name-view\` names this module by
//! - \`tests.rs\`: the module natively over \`guest::MockHost\`
mod program;
mod rules;
#[cfg(test)]
mod tests;
#[cfg(feature = "view")]
pub mod view;

pub use program::$title;

use borsh::{BorshDeserialize, BorshSerialize};

pub const MODULE: &str = "$name";

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Op {
    Bump { by: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Query {
    Count,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Reply {
    Count(u64),
}

/// An op as a person reads it: a title and its fields. The source of the
/// \`ducktape.describe\` module this module ships (\`make wasm-describes\`).
pub fn describe(op: &Op) -> describe::Description {
    use describe::{Value, field};
    let (title, fields) = match op {
        Op::Bump { by } => (
            format!("Bump by {by}"),
            vec![field("by", Value::count(*by))],
        ),
    };
    describe::Description { title, fields }
}

describe::export!(Op, describe);

/// Old op bytes are described with the current code (\`describe\`): the op
/// enum only grows at its end. Append a new variant here; never reorder.
#[test]
fn op_variants_only_append() {
    assert_eq!(describe::variants::<Op>(), ["Bump"]);
}
EOF
    cat > "$dir/src/program.rs" <<EOF
//! The module: every op and every query, each handed to its rule.

use guest::{Error, ExecCtx, Module, QueryCtx};

use crate::rules::{bump, count};
use crate::{Op, Query, Reply};

pub struct $title;

impl Module for $title {
    type Op = Op;
    type Query = Query;
    type Response = Reply;

    fn execute(ctx: &ExecCtx, op: Op) -> Result<(), Error> {
        // A write acts as an account: a key that holds none is refused.
        ctx.sender()?;
        match op {
            Op::Bump { by } => bump(ctx, by),
        }
    }

    fn query(ctx: &QueryCtx, query: Query) -> Result<Reply, Error> {
        Ok(match query {
            Query::Count => Reply::Count(count(ctx)?),
        })
    }
}

#[cfg(feature = "module")]
guest::export!($title);
EOF
    cat > "$dir/src/rules.rs" <<EOF
// The rules: what each op checks, then what it writes. A rule checks before
// it writes, so a refusal needs no rollback.

use guest::{Error, ExecCtx, QueryCtx};
use store::Item;

/// The one value this module keeps.
const COUNT: Item<u64> = Item::new("count");

pub(crate) fn bump(ctx: &ExecCtx, by: u64) -> Result<(), Error> {
    COUNT.update(ctx, |count| *count = count.saturating_add(by))?;
    Ok(())
}

pub(crate) fn count(ctx: &QueryCtx) -> Result<u64, Error> {
    Ok(COUNT.get(ctx)?.unwrap_or_default())
}
EOF
    cat > "$dir/src/view.rs" <<EOF
//! The marker a view names this module by in \`module.query\`/\`op.submit\`.
use ducktape_view_guest::methods::Module;

pub struct ${title}Api;
impl Module for ${title}Api {
    const NAME: &'static str = crate::MODULE;
    type Op = crate::Op;
    type Query = crate::Query;
    type Reply = crate::Reply;
}
EOF
    cat > "$dir/src/tests.rs" <<EOF
// The module natively over \`guest::MockHost\`, as the host runs it.

use guest::{Env, MockHost, Module, code};

use crate::{$title, MODULE, Op, Query, Reply};

/// Signed by key 1, which holds account 1.
fn signed() -> Env {
    MockHost::env(MODULE).signed([1u8; 32], Some(1))
}

#[test]
fn bumps_add_up_and_read_back() {
    let host = MockHost::default();
    $title::execute(&host.exec(signed()), Op::Bump { by: 2 }).unwrap();
    let later = Env {
        height: 2,
        ..signed()
    };
    $title::execute(&host.exec(later), Op::Bump { by: 3 }).unwrap();
    let count = $title::query(&host.query(MockHost::env(MODULE)), Query::Count).unwrap();
    assert_eq!(count, Reply::Count(5));
}

#[test]
fn a_key_that_holds_no_account_writes_nothing() {
    let host = MockHost::default();
    let unheld = MockHost::env(MODULE).signed([2u8; 32], None);
    let refusal = host.refused(|| $title::execute(&host.exec(unheld), Op::Bump { by: 1 }));
    assert_eq!(refusal.code, code::UNAUTHORIZED);
}
EOF
    register PROGRAMS "$name"
    edit Makefile "s/^VIEW_LINKABLE := .*/& $name/"
    edit Cargo.toml "s|$DEPS|&\\
$name = { path = \"$dir\" }|"
    cat <<EOF
$dir/{Cargo.toml,src/{lib,program,rules,view,tests}.rs}, PROGRAMS, VIEW_LINKABLE, workspace members and dependencies.
Next:
  1. write the contract (Op/Query/Reply and describe() in lib.rs, the module in program.rs, its rules in rules.rs); \`make dev P=$name\` builds and tests it
  2. \`make new-view NAME=$name-view\` for its screen
  3. found it in qa: a [[programs]] entry (id "$name", code "@PROGRAMS@/$snake.wasm") in founding.toml and "$snake" in kit's pack
     (crates/kit/src/main.rs: the programs it copies, the describe modules it embeds); then \`kit build NAME && kit up NAME\`
EOF
}

view() {
    case "$name" in *-view) ;; *) echo "$name: a view is named <module>-view" >&2; exit 1 ;; esac
    program=${name%-view}
    program_snake=$(echo "$program" | tr - _)
    test -d "crates/app/$program" || { echo "crates/app/$program is not there: make new-module NAME=$program first" >&2; exit 1; }
    anchor Makefile '^VIEWS := '
    anchor Cargo.toml "$MEMBERS"
    upper=$(echo "$program_snake" | tr a-z A-Z)
    # The view type: TitleCase of the module name.
    title=$(echo "$program" | awk -F- '{ for (i = 1; i <= NF; i++) printf "%s%s", toupper(substr($i, 1, 1)), substr($i, 2) }')
    mkdir -p "$dir/src"
    cat > "$dir/Cargo.toml" <<EOF
[package]
name = "$name"
edition.workspace = true
version.workspace = true
publish = false

# A Rust-authored view, exported as a dynamically loaded wasm module by
# \`export_view!\`; \`unsafe_code\` is not forbidden only because the module
# exports need \`#[unsafe(export_name)]\`.
[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
futures.workspace = true
ducktape-view-guest.workspace = true
# The module with \`module\` off and \`view\` on: its types and the marker
# this view names it by, no host import.
$program = { workspace = true, features = ["view"] }
serde.workspace = true

[dev-dependencies]
serde_json.workspace = true
EOF
    cat > "$dir/src/lib.rs" <<EOF
//! $title: the count the \`$program\` module keeps, re-read on every live
//! bump of the module.
use ducktape_view_guest::methods::{Changes, Query};
use ducktape_view_guest::export_view;
use ducktape_view_guest::host::Error;
use ducktape_view_guest::Loadable;
use ducktape_view_guest::{
    Context, Host, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Task, Theme, View, Window, div, px,
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};

use $program_snake::view::${title}Api;

#[derive(Serialize, Deserialize, Default)]
pub struct $title {
    count: Loadable<u64>,
    #[serde(skip)]
    live: Option<Task<()>>,
}

impl View for $title {
    const PREFERRED_WINDOW_SIZE: &'static str = "480,320";

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut view = Self::default();
        view.restored(window, cx);
        view
    }

    fn restored(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let mut stream = cx.host().subscribe::<Changes<${title}Api>>(());
        self.live = Some(cx.spawn(async move |this, cx| {
            while stream.next().await.is_some() {
                if this.update(cx, |view, cx| view.read(cx)).is_err() {
                    break;
                }
            }
        }));
        self.read(cx);
    }
}

impl Render for $title {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.global::<Theme>();
        let body = match &self.count {
            Loadable::Idle | Loadable::Loading(_) => "Reading…".to_owned(),
            Loadable::Ready(count) => format!("Count: {count}"),
            Loadable::Failed(refusal) => refusal.message.clone(),
        };
        div()
            .id("$program")
            .flex()
            .flex_col()
            .gap_3()
            .p_5()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_size(px(13.))
            .child(
                div()
                    .id("$program-title")
                    .text_size(px(16.))
                    .font_weight(ducktape_view_guest::FontWeight::SEMIBOLD)
                    .role(ducktape_view_guest::Role::Heading)
                    .aria_level(1)
                    .child("$title"),
            )
            .child(div().id("$program-body").child(body))
    }
}

impl $title {
    /// One read: the boot, a restore, a live bump. A value already on
    /// screen stays there while it runs.
    fn read(&mut self, cx: &mut Context<Self>) {
        match self.count.ready() {
            Some(_) => cx.refresh(count(cx.host()), |view, count, _| {
                view.count = Loadable::Ready(count)
            }),
            None => self.count = cx.load(count(cx.host()), |view| &mut view.count),
        }
        cx.notify();
    }
}

async fn count(host: Host) -> Result<u64, Error> {
    let $program_snake::Reply::Count(count) = host
        .ask::<Query<${title}Api>>($program_snake::Query::Count)
        .await?;
    Ok(count)
}

export_view!(
    $title,
    "$title",
    "The count the $program module keeps.",
    [Module]
);

#[cfg(test)]
mod tests;
EOF
    cat > "$dir/src/tests.rs" <<EOF
use super::*;
use ducktape_view_guest::testing::TestAppContext;

fn ready() -> TestAppContext {
    let mut cx = TestAppContext::new();
    cx.host().stream::<Changes<${title}Api>>();
    cx.host()
        .handle::<Query<${title}Api>>(|_| Ok($program_snake::Reply::Count(5)));
    cx.open::<$title>();
    cx.run_until_parked();
    cx
}

/// The ready screen; \`${upper}_SCREEN_EXPORT=1\` also writes its tree for the
/// app's node-less renderer (\`ducktape-app --render-tree <json>\`).
#[test]
fn the_ready_screen_shows_the_count() {
    let cx = ready();
    assert!(cx.has_text("Count: 5"), "{:?}", cx.texts());
    if std::env::var_os("${upper}_SCREEN_EXPORT").is_none() {
        return;
    }
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/$program-screens");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(
        out.join("01-ready-light.json"),
        serde_json::to_vec(cx.root()).unwrap(),
    )
    .unwrap();
}
EOF
    register VIEWS "$name"
    grep -q 'Count' "crates/app/$program/src/lib.rs" || echo "note: $program has no Query::Count; the screen's count() in $dir/src/lib.rs asks it, so step 1 comes before step 2"
    cat <<EOF
$dir/{Cargo.toml,src/lib.rs,src/tests.rs}, VIEWS and workspace members.
Next:
  1. replace the screen in src/lib.rs with what the module's Query answers (it asks \`Query::Count\` until then)
  2. \`make dev P=$program V=$name\` builds both, gates the view (ABI) and tests them
  3. pack it into its module in qa: ("$program_snake", "$snake") in kit's view list (crates/kit/src/main.rs, \`pack\`); then \`kit build NAME && kit up NAME\`
EOF
}

# Each on its own line: `set -e` does not reach into a function run on the
# left of `&&`.
case "$kind" in
    module) module ;;
    view) view ;;
    *) echo "usage: tools/scaffold.sh module <name> | view <name>-view" >&2; exit 1 ;;
esac
# Import order and line width follow the name, so rustfmt has the last word.
${CARGO:-cargo} fmt -p "$name"
