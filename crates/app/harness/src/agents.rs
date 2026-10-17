//! The agents harness runs: program accounts of the agent category whose script calls harness.
//! An agent's setup is what identity says of it, its configuration file in `files` read at the
//! head, and the model record that file names. A mention or a direct message it answers opens
//! or continues its thread for that place, and its manager closes a thread.
use ::program::{
    BlobId, Ctx, Error, ExecCtx, Principal, QueryCtx, not_found, unauthorized, unexpected_reply,
};
use abi::role::identity::{AccountNumber, Category, Kind, Profile, Standing};
use attribution::{Actor, Change, Kind as Changed, Reason, Source};

use crate::state::{BY_AGENT, COUNT, LAST, MODELS, PLACES, THREADS, newest_first};
use crate::turns::{converse, say};
use crate::{AgentSetup, Asking, Body, Config, Configured, Context, PROGRAM, ThreadRow};

pub(crate) struct Agent {
    pub number: AccountNumber,
    pub name: String,
    pub manager: AccountNumber,
    pub path: String,
}

fn acting(ctx: &ExecCtx) -> Result<AccountNumber, Error> {
    match ctx.sender()? {
        Principal::Account(number) => Ok(number),
        Principal::Root => Err(unauthorized("a thread is closed by its agent's manager")),
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
    Ok(Some(AgentSetup {
        agent: agent.number,
        manager: agent.manager,
        standing,
        path: agent.path,
        configured,
    }))
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
    Ok(by_manager || ownership(ctx, agent.number)?.allows(Some(agent.manager), actor))
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
         - Conversation: {title}. Each message that reaches this conversation names who wrote \
         it and where.\n\
         - Writes: everything written here is written as account {number}, and the network \
         allows only what it allows that account.\n\
         - Tools: each tool call runs in a later block, and its result arrives before the next \
         model call.\n\
         - Chat: chat_post posts to a channel; reply_to puts the post in a thread.",
        name = agent.name,
        number = agent.number,
        manager = agent.manager,
    )
}

fn opened(ctx: &ExecCtx, agent: &Agent, place: String, title: String) -> Result<ThreadRow, Error> {
    if let Some(id) = PLACES.get(ctx, &(agent.number, place.clone()))? {
        let existing = thread(ctx, id)?;
        if !existing.closed {
            return Ok(existing);
        }
    }
    let id = COUNT.get(ctx)?.unwrap_or(0) + 1;
    let row = ThreadRow {
        id,
        agent: agent.number,
        manager: agent.manager,
        place: place.clone(),
        title,
        opened: ctx.env().height,
        closed: false,
        frames: 0,
        last_input: 0,
        seen: 0,
        last_stop: None,
        asking: Asking::Idle,
        open_calls: 0,
        held: 0,
        sealed: 0,
        base: 1,
        tail_from: 1,
        summary: None,
        context: Context::Measured(0),
        compact_before: None,
        last_height: 0,
        last_time: 0,
    };
    COUNT.put(ctx, &id);
    BY_AGENT.insert(ctx, &(agent.number, newest_first(id)));
    PLACES.put(ctx, &(agent.number, place), &id);
    Ok(row)
}

struct Designation {
    place: String,
    title: String,
    text: String,
}

fn name_of(ctx: &QueryCtx, number: AccountNumber) -> String {
    match profile(ctx, number) {
        Ok(Some(profile)) => format!("{} (account {number})", profile.name),
        _ => format!("account {number}"),
    }
}

fn designation(
    ctx: &QueryCtx,
    change: &Change,
    actor: AccountNumber,
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
        return Ok(Designation {
            title: place.clone(),
            place,
            text,
        });
    };
    let verb = match &change.reason {
        Reason::Mention => "mentioned you",
        _ => "wrote to you directly",
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
        title: format!("chat channel {}", row.channel_id),
        text,
    })
}

fn designates(reason: &Reason) -> bool {
    match reason {
        Reason::Mention => true,
        Reason::Defined(name) => name == chat::attributed::DIRECT,
        _ => false,
    }
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
    let by_another = actor != change.recipient;
    let told_already =
        LAST.get(ctx, &change.recipient)? == Some((change.source.clone(), change.revision));
    let heeded = added && designates(&change.reason) && by_another && !told_already;
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
    let Designation { place, title, text } = designation(ctx, &change, actor)?;
    let mut thread = opened(ctx, &agent, place, title)?;
    let author = Principal::Account(actor);
    say(ctx, &mut thread, Body::Post { author, text });
    THREADS.put(ctx, &thread.id, &thread);
    Ok(Some(thread.id))
}
