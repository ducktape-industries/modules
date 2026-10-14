//! The loop consensus runs for a thread: ask the model when it has input it has not seen, hold
//! input while the loop runs, record the model's turn, run each tool call a block later, then ask again.
use ::program::{BlobId, Ctx, Error, ExecCtx, HashKind, QueryCtx, Range, all, unexpected_reply};
use provider::{RELAY, http};
use serde_json::Value;

use crate::agents::thread;
use crate::state::{AGENTS, CALLS, FRAMES, HELD, SEALED, THREADS};
use crate::tools::{self, Run, Seat};
use crate::{
    Asking, Body, CallRow, CallState, Frame, MAX_ATTEMPTS, MAX_CALL_ID_BYTES, MAX_INPUT_BYTES,
    MAX_MESSAGES, MAX_RESULT_BYTES, MAX_TURNS_PER_POST, MODEL_SERVICE, PART, Role, Said, ThreadRow,
    anthropic,
};

pub(crate) fn append(ctx: &ExecCtx, thread: &mut ThreadRow, body: Body) {
    thread.frames += 1;
    let seq = thread.frames;
    if body.role() == Some(Role::User) {
        thread.last_input = seq;
    }
    let frame = Frame {
        seq,
        height: ctx.env().height,
        time: ctx.env().time,
        body,
    };
    FRAMES.put(ctx, &(thread.id, seq), &frame);
}

pub(crate) fn say(ctx: &ExecCtx, thread: &mut ThreadRow, body: Body) {
    let looping = thread.asking != Asking::Idle;
    if !looping {
        append(ctx, thread, body);
        return;
    }
    HELD.put(ctx, &(thread.id, thread.held), &body);
    thread.held += 1;
}

fn release(ctx: &ExecCtx, thread: &mut ThreadRow) -> Result<(), Error> {
    for index in 0..thread.held {
        let key = (thread.id, index);
        if let Some(body) = HELD.get(ctx, &key)? {
            append(ctx, thread, body);
        }
        HELD.remove(ctx, &key);
    }
    thread.held = 0;
    Ok(())
}

fn serves(ctx: &QueryCtx, service: &str) -> Result<bool, Error> {
    let asked = provider::Query::Serving {
        service: service.into(),
    };
    match ctx.query(provider::PROGRAM, &asked)? {
        provider::Reply::Serving(keys) => Ok(!keys.is_empty()),
        other => Err(unexpected_reply(provider::PROGRAM, "Serving", &other)),
    }
}

async fn request(
    ctx: &Ctx,
    service: &str,
    input: Vec<u8>,
    parts: Vec<BlobId>,
) -> Result<Vec<u8>, Error> {
    let request = provider::Request {
        service: service.into(),
        input,
        parts,
    };
    let op = provider::Op::Request(request);
    ctx.call_raw(provider::PROGRAM, abi::encode(&op)).await
}

fn put_part(ctx: &ExecCtx, text: String) -> Result<BlobId, Error> {
    ctx.blob_put(HashKind::Sha256, PART, text.into_bytes())
}

fn grouped(frames: impl IntoIterator<Item = Frame>) -> Vec<(Role, Vec<Frame>)> {
    let mut groups: Vec<(Role, Vec<Frame>)> = Vec::new();
    for frame in frames {
        let Some(role) = frame.body.role() else {
            continue;
        };
        match groups.last_mut() {
            Some((last, frames)) if *last == role => frames.push(frame),
            _ => groups.push((role, vec![frame])),
        }
    }
    groups
}

