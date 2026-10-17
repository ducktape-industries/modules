//! The agents harness runs: program accounts of the agent category whose script calls harness.
//! An agent's setup is what identity says of it, its configuration file in `files` read at the
//! head, and the model record that file names. A mention, a direct message or a reply in a chat
//! thread it took part in, from someone it answers, continues its one current thread, from any
//! place, and its manager closes a thread.
use ::program::{
    BlobId, Ctx, Error, ExecCtx, Principal, QueryCtx, not_found, unauthorized, unexpected_reply,
};
use abi::role::identity::{AccountNumber, Category, Kind, Profile, Standing};
use attribution::{Actor, Change, Kind as Changed, Reason, Source};

use crate::conversations::{conversation, current};
use crate::state::{LAST, MODELS, PLACES, THREADS};
use crate::turns::{converse, say};
use crate::{AgentSetup, Body, Config, Configured, PROGRAM, ThreadRow};

pub(crate) struct Agent {
    pub number: AccountNumber,
    pub name: String,
    pub manager: AccountNumber,
    pub path: String,
}

fn acting(ctx: &ExecCtx) -> Result<AccountNumber, Error> {
    match ctx.sender()? {
        Principal::Account(number) => Ok(number),
        Principal::Root => Err(unauthorized("an agent's threads are its manager's")),
    }
}

fn profile(ctx: &QueryCtx, number: AccountNumber) -> Result<Option<Profile>, Error> {
    use abi::role::identity::{Query, Reply};
    let identity = &ctx.env().role(abi::role::IDENTITY)?.clone();
    match ctx.query::<Query, Reply>(identity, &Query::Profile(number))? {
        Reply::Profile(profile) => Ok(profile),
        other => Err(unexpected_reply(identity, "Profile", &other)),
    }
}

fn harnessed(profile: &Profile) -> Option<(Agent, Standing)> {
    let Kind::Program {
        manager,
        category: Category::Agent,
        standing,
        script: Some(script),
    } = &profile.kind
    else {
        return None;
    };
    let calls_harness = script.calls == PROGRAM;
    let agent = Agent {
        number: profile.number,
        name: profile.name.clone(),
        manager: *manager,
        path: script.config.clone(),
    };
    calls_harness.then_some((agent, *standing))
}

fn runs_here(profile: &Profile) -> Option<Agent> {
    let (agent, standing) = harnessed(profile)?;
    let active = standing == Standing::Active;
    active.then_some(agent)
}

pub(crate) fn running(ctx: &QueryCtx, number: AccountNumber) -> Result<Option<Agent>, Error> {
    Ok(profile(ctx, number)?.as_ref().and_then(runs_here))
}

fn read(ctx: &QueryCtx, path: &str) -> Result<(BlobId, Vec<u8>), Error> {
    let mut at = None;
    let mut bytes = Vec::new();
    loop {
        let asked = files::Query::Read {
            path: path.to_owned(),
            at,
            offset: bytes.len() as u64,
            len: u64::MAX,
        };
        let contents = match ctx.query(files::PROGRAM, &asked)? {
            files::Reply::Contents(contents) => contents,
            other => return Err(unexpected_reply(files::PROGRAM, "Read", &other)),
        };
        let stalled = contents.bytes.is_empty() && !contents.eof;
        bytes.extend(contents.bytes);
        if contents.eof {
            return Ok((contents.snapshot, bytes));
        }
        if stalled {
            return Err(::program::corrupt(
                "file",
                path.as_bytes(),
                "files answered no bytes before the end of the file",
            ));
        }
        at = Some(contents.snapshot);
    }
}

pub(crate) fn configured(ctx: &QueryCtx, path: &str) -> Result<Configured, Error> {
    let (snapshot, bytes) = match read(ctx, path) {
        Ok(read) => read,
        Err(error) => {
            let reason = format!("{}: {}", error.code, error.message);
            return Ok(Configured::Unreadable { reason });
        }
    };
    let config = match Config::parse(&bytes) {
        Ok(config) => config,
        Err(malformed) => {
            return Ok(Configured::Malformed {
                snapshot,
                malformed,
            });
        }
    };
    Ok(match MODELS.get(ctx, &config.model)? {
        Some(model) => Configured::Ready {
            snapshot,
            config,
            model,
        },
        None => Configured::UnknownModel { snapshot, config },
    })
}

