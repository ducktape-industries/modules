//! What the reader changes on the Programs page, and how: by a grant it
//! holds (the op goes to the programs role), else by the validators' vote (the
//! op goes to valset wrapped in a `Programs` motion), else not at all. The
//! app's confirm card is the yes for every op; nothing here asks again.
//! The forms' ops are built by pure functions, each a unit test.
use programs::{Change, Grant, NOTICE, Programs, Scheduled, Scope};
use valset::{Motion, Valset, Voted};
use view::methods::Submitted;
use view::prelude::*;

use crate::Explorer;
use crate::state::{Network, Note};

/// Blocks past the earliest a maintainer's change may land that a form
/// asks by default: the op lands a block or two after the press.
const SLACK: u64 = 30;
/// How far past the tip a vote's change lands by default: about ten minutes.
const LEAD_MS: u64 = 10 * 60 * 1000;

/// The way an op takes to the programs role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Path {
    /// the reader's grant: straight to the programs role
    Grant,
    /// the validators' vote: a motion to valset
    Vote,
}

/// Who reads, and what they are changing.
#[derive(Default)]
pub(crate) struct Acting {
    /// the session's account and its seated key
    pub(crate) account: Option<u64>,
    pub(crate) signer: Option<Vec<u8>>,
    /// the validators the seated key votes for; empty: it does not vote
    pub(crate) voices: Vec<Vec<u8>>,
    pub(crate) form: Option<Form>,
    /// an op in flight: no other press until it lands
    pub(crate) busy: bool,
}

/// What a form changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Edit {
    /// new code for a running program, or a listed view
    Upgrade {
        name: String,
        view: bool,
    },
    Remove {
        name: String,
        view: bool,
    },
    /// a name that runs nothing yet
    Add,
    /// an account made a maintainer, by vote
    Grant,
}

/// One form open on the page; its fields are the ones its [`Edit`] reads.
pub(crate) struct Form {
    pub(crate) edit: Edit,
    pub(crate) path: Path,
    pub(crate) code: TextField,
    pub(crate) height: TextField,
    /// Add: the new id; Grant: the program, empty for every program
    pub(crate) name: TextField,
    /// Add: params as hex, empty for none
    pub(crate) params: TextField,
    /// Grant: the account's number
    pub(crate) account: TextField,
    /// Add: a view, not a program
    pub(crate) view: bool,
    /// why the last press built no op, or the host's refusal
    pub(crate) problem: Option<String>,
}

impl Acting {
    pub(crate) fn votes(&self) -> bool {
        !self.voices.is_empty()
    }
}

impl Explorer {
    /// The way the reader changes `program`: by a grant that covers it,
    /// else by vote, else none.
    pub(crate) fn path(&self, program: &str) -> Option<Path> {
        if self.maintains(program) {
            return Some(Path::Grant);
        }
        self.acting.votes().then_some(Path::Vote)
    }

    /// Whether the reader's grants cover `program`, as the programs role reads them.
    pub(crate) fn maintains(&self, program: &str) -> bool {
        let (Some(network), Some(account)) = (self.network.ready(), self.acting.account) else {
            return false;
        };
        network
            .roles
            .as_ref()
            .is_some_and(|roles| programs::covers(&network.grants, roles, account, program))
    }

    /// Whether the reader may add a program by grant: it holds every program,
    /// or a program that runs nothing yet.
    pub(crate) fn adds(&self) -> bool {
        let (Some(network), Some(account)) = (self.network.ready(), self.acting.account) else {
            return false;
        };
        network.grants.iter().any(|grant| {
            grant.account == account
                && match &grant.scope {
                    Scope::All => true,
                    Scope::Program(id) => !network.names(id),
                }
        })
    }

    pub(crate) fn session_changed(&mut self, session: Session, cx: &mut Context<Self>) {
        if self.session_chain != session.chain_id {
            self.session_chain = session.chain_id;
            cx.notify();
        }
        let signer = abi::unhex(&session.signer).filter(|key| !key.is_empty());
        if signer != self.acting.signer || session.account != self.acting.account {
            // another reader's rights and forms are not this one's
            self.acting = Acting {
                account: session.account,
                signer,
                ..Acting::default()
            };
            cx.notify();
        }
        self.read_voices(cx);
    }

