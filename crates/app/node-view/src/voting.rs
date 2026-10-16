//! The vote, for a reader whose seated key votes for a validator (valset's
//! `Voices`): the motions that wait, the programs role's scheduled changes and
//! who made each, and the presses that vote on them. A program is changed
//! in Explorer's Programs; this list stays here, in valset's view, which
//! only the vote changes, so a maintainer's change is seen where no
//! maintainer can hide it. A press the reader's uncast voices would carry
//! to the quorum enacts, so it waits for a yes first
//! ([`Voting::confirming`]). The words for a motion are here, pure, so
//! each is a unit test.
use abi::hex;
use abi::role::halo;
use programs::{Change, Entry, Pending};
use valset::{Motion, Op, Valset, Voted};
use view::methods::{self, Submitted};
use view::prelude::*;

use crate::Nodes;
use crate::queries::{self, Motions, Names, Node, Schedule};

#[derive(Default)]
pub(crate) struct Voting {
    /// the seated key, as the session names it
    pub(crate) signer: Option<Vec<u8>>,
    /// the validators the seated key votes for; empty: it does not vote
    pub(crate) voices: Vec<Vec<u8>>,
    pub(crate) motions: Loadable<Motions>,
    /// the programs the programs role runs now
    pub(crate) roster: Loadable<Vec<Entry>>,
    pub(crate) scheduled: Loadable<Schedule>,
    /// a press that enacts, waiting for the reader's yes
    pub(crate) confirming: Option<Motion>,
    /// a vote in flight: no other press until it lands
    pub(crate) busy: bool,
    /// what the last vote did, or the refusal it met
    pub(crate) outcome: Option<Result<String, String>>,
}

impl Voting {
    pub(crate) fn votes(&self) -> bool {
        !self.voices.is_empty()
    }

    /// Whether a press may vote now: none in flight, and the motions read,
    /// so a press that enacts is known to and asks first.
    pub(crate) fn pressable(&self) -> bool {
        !self.busy && self.motions.ready().is_some()
    }

    /// The validators that voted for `motion` so far: none for one that
    /// does not wait yet.
    pub(crate) fn voters(&self, motion: &Motion) -> &[Vec<u8>] {
        self.motions
            .ready()
            .and_then(|motions| motions.pending.iter().find(|p| &p.motion == motion))
            .map_or(&[], |pending| pending.voters.as_slice())
    }

    /// Whether the reader's uncast voices carry `motion` to the quorum; a
    /// press waits for the motions ([`pressable`](Voting::pressable)).
    pub(crate) fn enacts(&self, motion: &Motion) -> bool {
        let Some(motions) = self.motions.ready() else {
            return true;
        };
        let voters = self.voters(motion);
        let uncast = self.voices.iter().filter(|v| !voters.contains(v)).count();
        (voters.len() + uncast) as u64 >= motions.needed
    }

    /// The reader's voices that have not voted for `motion`.
    pub(crate) fn uncast(&self, motion: &Motion) -> usize {
        let voters = self.voters(motion);
        self.voices.iter().filter(|v| !voters.contains(v)).count()
    }

    /// The programs the programs role runs now; none until read.
    pub(crate) fn entries(&self) -> &[Entry] {
        self.roster.ready().map_or(&[], Vec::as_slice)
    }

    pub(crate) fn words(&self, motion: &Motion) -> String {
        let names = self.motions.ready().map(|motions| &motions.names);
        words(motion, self.entries(), names.unwrap_or(&Names::new()))
    }
}

impl Nodes {
    pub(crate) fn session_changed(&mut self, session: Session, cx: &mut Context<Self>) {
        let signer = abi::unhex(&session.signer).filter(|key| !key.is_empty());
        if signer != self.voting.signer {
            // another reader's voices are not this one's
            self.voting.voices.clear();
            self.voting.confirming = None;
            self.voting.signer = signer;
        }
        self.read_voices(cx);
    }

