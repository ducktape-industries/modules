//! The vote, for a reader whose seated key votes for a validator (valset's
//! `Voices`): the motions that wait, a change of a module's code, the
//! registry's scheduled changes, and the presses that vote on them. A
//! press the reader's uncast voices would carry to the quorum enacts, so
//! it waits for a yes first ([`Voting::confirming`]). The words for a
//! motion are here, pure, so each is a unit test.
use abi::hex;
use ducktape_view_guest::prelude::*;
use module_registry::{Change, Entry, Scheduled};
use valset::{Motion, Op, Valset, Voted};

use crate::Nodes;
use crate::queries::{self, Motions};

/// How far past the tip a change lands by default: about ten minutes.
const LEAD_MS: u64 = 10 * 60 * 1000;

#[derive(Default)]
pub(crate) struct Voting {
    /// the seated key, as the session names it
    pub(crate) signer: Option<Vec<u8>>,
    /// the validators the seated key votes for; empty: it does not vote
    pub(crate) voices: Vec<Vec<u8>>,
    pub(crate) motions: Loadable<Motions>,
    /// the programs the registry runs now
    pub(crate) roster: Loadable<Vec<Entry>>,
    pub(crate) scheduled: Loadable<Vec<Scheduled>>,
    /// a press that enacts, waiting for the reader's yes
    pub(crate) confirming: Option<Motion>,
    /// a vote in flight: no other press until it lands
    pub(crate) busy: bool,
    /// what the last vote did, or the refusal it met
    pub(crate) outcome: Option<Result<String, String>>,
    pub(crate) change: ChangeForm,
}

/// Change a module: the program picked, the code typed, the height.
#[derive(Default)]
pub(crate) struct ChangeForm {
    pub(crate) program: Option<String>,
    pub(crate) code: TextField,
    pub(crate) height: TextField,
    /// why the last Propose built no motion
    pub(crate) problem: Option<String>,
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

    fn entries(&self) -> &[Entry] {
        self.roster.ready().map_or(&[], Vec::as_slice)
    }

    pub(crate) fn words(&self, motion: &Motion) -> String {
        words(motion, self.entries())
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
                        view.read_registry(cx);
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
    /// registry's live heads.
    pub(crate) fn read_registry(&mut self, cx: &mut Context<Self>) {
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

    pub(crate) fn confirm(&mut self, cx: &mut Context<Self>) {
        if let Some(motion) = self.voting.confirming.take() {
            self.vote(motion, cx);
        }
        cx.notify();
    }

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
            view.voting.outcome = Some(match answer {
                Ok(output) => Ok(match abi::decode::<Voted>(&output) {
                    Ok(Voted::Enacted) => "Enacted".to_owned(),
                    Ok(Voted::Counted { votes, needed }) => {
                        format!("Vote counted: {votes} of {needed}")
                    }
                    Err(_) => "Voted".to_owned(),
                }),
                Err(refusal) => Err(refusal.message),
            });
            view.read_voices(cx);
            cx.notify();
        })
        .detach();
    }

    pub(crate) fn dismiss_outcome(&mut self, cx: &mut Context<Self>) {
        self.voting.outcome = None;
        cx.notify();
    }

    /// Picks the program to change; an empty height takes the default.
    pub(crate) fn pick(&mut self, program: String, cx: &mut Context<Self>) {
        self.voting.change.program = Some(program);
        self.voting.change.problem = None;
        if self.voting.change.height.text().trim().is_empty()
            && let Some(status) = self.status.ready()
        {
            let height = default_height(status.height, status.block_time_ms);
            self.voting.change.height.reset(height.to_string());
        }
        cx.notify();
    }

    /// Change a module's Propose: the motion it builds, pressed.
    pub(crate) fn propose(&mut self, cx: &mut Context<Self>) {
        let tip = self.status.ready().map_or(0, |status| status.height);
        let form = &self.voting.change;
        let built = change(
            form.program.as_deref(),
            &form.code.text(),
            &form.height.text(),
            tip,
            self.voting.entries(),
        );
        match built {
            Ok(motion) => {
                self.voting.change.problem = None;
                self.press(motion, cx);
            }
            Err(problem) => {
                self.voting.change.problem = Some(problem);
                cx.notify();
            }
        }
    }
}

/// The block about [`LEAD_MS`] past `tip`, at the node's block time.
pub(crate) fn default_height(tip: u64, block_time_ms: u64) -> u64 {
    tip + LEAD_MS.div_ceil(block_time_ms.max(1))
}

/// The motion Change a module builds: `program` set to the code `code`
/// names at `height`, its params kept; or why it builds none.
pub(crate) fn change(
    program: Option<&str>,
    code: &str,
    height: &str,
    tip: u64,
    roster: &[Entry],
) -> Result<Motion, String> {
    let program = program.ok_or("Pick a program to change.")?;
    let current = roster
        .iter()
        .find(|entry| entry.program == program)
        .ok_or_else(|| format!("{program} no longer runs here."))?;
    let code = code_id(code).ok_or("A code id is sha256: and 64 hex digits.")?;
    let height: u64 = height
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
    let op = module_registry::Op::Schedule(Scheduled {
        height,
        change: Change::Set(Entry {
            program: program.to_owned(),
            code,
            params: current.params.clone(),
        }),
    });
    Ok(Motion::Registry {
        op: abi::encode(&op),
    })
}

/// A published code's id as a release names it: `sha256:<64 hex>`, or the
/// hex alone.
pub(crate) fn code_id(text: &str) -> Option<abi::BlobId> {
    let text = text.trim();
    let digits = text.strip_prefix("sha256:").unwrap_or(text);
    let digest: [u8; 32] = abi::unhex(digits)?.try_into().ok()?;
    Some(abi::BlobId::Sha256(digest))
}

/// The motion Cancel builds for a scheduled change.
pub(crate) fn cancel(scheduled: &Scheduled) -> Motion {
    let op = module_registry::Op::Cancel {
        height: scheduled.height,
        program: scheduled.change.program().to_owned(),
    };
    Motion::Registry {
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

/// What a motion does, in words; a registry one decoded, never as bytes.
pub(crate) fn words(motion: &Motion, roster: &[Entry]) -> String {
    match motion {
        Motion::Promote { key } => format!("Promote {} to validator", key_words(key)),
        Motion::Demote { key } => format!("Demote {} to resident", key_words(key)),
        Motion::Remove { key } => format!("Remove {} from the network", key_words(key)),
        Motion::Door { open: true } => "Open the door: anyone enrolls without an invite".into(),
        Motion::Door { open: false } => "Close the door: enrolling takes an invite".into(),
        Motion::Registry { op } => match abi::decode::<module_registry::Op>(op) {
            Ok(module_registry::Op::Schedule(scheduled)) => scheduled_words(&scheduled, roster),
            Ok(module_registry::Op::Cancel { height, program }) => format!(
                "Cancel {program}'s change at block {}",
                design::grouped(height)
            ),
            Ok(op) => module_registry::describe(&op).title,
            Err(_) => "A registry change this screen can't read".into(),
        },
    }
}

/// A scheduled change in words: the program, the code, whether its params
/// change (against what runs now), the height.
pub(crate) fn scheduled_words(scheduled: &Scheduled, roster: &[Entry]) -> String {
    let at = design::grouped(scheduled.height);
    match &scheduled.change {
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
