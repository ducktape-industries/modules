//! The registry role: which code runs at a height. [`run`] checks every
//! rule below.

use abi::role::registry::{Entry, Genesis, Query, Reply, View};
use guest::{BlobId, Error, MockHost, Module, ModuleId, Program};

use crate::{ask, init, same_bytes};

/// What the suite needs from the module beyond the role: publishing code
/// and scheduling changes, which the role leaves to it. Each op is sent at
/// `height`; a change lands at `at`.
pub trait Fixture {
    type Module: Module;

    /// Stores `body` as code; its blob id.
    fn publish(&self, host: &MockHost, height: u64, body: &[u8]) -> Result<BlobId, Error>;

    /// Schedules `entry` to run from `at`.
    fn set(&self, host: &MockHost, height: u64, at: u64, entry: Entry) -> Result<(), Error>;

    /// Schedules `program` to stop at `at`.
    fn remove(&self, host: &MockHost, height: u64, at: u64, program: &str) -> Result<(), Error>;

    /// The views the module lists at `height`, apart from its programs.
    fn views(&self, host: &MockHost, height: u64) -> Vec<View>;

    fn set_hook(&self, host: &MockHost, height: u64, at: u64, program: &str) -> Result<(), Error>;

    fn remove_hook(
        &self,
        host: &MockHost,
        height: u64,
        at: u64,
        program: &str,
    ) -> Result<(), Error>;
}

/// Every registry rule, each on a fresh host.
pub fn run(fixture: &impl Fixture) {
    the_role_is_the_modules_bytes(fixture);
    genesis_programs_run_and_views_are_apart(fixture);
    a_set_lands_at_its_height_and_not_before(fixture);
    a_remove_drops_at_its_height(fixture);
    a_hook_lands_at_its_height_and_goes_with_its_program(fixture);
}

fn module() -> String {
    MockHost::roles().registry
}

fn entry(program: &str, code: BlobId) -> Entry {
    Entry {
        program: program.into(),
        code,
        params: vec![],
    }
}

fn lens() -> View {
    View {
        name: "lens".into(),
        view: BlobId::Sha256([9; 32]),
    }
}

/// A host founded with programs `a` and `b` and the view `lens`.
fn founded<F: Fixture>() -> MockHost {
    let host = MockHost::default();
    let genesis = Genesis {
        programs: vec![
            entry("b", BlobId::Sha256([2; 32])),
            entry("a", BlobId::Sha256([1; 32])),
        ],
        views: vec![lens()],
        hooks: vec!["a".into()],
    };
    init::<F::Module>(&host, &module(), &genesis);
    host
}

/// What `At(height)` answers, by program. Asked a block ahead, as the
/// kernel asks, so an answer read off the env's height instead of the
/// asked one does not pass.
fn at<F: Fixture>(host: &MockHost, height: u64) -> Vec<Entry> {
    let asked = Query::At(height);
    let from = height.saturating_sub(1).max(1);
    let mut entries = match ask::<F::Module, Reply>(host, &module(), from, &asked) {
        Ok(Reply::Programs(entries)) => entries,
        other => panic!("registry: At({height}) answers Programs, not {other:?}"),
    };
    entries.sort_by(|a, b| a.program.cmp(&b.program));
    entries
}

fn programs<F: Fixture>(host: &MockHost, height: u64) -> Vec<ModuleId> {
    at::<F>(host, height)
        .into_iter()
        .map(|e| e.program)
        .collect()
}

pub fn the_role_is_the_modules_bytes<F: Fixture>(_: &F) {
    type M<F> = <F as Fixture>::Module;
    let m = module();
    same_bytes::<<M<F> as Program>::Query>(&m, "Query", &Query::At(5));
    same_bytes::<<M<F> as Program>::Query>(&m, "Query", &Query::Hooks(5));
    let reply = Reply::Programs(vec![entry("a", BlobId::Sha256([1; 32]))]);
    same_bytes::<<M<F> as Program>::Reply>(&m, "Reply", &reply);
    let hooks = Reply::Hooks(vec!["a".into()]);
    same_bytes::<<M<F> as Program>::Reply>(&m, "Reply", &hooks);
}