    /// Whom the seated key votes for: a session, a valset head.
    pub(crate) fn read_voices(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.acting.signer.clone() else {
            return;
        };
        let ask = cx.host().query(valset::ask::Voices { key: key.clone() });
        cx.land(ask, move |view, answer, cx| {
            // a session moved on while this was asked
            if view.acting.signer.as_ref() != Some(&key) {
                return;
            }
            match answer {
                Ok(voices) if voices != view.acting.voices => {
                    view.acting.voices = voices;
                    cx.notify();
                }
                Ok(_) => {}
                Err(refusal) => cx.log_refused("whom this key votes for", &refusal),
            }
        })
        .detach();
    }

    /// Opens a form for `edit` on `path`, its height at the path's default.
    pub(crate) fn open_form(&mut self, edit: Edit, path: Path, cx: &mut Context<Self>) {
        let height = match self.status.ready() {
            Some(status) => default_height(path, status.height, status.block_time_ms).to_string(),
            None => String::new(),
        };
        self.acting.form = Some(Form {
            edit,
            path,
            code: TextField::default(),
            height: TextField::new(height),
            name: TextField::default(),
            params: TextField::default(),
            account: TextField::default(),
            view: false,
            problem: None,
        });
        cx.notify();
    }

    pub(crate) fn close_form(&mut self, cx: &mut Context<Self>) {
        self.acting.form = None;
        cx.notify();
    }

    /// Add's kind: a program, or a view.
    pub(crate) fn add_view(&mut self, view: bool, cx: &mut Context<Self>) {
        if let Some(form) = &mut self.acting.form {
            form.view = view;
            cx.notify();
        }
    }

    /// The open form's op, sent on its path; a form that builds none says why.
    pub(crate) fn submit_form(&mut self, cx: &mut Context<Self>) {
        let (Some(form), Some(network)) = (&mut self.acting.form, self.network.ready()) else {
            return;
        };
        let tip = self.status.ready().map_or(0, |status| status.height);
        match built(form, network, tip) {
            Ok(op) => {
                form.problem = None;
                let path = form.path;
                self.send(path, op, cx);
            }
            Err(problem) => {
                form.problem = Some(problem);
                cx.notify();
            }
        }
    }

    /// A press with no form: a cancel, a revoke, stepping down.
    pub(crate) fn press(&mut self, path: Path, op: programs::Op, cx: &mut Context<Self>) {
        self.acting.form = None;
        self.send(path, op, cx);
    }

    fn send(&mut self, path: Path, op: programs::Op, cx: &mut Context<Self>) {
        if self.acting.busy {
            return;
        }
        self.acting.busy = true;
        self.note = None;
        let title = programs::describe(&op).title;
        let landed =
            move |view: &mut Self, answer: Result<Submitted, Error>, cx: &mut Context<Self>| {
                view.landed(path, &title, answer, cx)
            };
        match path {
            Path::Grant => {
                let ask = cx.host().ask::<Submit<Programs>>(op);
                cx.land(ask, landed).detach();
            }
            Path::Vote => {
                let motion = Motion::Programs {
                    op: abi::encode(&op),
                };
                let ask = cx.host().ask::<Submit<Valset>>(valset::Op::Vote(motion));
                cx.land(ask, landed).detach();
            }
        }
        // the cleared problem and note go now: the app's card may wait over
        // the form long before the answer lands
        cx.notify();
    }

    /// An op landed: the form closes and the line says what it did, or
    /// the form (else the line) says why not, in the host's words.
    fn landed(
        &mut self,
        path: Path,
        title: &str,
        answer: Result<Submitted, Error>,
        cx: &mut Context<Self>,
    ) {
        self.acting.busy = false;
        let problem = match answer {
            Ok(Submitted::Applied { output, .. }) => {
                self.acting.form = None;
                self.note = Some(Note::Sent(sent(path, &output, title)));
                cx.notify();
                return;
            }
            Ok(Submitted::Refused { refusal, .. }) | Err(refusal) => refusal.message,
            Ok(Submitted::InDoubt { .. }) => {
                "No answer came back: it may have gone through.".into()
            }
        };
        match &mut self.acting.form {
            Some(form) => form.problem = Some(problem),
            None => self.note = Some(Note::Refused(problem)),
        }
        cx.notify();
    }
}

impl Network {
    /// Whether `name` runs or is listed as a view.
    pub(crate) fn names(&self, name: &str) -> bool {
        self.programs.iter().any(|entry| entry.program == name)
            || self.views.iter().any(|view| view.name == name)
    }
}