fn render(ctx: &ExecCtx, thread: &mut ThreadRow) -> Result<Vec<BlobId>, Error> {
    let whole = FRAMES.prefix_of(&thread.id);
    let tail = Range::new(FRAMES.key(&(thread.id, thread.tail_from)), whole.end);
    let mut groups = grouped(FRAMES.scan(ctx, tail)?.into_iter().map(|(_, frame)| frame));
    let Some((last_role, last_frames)) = groups.pop() else {
        return Err(::program::wrong_state(
            "a thread with nothing said asks no model",
        ));
    };
    for (role, frames) in groups {
        let first = thread.sealed == 0;
        let part = put_part(ctx, anthropic::message(role, &frames, first, false))?;
        SEALED.put(ctx, &(thread.id, thread.sealed), &part);
        thread.sealed += 1;
    }
    thread.tail_from = last_frames[0].seq;
    let mut parts = vec![thread.header];
    let sealed = SEALED.scan(ctx, SEALED.prefix_of(&thread.id))?;
    parts.extend(sealed.into_iter().map(|(_, part)| part));
    let first = thread.sealed == 0;
    parts.push(put_part(
        ctx,
        anthropic::message(last_role, &last_frames, first, true),
    )?);
    parts.push(put_part(ctx, anthropic::CLOSE.into())?);
    Ok(parts)
}

pub(crate) async fn converse(ctx: &Ctx, id: u64) {
    if let Err(error) = drive(ctx, id).await {
        stall(ctx, id, &error);
    }
}

async fn drive(ctx: &Ctx, id: u64) -> Result<(), Error> {
    while let Some((upto, parts)) = claim(ctx, id)? {
        let head = abi::encode(&anthropic::head());
        let answer = request(ctx, MODEL_SERVICE, head, parts).await;
        let calls = turned(ctx, id, upto, answer)?;
        let runs = calls.into_iter().map(|call| run_call(ctx, id, call));
        for ran in all(runs).await {
            ran?;
        }
        settle(ctx, id)?;
    }
    Ok(())
}

fn may_ask(ctx: &ExecCtx, thread: &ThreadRow) -> bool {
    let idle = thread.asking == Asking::Idle;
    let within_turns = thread.turns_since_post < MAX_TURNS_PER_POST;
    let within_failures = thread.failures < MAX_ATTEMPTS;
    let room = thread.sealed < MAX_MESSAGES;
    let hired = AGENTS.has(ctx, &thread.agent);
    !thread.closed && hired && idle && thread.due() && within_turns && within_failures && room
}

fn claim(ctx: &ExecCtx, id: u64) -> Result<Option<(u64, Vec<BlobId>)>, Error> {
    let mut thread = thread(ctx, id)?;
    if !may_ask(ctx, &thread) {
        return Ok(None);
    }
    if !serves(ctx, MODEL_SERVICE)? {
        let text = format!("no member node serves {MODEL_SERVICE}: the model was not asked");
        append(ctx, &mut thread, Body::Notice { text });
        THREADS.put(ctx, &id, &thread);
        return Ok(None);
    }
    let parts = render(ctx, &mut thread)?;
    let upto = thread.frames;
    thread.asking = Asking::Asked { upto };
    THREADS.put(ctx, &id, &thread);
    Ok(Some((upto, parts)))
}

fn answered(answer: Result<Vec<u8>, Error>) -> Result<anthropic::Reply, String> {
    let bytes =
        answer.map_err(|error| format!("the model was not reached: {}", refusal(&error)))?;
    let response: http::Response = abi::decode(&bytes)
        .map_err(|_| "the provider's answer is not an HTTP response".to_owned())?;
    if response.status != 200 {
        return Err(anthropic::failure(response.status, &response.body));
    }
    anthropic::reply(&response.body)
}

fn turned(
    ctx: &ExecCtx,
    id: u64,
    upto: u64,
    answer: Result<Vec<u8>, Error>,
) -> Result<Vec<String>, Error> {
    let mut thread = thread(ctx, id)?;
    thread.asking = Asking::Running;
    let calls = match (thread.closed, answered(answer)) {
        (true, _) => Vec::new(),
        (false, Ok(reply)) => record(ctx, &mut thread, upto, reply)?,
        (false, Err(text)) => {
            thread.failures += 1;
            append(ctx, &mut thread, Body::Notice { text });
            Vec::new()
        }
    };
    THREADS.put(ctx, &id, &thread);
    Ok(calls)
}