    /// Whom the seated key votes for, then, while it votes, what the vote
    /// reads: the boot, a session, a vote's reply, a valset head.
    pub(crate) fn read_voices(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.voting.signer.clone() else {
            return;
        };
        let ask = queries::voices(cx.host(), key.clone());
        cx.land(ask, move |view, answer, cx| {
            // a session moved on while this was asked
            if view.voting.signer.as_ref() != Some(&key) {
                return;
            }
            match answer {
                Ok(voices) => {
                    view.voting.voices = voices;
                    if view.voting.votes() {
                        view.read_motions(cx);
                        view.read_programs(cx);
                    }
                }
                Err(refusal) => cx.log_refused("whom this key votes for", &refusal),
            }
            cx.notify();
        })
        .detach();
    }

    pub(crate) fn read_motions(&mut self, cx: &mut Context<Self>) {
        let work = queries::motions(cx.host());
        cx.load(self, work, |view| &mut view.voting.motions);
    }

    /// The roster and the schedule: on a vote's first read and on the
    /// `programs`'s live heads.
    pub(crate) fn read_programs(&mut self, cx: &mut Context<Self>) {
        if !self.voting.votes() {
            return;
        }
        let roster = queries::roster(cx.host());
        cx.load(self, roster, |view| &mut view.voting.roster);
        let scheduled = queries::scheduled(cx.host());
        cx.load(self, scheduled, |view| &mut view.voting.scheduled);
    }

    /// A press on a motion: voted now, or, where it enacts, once the reader
    /// says yes.
    pub(crate) fn press(&mut self, motion: Motion, cx: &mut Context<Self>) {
        if !self.voting.pressable() {
            return;
        }
        match self.voting.enacts(&motion) {
            true => self.voting.confirming = Some(motion),
            false => self.vote(motion, cx),
        }
        cx.notify();
    }

    /// The reader's yes to the press that waits for one: its vote goes.
    pub(crate) fn confirm(&mut self, cx: &mut Context<Self>) {
        if let Some(motion) = self.voting.confirming.take() {
            self.vote(motion, cx);
        }
        cx.notify();
    }

    /// The reader's Not now: the press that waits is dropped, unsent.
    pub(crate) fn hold(&mut self, cx: &mut Context<Self>) {
        self.voting.confirming = None;
        cx.notify();
    }

    fn vote(&mut self, motion: Motion, cx: &mut Context<Self>) {
        self.voting.busy = true;
        self.voting.outcome = None;
        let ask = cx.host().ask::<Submit<Valset>>(Op::Vote(motion));
        cx.land(ask, |view, answer, cx| {
            view.voting.busy = false;
            view.voting.outcome = Some(said(answer));
            view.read_voices(cx);
            cx.notify();
        })
        .detach();
    }

    /// The reader is done with the last vote's outcome.
    pub(crate) fn dismiss_outcome(&mut self, cx: &mut Context<Self>) {
        self.voting.outcome = None;
        cx.notify();
    }
}

/// What a vote came to, in the Expo app's words (mobile
/// `src/network/nodes.ts` `votedWords`, `src/write/words.ts` `failure`):
/// what a landed one did; else why it stopped, told apart by whether it
/// was refused, never sent (nothing reached the node, or no key signed
/// it: safe to send again), or may have gone through.
pub(crate) fn said(answer: Result<Submitted, Error>) -> Result<String, String> {
    match answer {
        Ok(Submitted::Applied { output, .. }) => Ok(match abi::decode::<Voted>(&output) {
            Ok(Voted::Enacted) => "Enacted".to_owned(),
            Ok(Voted::Counted { votes, needed }) => format!("Vote counted: {votes} of {needed}"),
            Err(_) => "Voted".to_owned(),
        }),
        Ok(Submitted::InDoubt { .. }) => Err(
            "This may have gone through: no answer came back. What you see is read again from the node."
                .into(),
        ),
        Err(refusal) if refusal.code == methods::refusal::RPC_CLIENT => Err(
            "Not sent: the node can’t be reached. Check the connection, then try again.".into(),
        ),
        Err(refusal) if refusal.code == methods::refusal::SESSION_LOCKED => {
            Err(format!("Not sent: {}", refusal.message))
        }
        Ok(Submitted::Refused { refusal, .. }) | Err(refusal) => {
            Err(format!("That didn’t go through: {}", refusal.message))
        }
    }
}