pub(crate) fn setup(ctx: &QueryCtx, number: AccountNumber) -> Result<Option<AgentSetup>, Error> {
    let Some((agent, standing)) = profile(ctx, number)?.as_ref().and_then(harnessed) else {
        return Ok(None);
    };
    let configured = configured(ctx, &agent.path)?;
    let current = current(ctx, agent.number)?.map(|thread| thread.id);
    Ok(Some(AgentSetup {
        agent: agent.number,
        manager: agent.manager,
        standing,
        path: agent.path,
        configured,
        current,
    }))
}

pub(crate) fn managed(ctx: &ExecCtx, number: AccountNumber) -> Result<Agent, Error> {
    let Some((agent, _)) = profile(ctx, number)?.as_ref().and_then(harnessed) else {
        return Err(not_found(format!(
            "account {number} is no agent whose script calls harness"
        )));
    };
    if acting(ctx)? != agent.manager {
        return Err(unauthorized(format!(
            "agent {number}'s conversations are its manager's to start"
        )));
    }
    Ok(agent)
}

fn ownership(ctx: &QueryCtx, agent: AccountNumber) -> Result<identity::Ownership, Error> {
    let identity = &ctx.env().role(abi::role::IDENTITY)?.clone();
    let asked = identity::Query::Ownership { number: agent };
    match ctx.query::<identity::Query, identity::Reply>(identity, &asked)? {
        identity::Reply::Ownership(ownership) => Ok(ownership),
        other => Err(unexpected_reply(identity, "Ownership", &other)),
    }
}

fn answers(ctx: &QueryCtx, agent: &Agent, actor: AccountNumber) -> Result<bool, Error> {
    let by_manager = actor == agent.manager;
    if by_manager {
        return Ok(true);
    }
    let ownership = ownership(ctx, agent.number)?;
    Ok(ownership.allows(Some(agent.manager), &caller(ctx, actor)?))
}

fn caller(ctx: &QueryCtx, actor: AccountNumber) -> Result<Vec<identity::Party>, Error> {
    let account = identity::Party::Account(actor);
    let program = profile(ctx, actor)?.and_then(|profile| match profile.kind {
        Kind::System(program) => Some(identity::Party::Program(program)),
        Kind::User | Kind::Program { .. } => None,
    });
    Ok(std::iter::once(account).chain(program).collect())
}

pub(crate) fn thread(ctx: &QueryCtx, id: u64) -> Result<ThreadRow, Error> {
    THREADS
        .get(ctx, &id)?
        .ok_or_else(|| not_found(format!("no thread {id}")))
}

pub(crate) fn close(ctx: &ExecCtx, id: u64) -> Result<(), Error> {
    let mut thread = thread(ctx, id)?;
    if acting(ctx)? != thread.manager {
        return Err(unauthorized(format!(
            "thread {id} is closed by its agent's manager"
        )));
    }
    thread.closed = true;
    THREADS.put(ctx, &id, &thread);
    Ok(())
}

pub(crate) fn manual(agent: &Agent, title: &str) -> String {
    format!(
        "ducktape resource manual\n\
         - Account: {name}, account {number}, managed by account {manager}.\n\
         - Conversation: {title}. It is your one conversation: every message to you, from \
         every person and every place (channels, direct messages, threads), arrives here and \
         names who wrote it and where. Answer each where it came from: chat_post to its \
         channel, with reply_to for its thread.\n\
         - Writes: everything written here is written as account {number}, sent by harness, \
         and the network allows only what it allows that account or harness.\n\
         - Tools: each tool call runs in a later block, and its result arrives before the next \
         model call.\n\
         - Chat: chat_post posts to a channel; reply_to puts the post in a thread.\n\
         - Programs: network_schema gives any program's op, query and reply shapes; \
         network_query asks it a query and network_call sends it an op as account {number}.",
        name = agent.name,
        number = agent.number,
        manager = agent.manager,
    )
}