/// What an op that landed did, in words.
fn sent(path: Path, output: &[u8], title: &str) -> String {
    match (path, abi::decode::<Voted>(output)) {
        (Path::Grant, _) => format!("Sent to the programs role: {title}."),
        (Path::Vote, Ok(Voted::Enacted)) => format!("Enacted: {title}."),
        (Path::Vote, Ok(Voted::Counted { votes, needed })) => {
            format!("Vote counted, {votes} of {needed}: {title}.")
        }
        (Path::Vote, Err(_)) => format!("Voted: {title}."),
    }
}

/// The block a form asks by default: on a grant, [`NOTICE`] past the tip
/// and some slack, so the block the op lands in still leaves the notice;
/// on a vote, about ten minutes.
pub(crate) fn default_height(path: Path, tip: u64, block_time_ms: u64) -> u64 {
    match path {
        Path::Grant => tip + NOTICE + SLACK,
        Path::Vote => tip + LEAD_MS.div_ceil(block_time_ms.max(1)),
    }
}

/// The first block a maintainer's change pressed now may land at: the op
/// lands in the next block, and waits [`NOTICE`] from there.
pub(crate) fn earliest(tip: u64) -> u64 {
    tip + 1 + NOTICE
}

/// The op a form builds, or why it builds none.
pub(crate) fn built(form: &Form, network: &Network, tip: u64) -> Result<programs::Op, String> {
    let at = |height: u64, change: Change| programs::Op::Schedule(Scheduled { height, change });
    Ok(match &form.edit {
        Edit::Upgrade { name, view: false } => {
            let current = network
                .programs
                .iter()
                .find(|entry| &entry.program == name)
                .ok_or_else(|| format!("{name} no longer runs here."))?;
            let entry = programs::Entry {
                code: code(&form.code.text())?,
                ..current.clone()
            };
            at(height(&form.height.text(), tip)?, Change::Set(entry))
        }
        Edit::Upgrade { name, view: true } => {
            let view = programs::View {
                name: name.clone(),
                view: code(&form.code.text())?,
            };
            at(height(&form.height.text(), tip)?, Change::SetView(view))
        }
        Edit::Remove { name, view } => {
            let change = match view {
                false => Change::Remove(name.clone()),
                true => Change::RemoveView(name.clone()),
            };
            at(height(&form.height.text(), tip)?, change)
        }
        Edit::Add => {
            let name = form.name.text().trim().to_owned();
            if name.is_empty() {
                return Err("Name the program.".into());
            }
            let code = code(&form.code.text())?;
            let change = match form.view {
                true => Change::SetView(programs::View { name, view: code }),
                false => {
                    let params = form.params.text();
                    let params = params.trim().trim_start_matches("0x");
                    let params = abi::unhex(params).ok_or("Params are hex digits, or nothing.")?;
                    Change::Set(programs::Entry {
                        program: name,
                        code,
                        params,
                    })
                }
            };
            at(height(&form.height.text(), tip)?, change)
        }
        Edit::Grant => {
            let account = form
                .account
                .text()
                .trim()
                .trim_start_matches('#')
                .parse()
                .map_err(|_| "An account is its number.".to_owned())?;
            let program = form.name.text().trim().to_owned();
            let scope = match program.is_empty() {
                true => Scope::All,
                false => Scope::Program(program),
            };
            programs::Op::Grant(Grant { scope, account })
        }
    })
}

/// A held code's id as a release names it: `sha256:<64 hex>`, or the
/// hex alone.
fn code(text: &str) -> Result<abi::BlobId, String> {
    let text = text.trim();
    let digits = text.strip_prefix("sha256:").unwrap_or(text);
    let digest: Option<[u8; 32]> = abi::unhex(digits).and_then(|bytes| bytes.try_into().ok());
    digest
        .map(abi::BlobId::Sha256)
        .ok_or_else(|| "A code id is sha256: and 64 hex digits.".into())
}

fn height(text: &str, tip: u64) -> Result<u64, String> {
    let height: u64 = text
        .trim()
        .replace(',', "")
        .parse()
        .map_err(|_| "The height is a block number.".to_owned())?;
    if height <= tip {
        return Err(format!(
            "Block {} has passed: pick a later one.",
            design::grouped(height)
        ));
    }
    Ok(height)
}

/// The cancel of a waiting change.
pub(crate) fn cancel(pending: &programs::Pending) -> programs::Op {
    programs::Op::Cancel {
        height: pending.height,
        program: pending.change.program().to_owned(),
    }
}
