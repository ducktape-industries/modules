//! The create form: a new agent is identity's `CreateAgent`, a `files` commit of its
//! configuration file (proposed in its manager's home, `/home/<manager's name>/agents/`, which
//! files lets that account alone write), identity's `SetScript` with a script that calls
//! harness and names that file, then identity's `SetAudience` when it answers more than its
//! manager. A step that is refused keeps the steps before it, so pressing on resumes where it
//! stopped. Editing an agent is the same form at its commit: a new version of its file, and a
//! new script when its path moves.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use view::methods::Submitted;
use view::prelude::*;

use harness::{AgentSetup, BlobId, Config, Configured};
use identity::{Audience, Identity, Script};

use crate::{Agents, Screen, applied};

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Draft {
    pub name: TextField,
    pub path: TextField,
    pub model: Option<String>,
    pub max_tokens: TextField,
    pub wait: TextField,
    pub system: TextField,
    pub answers: Answers,
    pub accounts: TextField,
    pub created: Option<u64>,
    pub committed: bool,
    pub scripted: bool,
    pub answered: bool,
    pub again: bool,
    pub base: Option<[u8; 32]>,
    pub script_path: Option<String>,
    pub problem: Option<String>,
    #[serde(skip)]
    pub sending: Option<Task<()>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Answers {
    #[default]
    Manager,
    Anyone,
    Accounts,
}

pub(crate) struct Form {
    pub path: String,
    pub config: Config,
    pub audience: Audience,
}

enum Next {
    Create(String),
    Commit(u64),
    Script(u64),
    Answer(u64, Audience),
    Done(u64),
}

pub(crate) fn proposed_path(home: Option<&str>, name: &str) -> Option<String> {
    let home = home?;
    let slug: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| match c.is_alphanumeric() || c == '-' || c == '_' {
            true => c,
            false => '-',
        })
        .collect();
    Some(format!("/home/{home}/agents/{slug}.md"))
}

impl Draft {
    pub fn path(&self, home: Option<&str>) -> Option<String> {
        let typed = self.path.text().trim().to_owned();
        match typed.is_empty() {
            true => proposed_path(home, &self.name.text()),
            false => Some(typed),
        }
    }

    pub fn checked(&self, home: Option<&str>) -> Result<Form, String> {
        if self.name.text().trim().is_empty() {
            return Err("Name the agent.".into());
        }
        let Some(path) = self.path(home) else {
            return Err(
                "Your account's name is not read yet, so no home is proposed: type the configuration file's path.".into(),
            );
        };
        let Some(model) = self.model.clone() else {
            return Err("Pick the model it asks.".into());
        };
        let max_tokens = match self.max_tokens.text().trim().parse::<u32>() {
            Ok(max) if max > 0 => max,
            _ => return Err("Max output tokens is a whole number, at least 1.".into()),
        };
        let wait = self.wait.text().trim().to_owned();
        let wait = match wait.is_empty() {
            true => None,
            false => match wait.parse::<u64>() {
                Ok(blocks) if blocks > 0 => Some(blocks),
                _ => return Err("Wait is a whole number of blocks, at least 1, or empty.".into()),
            },
        };
        let audience = match self.answers {
            Answers::Manager => Audience::Manager,
            Answers::Anyone => Audience::Anyone,
            Answers::Accounts => {
                let numbers: Result<Vec<u64>, _> = self
                    .accounts
                    .text()
                    .split([',', ' '])
                    .filter(|word| !word.is_empty())
                    .map(str::parse)
                    .collect();
                match numbers {
                    Ok(numbers) if !numbers.is_empty() => Audience::Accounts(numbers),
                    _ => return Err("List the account numbers it answers, like 7, 12.".into()),
                }
            }
        };
        Ok(Form {
            path,
            config: Config {
                model,
                max_tokens,
                wait,
                system: self.system.text().trim().to_owned(),
            },
            audience,
        })
    }

    fn next(&self, form: &Form) -> Next {
        let Some(agent) = self.created else {
            return Next::Create(self.name.text().trim().to_owned());
        };
        if !self.committed {
            return Next::Commit(agent);
        }
        let moved = self.script_path.as_deref() != Some(form.path.as_str());
        if moved && !self.scripted {
            return Next::Script(agent);
        }
        let answers_more = !self.again && !self.answered && form.audience != Audience::Manager;
        if answers_more {
            return Next::Answer(agent, form.audience.clone());
        }
        Next::Done(agent)
    }
}

fn base_of(configured: &Configured) -> Option<[u8; 32]> {
    let snapshot = match configured {
        Configured::Unreadable { .. } => None,
        Configured::Malformed { snapshot, .. }
        | Configured::UnknownModel { snapshot, .. }
        | Configured::Ready { snapshot, .. } => Some(*snapshot),
    };
    match snapshot {
        Some(BlobId::Sha256(digest)) => Some(digest),
        Some(BlobId::Sha1(_)) | None => None,
    }
}