struct Designation {
    place: String,
    text: String,
}

fn name_of(ctx: &QueryCtx, number: AccountNumber) -> String {
    match profile(ctx, number) {
        Ok(Some(profile)) => format!("{} (account {number})", profile.name),
        _ => format!("account {number}"),
    }
}

enum Heard {
    Mention,
    Direct,
    Thread,
}

fn heard_as(reason: &Reason) -> Option<Heard> {
    match reason {
        Reason::Mention => Some(Heard::Mention),
        Reason::Defined(name) => match name.as_str() {
            chat::attributed::DIRECT => Some(Heard::Direct),
            chat::attributed::THREAD => Some(Heard::Thread),
            _ => None,
        },
        Reason::Authorship
        | Reason::Ownership
        | Reason::Assignment
        | Reason::Credit
        | Reason::Result
        | Reason::Report => None,
    }
}

fn designation(
    ctx: &QueryCtx,
    change: &Change,
    actor: AccountNumber,
    heard: Heard,
) -> Result<Designation, Error> {
    let who = name_of(ctx, actor);
    let from_chat = change.source.program == chat::PROGRAM
        && change.source.object.kind == chat::attributed::MESSAGE;
    let message = match from_chat {
        true => {
            let asked = chat::Query::MessageById {
                message_id: change.source.object.id.clone(),
            };
            match ctx.query(chat::PROGRAM, &asked)? {
                chat::Reply::Message(row) => row,
                other => return Err(unexpected_reply(chat::PROGRAM, "MessageById", &other)),
            }
        }
        false => None,
    };
    let Some(row) = message else {
        let Source { program, object } = &change.source;
        let place = format!("{program}/{}/{}", object.kind, object.id);
        let text = format!(
            "{who} related you to {program}'s {} {} ({:?}).",
            object.kind, object.id, change.reason
        );
        return Ok(Designation { place, text });
    };
    let verb = match heard {
        Heard::Mention => "mentioned you",
        Heard::Direct => "wrote to you directly",
        Heard::Thread => "continued a thread with you",
    };
    let in_thread = match row.thread {
        Some(root) => format!(", in the thread of message {root} (reply_to {root})"),
        None => format!(" (reply_to {} to answer in its thread)", row.seq),
    };
    let text = format!(
        "{who} {verb} in chat channel {channel}, message {seq}{in_thread}:\n{body}",
        channel = row.channel_id,
        seq = row.seq,
        body = row.text,
    );
    Ok(Designation {
        place: format!("{}/{}", chat::PROGRAM, row.channel_id),
        text,
    })
}

pub(crate) async fn attributed(ctx: Ctx, change: Change) -> Result<(), Error> {
    let Some(thread) = heard(&ctx, change)? else {
        return Ok(());
    };
    converse(&ctx, thread).await;
    Ok(())
}

fn heard(ctx: &ExecCtx, change: Change) -> Result<Option<u64>, Error> {
    ctx.env().sent_by(attribution::PROGRAM)?;
    let added = change.kind == Changed::Added;
    let Actor::Account(actor) = change.actor else {
        return Ok(None);
    };
    let Some(why) = heard_as(&change.reason) else {
        return Ok(None);
    };
    let by_another = actor != change.recipient;
    let told_already =
        LAST.get(ctx, &change.recipient)? == Some((change.source.clone(), change.revision));
    let heeded = added && by_another && !told_already;
    if !heeded {
        return Ok(None);
    }
    let Some(agent) = running(ctx, change.recipient)? else {
        return Ok(None);
    };
    if !answers(ctx, &agent, actor)? {
        return Ok(None);
    }
    LAST.put(
        ctx,
        &agent.number,
        &(change.source.clone(), change.revision),
    );
    let Designation { place, text } = designation(ctx, &change, actor, why)?;
    let mut thread = conversation(ctx, &agent, &place)?;
    PLACES.put(ctx, &(agent.number, place), &thread.id);
    let author = Principal::Account(actor);
    say(ctx, &mut thread, Body::Post { author, text });
    THREADS.put(ctx, &thread.id, &thread);
    Ok(Some(thread.id))
}