fn settle(ctx: &ExecCtx, id: u64) -> Result<(), Error> {
    let mut thread = thread(ctx, id)?;
    release(ctx, &mut thread)?;
    thread.asking = Asking::Idle;
    THREADS.put(ctx, &id, &thread);
    Ok(())
}

fn stall(ctx: &ExecCtx, id: u64, error: &Error) {
    let Ok(Some(mut thread)) = THREADS.get(ctx, &id) else {
        return;
    };
    thread.asking = Asking::Idle;
    let text = format!("the agent stopped: {}", refusal(error));
    append(ctx, &mut thread, Body::Notice { text });
    THREADS.put(ctx, &id, &thread);
}

fn record(
    ctx: &ExecCtx,
    thread: &mut ThreadRow,
    upto: u64,
    reply: anthropic::Reply,
) -> Result<Vec<String>, Error> {
    thread.seen = upto;
    thread.failures = 0;
    thread.turns_since_post += 1;
    thread.last_stop = Some(reply.stop);
    let asked: Vec<(String, String, String)> = reply
        .said
        .iter()
        .filter_map(|said| match said {
            Said::ToolUse { id, name, input } => Some((id.clone(), name.clone(), input.clone())),
            Said::Text(_) | Said::Thinking { .. } | Said::RedactedThinking(_) => None,
        })
        .collect();
    let turn = Body::Turn {
        model: reply.model,
        said: reply.said,
        stop: reply.stop,
        usage: reply.usage,
    };
    append(ctx, thread, turn);
    let mut calls = Vec::new();
    for (call, tool, input) in asked {
        if let Some(placed) = place(ctx, thread, call, tool, input)? {
            calls.push(placed);
        }
    }
    if thread.turns_since_post == MAX_TURNS_PER_POST {
        let text = format!(
            "the model took {MAX_TURNS_PER_POST} turns since it was last addressed; address it again to let it go on"
        );
        append(ctx, thread, Body::Notice { text });
    }
    Ok(calls)
}

