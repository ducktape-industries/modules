//! The agents harness runs: a manager hires one it handed to harness, a mention or a direct
//! message it answers opens or continues its thread for that place, and its manager
//! dismisses it or closes a thread.
use abi::role::identity::{AccountNumber, Kind, Profile, Standing};
use attribution::{Actor, Change, Kind as Changed, Reason, Source};
use guest::{
    BlobId, Error, ExecCtx, HashKind, Principal, QueryCtx, invalid, not_found, unauthorized,
    unexpected_reply, wrong_state,
};

use crate::state::{AGENTS, BY_AGENT, COUNT, LAST, PLACES, THREADS, newest_first};
use crate::turns::{advance, say};
use crate::{
    AgentRow, Asking, Body, MAX_MODEL_BYTES, MAX_OUTPUT_TOKENS, MAX_POST_BYTES, MAX_SYSTEM_BYTES,
    MODULE, PART, ThreadRow, anthropic, attributed_as, tools,
};

fn acting(ctx: &ExecCtx) -> Result<AccountNumber, Error> {
    match ctx.sender()? {
        Principal::Account(number) => Ok(number),
        Principal::Root => Err(unauthorized("an agent is hired by its manager's account")),
    }
}

fn profile(ctx: &QueryCtx, number: AccountNumber) -> Result<Option<Profile>, Error> {
    use abi::role::identity::{Query, Reply};
    let identity = &ctx.env().roles.identity;
    match ctx.query::<Query, Reply>(identity, &Query::Profile(number))? {
        Reply::Profile(profile) => Ok(profile),
        other => Err(unexpected_reply(identity, "Profile", &other)),
    }
}

fn runs_here(profile: &Profile) -> Option<AccountNumber> {
    match &profile.kind {
        Kind::Managed {
            manager,
            standing: Standing::Active,
            runner: Some(runner),
            ..
        } if runner == MODULE => Some(*manager),
        _ => None,
    }
}

fn audience(ctx: &QueryCtx, agent: AccountNumber) -> Result<identity::Audience, Error> {
    let identity = &ctx.env().roles.identity;
    let asked = identity::Query::Audience { number: agent };
    match ctx.query::<identity::Query, identity::Reply>(identity, &asked)? {
        identity::Reply::Audience(audience) => Ok(audience),
        other => Err(unexpected_reply(identity, "Audience", &other)),
    }
}

fn answers(ctx: &QueryCtx, agent: &AgentRow, actor: AccountNumber) -> Result<bool, Error> {
    let by_manager = actor == agent.manager;
    Ok(by_manager
        || match audience(ctx, agent.agent)? {
            identity::Audience::Manager => false,
            identity::Audience::Accounts(numbers) => numbers.contains(&actor),
            identity::Audience::Anyone => true,
        })
}

pub(crate) fn thread(ctx: &QueryCtx, id: u64) -> Result<ThreadRow, Error> {
    THREADS
        .get(ctx, &id)?
        .ok_or_else(|| not_found(format!("no thread {id}")))
}

fn checked(system: &str, model: &str, max_tokens: u32) -> Result<(), Error> {
    if system.len() > MAX_SYSTEM_BYTES {
        return Err(invalid(format!(
            "a system prompt is at most {MAX_SYSTEM_BYTES} bytes"
        )));
    }
    let model_named = !model.is_empty()
        && model.len() <= MAX_MODEL_BYTES
        && model.bytes().all(|byte| byte.is_ascii_graphic());
    if !model_named {
        return Err(invalid(format!(
            "a model is 1 to {MAX_MODEL_BYTES} printable ASCII bytes"
        )));
    }
    if !(1..=MAX_OUTPUT_TOKENS).contains(&max_tokens) {
        return Err(invalid(format!("max_tokens is 1 to {MAX_OUTPUT_TOKENS}")));
    }
    Ok(())
}

pub(crate) fn hire(
    ctx: &ExecCtx,
    agent: AccountNumber,
    system: String,
    model: String,
    max_tokens: u32,
) -> Result<(), Error> {
    let manager = acting(ctx)?;
    checked(&system, &model, max_tokens)?;
    let profile = profile(ctx, agent)?.ok_or_else(|| not_found(format!("no account {agent}")))?;
    let Kind::Managed {
        manager: managing, ..
    } = &profile.kind
    else {
        return Err(wrong_state(format!("account {agent} is no one's agent")));
    };
    if *managing != manager {
        return Err(unauthorized(format!(
            "account {manager} does not manage account {agent}"
        )));
    }
    if runs_here(&profile).is_none() {
        return Err(wrong_state(format!(
            "account {agent} is not an active agent handed to {MODULE}: its manager sets identity's SetRunner first"
        )));
    }
    let row = AgentRow {
        agent,
        manager,
        name: profile.name,
        system,
        model,
        max_tokens,
        hired: ctx.env().height,
    };
    AGENTS.put(ctx, &agent, &row);
    let subscribe = attribution::Op::Subscribe {
        deliver: attributed_as(),
    };
    ctx.emit(attribution::MODULE, &subscribe, guest::Reply::None);
    Ok(())
}

