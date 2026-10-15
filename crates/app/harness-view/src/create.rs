//! The create form: a new agent is three ops in order, identity's `CreateAgent`, identity's
//! `SetScript` with a script that calls harness, then harness's `Hire`, and identity's `SetAudience` when it
//! answers more than its manager. The app asks the person to confirm each identity op;
//! `Hire` gets no card from the app, so this form asks once before it sends it. A step
//! that is refused keeps the steps before it, so pressing on resumes where it stopped.
//! Hiring again is the same form at its third step.
use serde::{Deserialize, Serialize};
use view::methods::Submitted;
use view::prelude::*;

use harness::AgentRow;
use identity::{Audience, Identity, Script};

use crate::{Agents, Screen, applied};

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Draft {
    pub name: TextField,
    pub system: TextField,
    pub model: TextField,
    pub max_tokens: TextField,
    pub service: Option<String>,
    pub answers: Answers,
    /// the account numbers `Answers::Accounts` names
    pub accounts: TextField,
    /// the account `CreateAgent` made
    pub created: Option<u64>,
    /// `SetScript` handed it to harness: its script calls harness
    pub handed: bool,
    pub hired: bool,
    pub answered: bool,
    /// it is hired already: the form hires it again
    pub again: bool,
    /// the hire is shown for the person's yes
    pub confirming: bool,
    pub problem: Option<String>,
    #[serde(skip)]
    pub sending: Option<Task<()>>,
}

/// Who the agent answers besides its manager.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Answers {
    #[default]
    Manager,
    Anyone,
    Accounts,
}

/// What a hire sends, read off the form.
pub(crate) struct Hire {
    pub system: String,
    pub model: String,
    pub service: String,
    pub max_tokens: u32,
    pub audience: Audience,
}

/// The op the form sends next.
enum Next {
    Create(String),
    Hand(u64),
    Confirm,
    Answer(u64, Audience),
    Done(u64),
}

impl Draft {
    /// The form, or what is wrong with it, in the words shown under it.
    pub fn checked(&self) -> Result<Hire, String> {
        if self.name.text().trim().is_empty() {
            return Err("Name the agent.".into());
        }
        let model = self.model.text().trim().to_owned();
        if model.is_empty() {
            return Err("Write the model id.".into());
        }
        let max_tokens = match self.max_tokens.text().trim().parse::<u32>() {
            Ok(max) if max > 0 => max,
            _ => return Err("Max output tokens is a whole number, at least 1.".into()),
        };
        let Some(service) = self.service.clone() else {
            return Err("Pick the service that reaches the model.".into());
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
        Ok(Hire {
            system: self.system.text(),
            model,
            service,
            max_tokens,
            audience,
        })
    }

    fn next(&self, audience: Audience) -> Next {
        match (self.created, self.handed, self.hired) {
            (None, _, _) => Next::Create(self.name.text().trim().to_owned()),
            (Some(agent), false, _) => Next::Hand(agent),
            (Some(_), true, false) => Next::Confirm,
            (Some(agent), true, true)
                if !self.again && !self.answered && audience != Audience::Manager =>
            {
                Next::Answer(agent, audience)
            }
            (Some(agent), true, true) => Next::Done(agent),
        }
    }
}

impl Agents {
    pub(crate) fn start_create(&mut self, cx: &mut Context<Self>) {
        self.draft = Draft::default();
        self.go(Screen::Create, cx);
    }

    /// Picks up an agent that stopped before its hire.
    pub(crate) fn resume(&mut self, agent: u64, name: &str, cx: &mut Context<Self>) {
        self.draft = Draft {
            name: TextField::new(name),
            created: Some(agent),
            handed: true,
            ..Draft::default()
        };
        self.go(Screen::Create, cx);
    }

    /// The form over what `row` was hired with: a hire again changes what the next
    /// thread opens with.
    pub(crate) fn edit(&mut self, row: &AgentRow, cx: &mut Context<Self>) {
        self.draft = Draft {
            name: TextField::new(row.name.clone()),
            system: TextField::new(row.system.clone()),
            model: TextField::new(row.model.clone()),
            max_tokens: TextField::new(row.max_tokens.to_string()),
            service: Some(row.service.clone()),
            created: Some(row.agent),
            handed: true,
            again: true,
            ..Draft::default()
        };
        self.go(Screen::Create, cx);
    }

    /// Sends what is missing, in order, up to the hire's confirmation.
    pub(crate) fn advance(&mut self, cx: &mut Context<Self>) {
        if self.draft.sending.is_some() {
            return;
        }
        let hire = match self.draft.checked() {
            Ok(hire) => hire,
            Err(problem) => {
                self.draft.problem = Some(problem);
                return cx.notify();
            }
        };
        self.draft.problem = None;
        match self.draft.next(hire.audience) {
            Next::Create(name) => self.send(identity::Op::CreateAgent { name }, cx),
            Next::Hand(account) => {
                // harness reads nothing from the config
                let script = Some(Script {
                    calls: harness::PROGRAM.to_owned(),
                    config: String::new(),
                });
                self.send(identity::Op::SetScript { account, script }, cx)
            }
            Next::Confirm => {
                self.draft.confirming = true;
                cx.notify();
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
                    view.draft.handed = true;
                    Ok(())
                }
                _ => {
                    view.draft.answered = true;
                    Ok(())
                }
            });
            match landed {
                Ok(()) => view.advance(cx),
                Err(problem) => {
                    view.draft.problem = Some(problem);
                    cx.notify();
                }
            }
        }));
        cx.notify();
    }

    /// The person's yes to the hire the form shows.
    pub(crate) fn hire(&mut self, cx: &mut Context<Self>) {
        let (Some(agent), Ok(hire)) = (self.draft.created, self.draft.checked()) else {
            return;
        };
        if self.draft.sending.is_some() {
            return;
        }
        let op = harness::Op::Hire {
            agent,
            system: hire.system,
            model: hire.model,
            service: hire.service,
            max_tokens: hire.max_tokens,
        };
        let submit = cx.host().ask::<Submit<harness::Harness>>(op);
        self.draft.sending = Some(cx.land(submit, |view, answer, cx| {
            view.draft.sending = None;
            view.draft.confirming = false;
            match applied(answer) {
                Ok(_) => {
                    view.draft.hired = true;
                    view.advance(cx);
                }
                Err(problem) => {
                    view.draft.problem = Some(problem);
                    cx.notify();
                }
            }
        }));
        cx.notify();
    }
}