/// The motions a voter can put on a member, each with its slug and its
/// label: Demote and Remove a validator, Promote a resident.
pub(crate) fn motions_on(node: &Node) -> Vec<(&'static str, &'static str, Motion)> {
    let key = node.key.clone();
    match node.validator {
        true => vec![
            ("demote", "Demote", Motion::Demote { key: key.clone() }),
            ("remove", "Remove", Motion::Remove { key }),
        ],
        false => vec![("promote", "Promote", Motion::Promote { key })],
    }
}

/// The motion Propose cancel builds for a scheduled change.
pub(crate) fn cancel(pending: &Pending) -> Motion {
    let op = programs::Op::Cancel {
        height: pending.height,
        program: pending.change.program().to_owned(),
    };
    Motion::Role {
        role: abi::role::PROGRAMS.into(),
        op: abi::encode(&op),
    }
}

/// A code id as the screen shows it: `sha256:1a2b3c4d…9f0e`.
pub(crate) fn code_words(code: &abi::BlobId) -> String {
    let kind = match code.kind() {
        abi::HashKind::Sha256 => "sha256",
        abi::HashKind::Sha1 => "sha1",
    };
    format!("{kind}:{}", design::short_hex(&hex(code.digest())))
}

fn key_words(key: &[u8]) -> String {
    design::short_hex(&hex(key))
}

/// An account by its name, or its number while identity names none.
pub(crate) fn named(names: &Names, account: u64) -> String {
    names
        .get(&account)
        .cloned()
        .unwrap_or_else(|| format!("account {account}"))
}

/// What a motion does, in words: a programs or halo role's op decoded, another role's op
/// and a binding by the role's name, never as bytes.
pub(crate) fn words(motion: &Motion, roster: &[Entry], names: &Names) -> String {
    match motion {
        Motion::Promote { key } => format!("Promote {} to validator", key_words(key)),
        Motion::Demote { key } => format!("Demote {} to resident", key_words(key)),
        Motion::Remove { key } => format!("Remove {} from the network", key_words(key)),
        Motion::Door { open } => {
            let (door, meaning) = crate::words::door(*open);
            format!("{door}: {meaning}")
        }
        Motion::Role { role, op } if role == abi::role::PROGRAMS => match abi::decode(op) {
            Ok(programs::Op::Schedule(scheduled)) => {
                scheduled_words(scheduled.height, &scheduled.change, roster)
            }
            Ok(programs::Op::Cancel { height, program }) => format!(
                "Cancel {program}'s change at block {}",
                design::grouped(height)
            ),
            Ok(programs::Op::Grant(grant)) => {
                format!("Grant {} {}", named(names, grant.account), grant.scope)
            }
            Ok(programs::Op::Revoke(grant)) => {
                format!("Revoke {} {}", named(names, grant.account), grant.scope)
            }
            Err(_) => "A program change this screen can't read".into(),
        },
        Motion::Role { role, op } if role == abi::role::HALO => match abi::decode(op) {
            Ok(halo::Op::Open { name, cap }) => format!("Open lane {name} at class {cap:?}"),
            Ok(halo::Op::Set {
                name,
                cap,
                enabled: true,
            }) => format!("Serve lane {name} at class {cap:?}"),
            Ok(halo::Op::Set {
                name,
                enabled: false,
                ..
            }) => format!("Pause lane {name}"),
            Ok(halo::Op::Close { name }) => format!("Close lane {name}"),
            Err(_) => "A lane change this screen can't read".into(),
        },
        Motion::Role { role, .. } => format!("An op for the {role} role"),
        Motion::Bind {
            role,
            program: Some(program),
        } => format!("Bind the {role} role to {program}"),
        Motion::Bind {
            role,
            program: None,
        } => format!("Unbind the {role} role"),
    }
}

/// A scheduled change in words: the program, the code, whether its params
/// change (against what runs now), the height.
pub(crate) fn scheduled_words(height: u64, change: &Change, roster: &[Entry]) -> String {
    let at = design::grouped(height);
    match change {
        Change::Set(entry) => {
            let params = match roster.iter().find(|now| now.program == entry.program) {
                Some(now) if now.params == entry.params => "params unchanged",
                Some(_) => "params changed",
                None => "a new program",
            };
            format!(
                "Set {} to code {} at block {at} · {params}",
                entry.program,
                code_words(&entry.code)
            )
        }
        change => format!("{} {} at block {at}", change.verb(), change.program()),
    }
}