impl Agents {
    pub(crate) fn start_create(&mut self, cx: &mut Context<Self>) {
        self.draft = Draft::default();
        self.go(Screen::Create, cx);
    }

    pub(crate) fn resume(&mut self, agent: u64, name: &str, cx: &mut Context<Self>) {
        self.draft = Draft {
            name: TextField::new(name),
            created: Some(agent),
            ..Draft::default()
        };
        self.go(Screen::Create, cx);
    }

    pub(crate) fn edit(&mut self, setup: &AgentSetup, name: &str, cx: &mut Context<Self>) {
        let config = setup.configured.config().map(|(_, config)| config.clone());
        let field = |text: Option<String>| TextField::new(text.unwrap_or_default());
        self.draft = Draft {
            name: TextField::new(name),
            path: TextField::new(setup.path.clone()),
            model: config.as_ref().map(|config| config.model.clone()),
            max_tokens: field(config.as_ref().map(|config| config.max_tokens.to_string())),
            wait: field(
                config
                    .as_ref()
                    .and_then(|config| config.wait)
                    .map(|wait| wait.to_string()),
            ),
            system: field(config.as_ref().map(|config| config.system.clone())),
            created: Some(setup.agent),
            again: true,
            base: base_of(&setup.configured),
            script_path: Some(setup.path.clone()),
            ..Draft::default()
        };
        self.go(Screen::Create, cx);
    }

    pub(crate) fn advance(&mut self, cx: &mut Context<Self>) {
        if self.draft.sending.is_some() {
            return;
        }
        let form = match self.draft.checked(self.home()) {
            Ok(form) => form,
            Err(problem) => {
                self.draft.problem = Some(problem);
                return cx.notify();
            }
        };
        self.draft.problem = None;
        match self.draft.next(&form) {
            Next::Create(name) => self.send(identity::Op::CreateAgent { name }, cx),
            Next::Commit(agent) => self.commit(agent, form, cx),
            Next::Script(account) => {
                let script = Some(Script {
                    calls: harness::PROGRAM.to_owned(),
                    config: form.path,
                });
                self.send(identity::Op::SetScript { account, script }, cx)
            }
            Next::Answer(account, audience) => {
                self.send(identity::Op::SetAudience { account, audience }, cx)
            }
            Next::Done(agent) => {
                self.draft = Draft::default();
                self.go(Screen::Agent(agent), cx);
            }
        }
    }

    fn send(&mut self, op: identity::Op, cx: &mut Context<Self>) {
        let submit = cx.host().ask::<Submit<Identity>>(op.clone());
        self.draft.sending = Some(cx.land(submit, move |view, answer, cx| {
            view.draft.sending = None;
            let in_doubt = matches!(answer, Ok(Submitted::InDoubt { .. }));
            let landed = match applied(answer) {
                Err(_) if in_doubt && matches!(op, identity::Op::CreateAgent { .. }) => Err(
                    "No answer came back: the account may have been made. Look for it before you create another."
                        .to_owned(),
                ),
                landed => landed,
            };
            let landed = landed.and_then(|output| match op {
                identity::Op::CreateAgent { .. } => {
                    let number = view::methods::decode::<u64>(&output)
                        .map_err(|error| format!("No account number came back: {error}"))?;
                    view.draft.created = Some(number);
                    Ok(())
                }
                identity::Op::SetScript { .. } => {
                    view.draft.scripted = true;
                    Ok(())
                }
                _ => {
                    view.draft.answered = true;
                    Ok(())
                }
            });
            view.landed(landed, cx);
        }));
        cx.notify();
    }

    fn commit(&mut self, agent: u64, form: Form, cx: &mut Context<Self>) {
        let name = self.draft.name.text();
        let op = files::Op::Commit {
            base: self.draft.base.map(BlobId::Sha256),
            message: format!("Configure agent {name} (account {agent})"),
            changes: vec![files::Change::Put {
                path: form.path,
                exec: false,
                meta: BTreeMap::new(),
                content: files::Content::Inline(form.config.write().into_bytes()),
            }],
        };
        let submit = cx.host().ask::<Submit<files::Files>>(op);
        self.draft.sending = Some(cx.land(submit, |view, answer, cx| {
            view.draft.sending = None;
            let landed = applied(answer).map(|_| view.draft.committed = true);
            view.landed(landed, cx);
        }));
        cx.notify();
    }

    fn landed(&mut self, landed: Result<(), String>, cx: &mut Context<Self>) {
        match landed {
            Ok(()) => self.advance(cx),
            Err(problem) => {
                self.draft.problem = Some(problem);
                cx.notify();
            }
        }
    }
}
