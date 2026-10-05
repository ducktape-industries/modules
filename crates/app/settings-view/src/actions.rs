//! What the reader's presses do: reads again, each form's submit, and the
//! invite. A refusal lands on the form it came from.
use ducktape_view_guest::Loadable;
use ducktape_view_guest::methods::ClipboardWrite;
use ducktape_view_guest::{Context, TextField};

use crate::api::{CreateInvite, InviteCreate, Session, Submit};
use crate::state::{Form, Problem, Section, TTL};
use crate::{Settings, queries};
use identity::Identity;

impl Settings {
    pub(crate) fn session_changed(&mut self, session: Session, cx: &mut Context<Self>) {
        if session.account.is_some() {
            self.create_account = Form::default();
        }
        self.session = session;
        self.read_account(cx);
    }

    pub(crate) fn select_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        cx.notify();
    }

    /// Who the session's key is, read afresh: the boot, a new session, a
    /// retry.
    pub(crate) fn read_account(&mut self, cx: &mut Context<Self>) {
        let work = self.account_query(cx);
        // another reader's account is not this one's: nothing of it stays
        self.account = Loadable::Idle;
        cx.load(self, work, |view| &mut view.account);
    }

    /// The same account, re-read with what is on screen kept.
    pub(crate) fn refresh_account(&mut self, cx: &mut Context<Self>) {
        let work = self.account_query(cx);
        cx.load(self, work, |view| &mut view.account);
    }

    fn account_query(
        &self,
        cx: &mut Context<Self>,
    ) -> impl Future<Output = Result<Option<queries::Seat>, ducktape_view_guest::host::Error>> + 'static
    {
        let (signer, number) = (self.session.signer.clone(), self.session.account);
        queries::account(cx.host(), signer, number)
    }

    pub(crate) fn submit_create_account(&mut self, cx: &mut Context<Self>) {
        if self.create_account.busy {
            return;
        }
        let name = self.create_account.text.text().trim().to_string();
        if name.is_empty() {
            self.create_account.problem = Some(Problem::Empty);
            cx.notify();
            return;
        }
        self.create_account.problem = None;
        self.create_account.busy = true;
        cx.notify();
        let op = identity::Op::Create {
            name,
            scheme: abi::Scheme::Ed25519,
        };
        let ask = cx.host().ask::<Submit<Identity>>(op);
        cx.land(ask, |view, result, cx| {
            // created: the form stays busy until the host's session
            // names the new account, which re-reads it
            if let Err(refusal) = result {
                view.create_account.busy = false;
                view.create_account.problem = Some(Problem::Refused(refusal.message));
            }
            cx.notify();
        })
        .detach();
    }

    /// Rename's first press turns the agent's name into a field holding
    /// it; the next submits it.
    pub(crate) fn submit_rename_agent(&mut self, number: u64, cx: &mut Context<Self>) {
        if !self.rename_agent.contains_key(&number) {
            let name = self.agent_name(number).unwrap_or_default();
            self.rename_agent.insert(
                number,
                Form {
                    text: TextField::new(name),
                    ..Form::default()
                },
            );
            cx.notify();
            return;
        }
        let form = self.rename_agent.entry(number).or_default();
        let name = form.text.text().trim().to_string();
        if name.is_empty() {
            form.problem = Some(Problem::Empty);
            cx.notify();
            return;
        }
        let op = identity::Op::SetName {
            account: number,
            name,
        };
        self.submit_agent_op(op, move |v| v.rename_agent.entry(number).or_default(), cx);
    }

    pub(crate) fn cancel_rename_agent(&mut self, number: u64, cx: &mut Context<Self>) {
        self.rename_agent.remove(&number);
        cx.notify();
    }

    fn agent_name(&self, number: u64) -> Option<String> {
        match self.account.ready() {
            Some(Some(queries::Seat::Account(account))) => account
                .agents
                .iter()
                .find(|agent| agent.number == number)
                .map(|agent| agent.name.clone()),
            _ => None,
        }
    }

    pub(crate) fn submit_create_agent(&mut self, cx: &mut Context<Self>) {
        let name = self.create_agent.text.text().trim().to_string();
        if name.is_empty() {
            self.create_agent.problem = Some(Problem::Empty);
            cx.notify();
            return;
        }
        let op = identity::Op::CreateAgent { name };
        self.submit_agent_op(op, |v| &mut v.create_agent, cx);
    }

    /// The agent's key request: the hex of an `AddKey` whose consent the
    /// new key signed, for one of the reader's agents; this key submits it
    /// as the manager.
    pub(crate) fn submit_agent_key(&mut self, cx: &mut Context<Self>) {
        let mine = |account: u64| {
            matches!(self.account.ready(), Some(Some(queries::Seat::Account(a)))
            if a.agents.iter().any(|agent| {
                agent.number == account && agent.standing() != identity::Standing::Revoked
            }))
        };
        let op = abi::unhex(self.agent_key.text.text().trim())
            .and_then(|bytes| abi::decode::<identity::Op>(&bytes).ok())
            .filter(
                |op| matches!(op, identity::Op::AddKey { consent, .. } if mine(consent.account)),
            );
        let Some(op) = op else {
            self.agent_key.problem = Some(Problem::NotAKeyRequest);
            cx.notify();
            return;
        };
        self.submit_agent_op(op, |v| &mut v.agent_key, cx);
    }

    /// Suspends, resumes or revokes an agent: one press. Suspend and
    /// Revoke are confirmed by the host, natively, before the key signs
    /// them, so the view asks nothing of its own.
    pub(crate) fn set_standing(&mut self, op: identity::Op, cx: &mut Context<Self>) {
        self.submit_agent_op(op, |v| &mut v.agent_standing, cx);
    }

    /// Submits `op` from `form`; done, the form clears and the account is
    /// read again.
    fn submit_agent_op(
        &mut self,
        op: identity::Op,
        form: impl Fn(&mut Settings) -> &mut Form + 'static,
        cx: &mut Context<Self>,
    ) {
        if form(self).busy {
            return;
        }
        let pending = form(self);
        pending.busy = true;
        pending.problem = None;
        cx.notify();
        let ask = cx.host().ask::<Submit<Identity>>(op);
        cx.land(ask, move |view, result, cx| {
            let form = form(view);
            form.busy = false;
            match result {
                Ok(_) => form.text.reset(""),
                Err(refusal) => form.problem = Some(Problem::Refused(refusal.message)),
            }
            // a rename that landed closes its field
            view.rename_agent.retain(|_, form| {
                form.busy || form.problem.is_some() || !form.text.text().is_empty()
            });
            view.refresh_account(cx);
            cx.notify();
        })
        .detach();
    }

    pub(crate) fn mint_invite(&mut self, cx: &mut Context<Self>) {
        self.copied = Loadable::Idle;
        let ask = cx.host().ask::<InviteCreate>(CreateInvite {
            ttl_days: TTL[self.ttl],
        });
        // a new invite: the one shown is no longer the one being minted
        self.invite = Loadable::Idle;
        cx.load(self, ask, |view| &mut view.invite);
    }

    pub(crate) fn copy_invite(&mut self, cx: &mut Context<Self>) {
        let Some(invite) = self.invite.ready() else {
            return;
        };
        let ask = cx.host().ask::<ClipboardWrite>(invite.invite.clone());
        // each press copies again and says so again
        self.copied = Loadable::Idle;
        cx.load(self, ask, |view| &mut view.copied);
    }
}