fn call_id_fits(call: &str) -> bool {
    let sized = !call.is_empty() && call.len() <= MAX_CALL_ID_BYTES;
    sized
        && call
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn planned(
    ctx: &ExecCtx,
    thread: &ThreadRow,
    call: &str,
    tool: &str,
    input: &str,
) -> Result<(), String> {
    if !call_id_fits(call) {
        return Err(format!(
            "a call id is 1 to {MAX_CALL_ID_BYTES} letters, digits, _ and -"
        ));
    }
    if CALLS.has(ctx, &(thread.id, call.to_owned())) {
        return Err(format!("call {call} was made before"));
    }
    if input.len() > MAX_INPUT_BYTES {
        return Err(format!("a call's input is at most {MAX_INPUT_BYTES} bytes"));
    }
    let entry = tools::find(tool).ok_or_else(|| format!("there is no tool named {tool}"))?;
    let input: Value =
        serde_json::from_str(input).map_err(|error| format!("the input is not JSON: {error}"))?;
    (entry.check)(&input)?;
    let relaying = serves(ctx, RELAY).map_err(|error| error.message)?;
    if !relaying {
        return Err(format!(
            "no member node serves {RELAY}, so no tool call can run"
        ));
    }
    Ok(())
}

fn place(
    ctx: &ExecCtx,
    thread: &mut ThreadRow,
    call: String,
    tool: String,
    input: String,
) -> Result<Option<String>, Error> {
    if let Err(problem) = planned(ctx, thread, &call, &tool, &input) {
        append(
            ctx,
            thread,
            Body::Refused {
                call,
                tool,
                problem,
            },
        );
        return Ok(None);
    }
    let row = CallRow {
        thread: thread.id,
        id: call.clone(),
        tool,
        input,
        made: ctx.env().height,
        state: CallState::Relaying { attempts: 0 },
    };
    CALLS.put(ctx, &(thread.id, call.clone()), &row);
    thread.open_calls += 1;
    Ok(Some(call))
}

fn mark(ctx: &ExecCtx, id: u64, call: &str, state: CallState) -> Result<(), Error> {
    let key = (id, call.to_owned());
    if let Some(mut row) = CALLS.get(ctx, &key)? {
        row.state = state;
        CALLS.put(ctx, &key, &row);
    }
    Ok(())
}

async fn run_call(ctx: &Ctx, id: u64, call: String) -> Result<(), Error> {
    let (text, failed) = match relay(ctx, id, &call).await? {
        true => perform(ctx, id, &call).await?,
        false => ("no provider relayed the call back".to_owned(), true),
    };
    finish(ctx, id, &call, text, failed)
}

async fn relay(ctx: &Ctx, id: u64, call: &str) -> Result<bool, Error> {
    for attempts in 1..=MAX_ATTEMPTS {
        mark(ctx, id, call, CallState::Relaying { attempts })?;
        let relayed = request(ctx, RELAY, call.as_bytes().to_vec(), Vec::new()).await;
        if relayed.is_ok() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn refusal(error: &Error) -> String {
    format!("{}: {}", error.code, error.message)
}

async fn perform(ctx: &Ctx, id: u64, call: &str) -> Result<(String, bool), Error> {
    let thread = thread(ctx, id)?;
    let Some(row) = CALLS.get(ctx, &(id, call.to_owned()))? else {
        return Ok((format!("call {call} is not open"), true));
    };
    let hired = AGENTS.has(ctx, &thread.agent);
    let entry = match (thread.closed, hired, tools::find(&row.tool)) {
        (true, _, _) => return Ok(("the thread closed before the call ran".to_owned(), true)),
        (false, false, _) => {
            return Ok((
                "the agent was dismissed before the call ran".to_owned(),
                true,
            ));
        }
        (false, true, None) => return Ok((format!("there is no tool named {}", row.tool), true)),
        (false, true, Some(entry)) => entry,
    };
    let input: Value = serde_json::from_str(&row.input).unwrap_or(Value::Null);
    let seat = Seat {
        thread: id,
        agent: thread.agent,
        call,
    };
    match (entry.run)(ctx, &seat, input) {
        Ok(Run::Done(value)) => Ok((value.to_string(), false)),
        Ok(Run::Write { target, payload }) => {
            mark(ctx, id, call, CallState::Writing)?;
            let written = ctx.call_raw_as(thread.agent, target, payload).await;
            Ok(match written {
                Ok(output) => ((entry.render)(&output).to_string(), false),
                Err(error) => (refusal(&error), true),
            })
        }
        Err(error) => Ok((refusal(&error), true)),
    }
}

fn clipped(mut text: String) -> String {
    if text.len() <= MAX_RESULT_BYTES {
        return text;
    }
    let mut end = MAX_RESULT_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text.push_str(" …(cut: ask for less, or page with a cursor)");
    text
}

fn finish(ctx: &ExecCtx, id: u64, call: &str, text: String, failed: bool) -> Result<(), Error> {
    let mut thread = thread(ctx, id)?;
    let key = (id, call.to_owned());
    let Some(mut row) = CALLS.get(ctx, &key)? else {
        return Ok(());
    };
    let result = Body::Result {
        call: row.id.clone(),
        tool: row.tool.clone(),
        text: clipped(text),
        failed,
    };
    append(ctx, &mut thread, result);
    row.state = CallState::Done {
        height: ctx.env().height,
        failed,
    };
    thread.open_calls = thread.open_calls.saturating_sub(1);
    CALLS.put(ctx, &key, &row);
    THREADS.put(ctx, &id, &thread);
    Ok(())
}