fn hooks<F: Fixture>(host: &MockHost, height: u64) -> Vec<ModuleId> {
    let asked = Query::Hooks(height);
    let from = height.saturating_sub(1).max(1);
    let mut hooks = match ask::<F::Module, Reply>(host, &module(), from, &asked) {
        Ok(Reply::Hooks(hooks)) => hooks,
        other => panic!("registry: Hooks({height}) answers Hooks, not {other:?}"),
    };
    hooks.sort();
    hooks
}

pub fn a_hook_lands_at_its_height_and_goes_with_its_program<F: Fixture>(fixture: &F) {
    let host = founded::<F>();
    assert_eq!(hooks::<F>(&host, 1), ["a"], "registry: genesis hooks a");
    fixture
        .set_hook(&host, 1, 5, "b")
        .unwrap_or_else(|e| panic!("registry: scheduling b's hook at 5: {e:?}"));
    fixture
        .remove_hook(&host, 1, 6, "a")
        .unwrap_or_else(|e| panic!("registry: scheduling a's unhook at 6: {e:?}"));
    fixture
        .remove(&host, 1, 7, "b")
        .unwrap_or_else(|e| panic!("registry: scheduling b's removal at 7: {e:?}"));
    assert_eq!(
        hooks::<F>(&host, 4),
        ["a"],
        "registry: b is no hook before 5"
    );
    assert_eq!(hooks::<F>(&host, 5), ["a", "b"], "registry: b hooks from 5");
    assert_eq!(hooks::<F>(&host, 6), ["b"], "registry: a unhooks at 6");
    assert_eq!(
        hooks::<F>(&host, 7),
        Vec::<ModuleId>::new(),
        "registry: a program dropped is no hook"
    );
}

/// `At` answers every founding program as founded, and no view: views are
/// listed apart.
pub fn genesis_programs_run_and_views_are_apart<F: Fixture>(fixture: &F) {
    let host = founded::<F>();
    assert_eq!(
        at::<F>(&host, 1),
        [
            entry("a", BlobId::Sha256([1; 32])),
            entry("b", BlobId::Sha256([2; 32]))
        ],
        "registry: At answers the founding programs, and no view"
    );
    assert_eq!(
        fixture.views(&host, 1),
        [lens()],
        "registry: the founding view is listed apart"
    );
}

/// Published code scheduled for a height runs from that height, not
/// before: a new program is admitted, a running one's code swapped.
pub fn a_set_lands_at_its_height_and_not_before<F: Fixture>(fixture: &F) {
    let host = founded::<F>();
    let code = fixture
        .publish(&host, 1, b"new code")
        .unwrap_or_else(|e| panic!("registry: publishing code: {e:?}"));
    for program in ["new", "a"] {
        fixture
            .set(&host, 1, 5, entry(program, code))
            .unwrap_or_else(|e| panic!("registry: scheduling {program} at 5: {e:?}"));
    }
    assert_eq!(
        at::<F>(&host, 4),
        [
            entry("a", BlobId::Sha256([1; 32])),
            entry("b", BlobId::Sha256([2; 32]))
        ],
        "registry: nothing scheduled at 5 runs at 4"
    );
    for height in [5, 6] {
        assert_eq!(
            at::<F>(&host, height),
            [
                entry("a", code),
                entry("b", BlobId::Sha256([2; 32])),
                entry("new", code)
            ],
            "registry: at {height}, the set scheduled at 5 runs"
        );
    }
}

/// A program scheduled to stop at a height runs until it, and not from it,
/// even once the module has run past it.
pub fn a_remove_drops_at_its_height<F: Fixture>(fixture: &F) {
    let host = founded::<F>();
    fixture
        .remove(&host, 1, 6, "b")
        .unwrap_or_else(|e| panic!("registry: scheduling b's removal at 6: {e:?}"));
    assert_eq!(
        programs::<F>(&host, 5),
        ["a", "b"],
        "registry: b runs until 6"
    );
    assert_eq!(
        programs::<F>(&host, 6),
        ["a"],
        "registry: b is dropped at 6"
    );
    fixture
        .publish(&host, 7, b"later")
        .unwrap_or_else(|e| panic!("registry: publishing at 7: {e:?}"));
    assert_eq!(
        programs::<F>(&host, 7),
        ["a"],
        "registry: b stays dropped once the module has run past 6"
    );
    assert_eq!(
        fixture.views(&host, 7),
        [lens()],
        "registry: removing a program leaves the views"
    );
}