pub(crate) fn dismiss(ctx: &ExecCtx, agent: AccountNumber) -> Result<(), Error> {
    let row = AGENTS
        .get(ctx, &agent)?
        .ok_or_else(|| not_found(format!("account {agent} is not hired here")))?;
    if acting(ctx)? != row.manager {
        return Err(unauthorized(format!("agent {agent} is its manager's")));
    }
    AGENTS.remove(ctx, &agent);
    LAST.remove(ctx, &agent);
    Ok(())
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

fn preamble(agent: &AgentRow, title: &str) -> String {
    format!(
        "You are {name}, an agent on a ducktape network: account {number}, managed by account \
         {manager}. People reach you by mentioning you or by writing to you directly in chat; \
         each such message arrives here, naming who wrote it and where. This conversation is \
         {title}. Answer with chat_post in the same channel, with reply_to set when the \
         message was in a thread. Everything you write is written as your account, and the \
         network allows you only what it allows that account. Every tool call runs in a later \
         block: you get a receipt at once and its result as a later message.",
        name = agent.name,
        number = agent.agent,
        manager = agent.manager,
    )
}

fn opened(
    ctx: &ExecCtx,
    agent: &AgentRow,
    place: String,
    title: String,
) -> Result<ThreadRow, Error> {
    if let Some(id) = PLACES.get(ctx, &(agent.agent, place.clone()))? {
        let existing = thread(ctx, id)?;
        if !existing.closed {
            return Ok(existing);
        }
    }
    let id = COUNT.get(ctx)?.unwrap_or(0) + 1;
    let mut system = preamble(agent, &title);
    if !agent.system.is_empty() {
        system.push_str("\n\n");
        system.push_str(&agent.system);
    }
    let mut row = ThreadRow {
        id,
        agent: agent.agent,
        manager: agent.manager,
        place: place.clone(),
        title,
        system,
        model: agent.model.clone(),
        max_tokens: agent.max_tokens,
        opened: ctx.env().height,
        closed: false,
        frames: 0,
        last_input: 0,
        seen: 0,
        last_stop: None,
        asking: Asking::Idle,
        failures: 0,
        turns_since_post: 0,
        open_calls: 0,
        held: 0,
        header: BlobId::Sha256([0; 32]),
        sealed: 0,
        tail_from: 1,
    };
    let header = anthropic::header(&row, tools::definitions());
    row.header = ctx.blob_put(HashKind::Sha256, PART, header.into_bytes())?;
    COUNT.put(ctx, &id);
    BY_AGENT.insert(ctx, &(agent.agent, newest_first(id)));
    PLACES.put(ctx, &(agent.agent, place), &id);
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

fn clipped(mut text: String) -> String {
    if text.len() <= MAX_POST_BYTES {
        return text;
    }
    let mut end = MAX_POST_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text
}

fn designation(
    ctx: &QueryCtx,
    change: &Change,
    actor: AccountNumber,
) -> Result<Designation, Error> {
    let who = name_of(ctx, actor);
    let from_chat = change.source.program == chat::MODULE
        && change.source.object.kind == chat::attributed::MESSAGE;
    let message = match from_chat {
        true => {
            let asked = chat::Query::MessageById {
                message_id: change.source.object.id.clone(),
            };
            match ctx.query(chat::MODULE, &asked)? {
                chat::Reply::Message(row) => row,
                other => return Err(unexpected_reply(chat::MODULE, "MessageById", &other)),
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
        place: format!("{}/{}", chat::MODULE, row.channel_id),
        title: format!("chat channel {}", row.channel_id),
        text: clipped(text),
    })
}

fn designates(reason: &Reason) -> bool {
    match reason {
        Reason::Mention => true,
        Reason::Defined(name) => name == chat::attributed::DIRECT,
        _ => false,
    }
}

pub(crate) fn attributed(ctx: &ExecCtx, change: Change) -> Result<(), Error> {
    ctx.env().sent_by(attribution::MODULE)?;
    let Some(agent) = AGENTS.get(ctx, &change.recipient)? else {
        return Ok(());
    };
    let added = change.kind == Changed::Added;
    let Actor::Account(actor) = change.actor else {
        return Ok(());
    };
    let by_another = actor != agent.agent;
    let told_already =
        LAST.get(ctx, &agent.agent)? == Some((change.source.clone(), change.revision));
    let heeded = added && designates(&change.reason) && by_another && !told_already;
    if !heeded {
        return Ok(());
    }
    let still_runs = profile(ctx, agent.agent)?
        .as_ref()
        .and_then(runs_here)
        .is_some();
    if !still_runs || !answers(ctx, &agent, actor)? {
        return Ok(());
    }
    LAST.put(ctx, &agent.agent, &(change.source.clone(), change.revision));
    let Designation { place, title, text } = designation(ctx, &change, actor)?;
    let mut thread = opened(ctx, &agent, place, title)?;
    let author = Principal::Account(actor);
    say(ctx, &mut thread, Body::Post { author, text });
    thread.turns_since_post = 0;
    thread.failures = 0;
    advance(ctx, &mut thread)?;
    THREADS.put(ctx, &thread.id, &thread);
    Ok(())
}
