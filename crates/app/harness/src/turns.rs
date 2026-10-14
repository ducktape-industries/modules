//! The loop consensus runs: ask the model when a thread has input it has not seen, hold
//! input while it answers, record its turn, receipt and relay each tool call, then run it.
use ::program::{
    BlobId, Error, ExecCtx, HashKind, Outcome as Written, QueryCtx, Range, Reply as Wanted,
    unexpected_reply,
};
use provider::{Delivery, Outcome, RELAY, http};
use serde_json::Value;

use crate::agents::thread;
use crate::state::{AGENTS, CALLS, FRAMES, HELD, SEALED, THREADS, WRITES};
use crate::tools::{self, Run, Seat};
use crate::{
    Asking, Body, CallRow, CallState, Frame, MAX_ATTEMPTS, MAX_CALL_ID_BYTES, MAX_INPUT_BYTES,
    MAX_MESSAGES, MAX_RESULT_BYTES, MAX_TURNS_PER_POST, MODEL_SERVICE, PART, Role, Said, ThreadRow,
    anthropic, relayed_as, turned_as,
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
    let model_answering = thread.asking != Asking::Idle;
    if !model_answering {
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

fn request(ctx: &ExecCtx, service: &str, input: Vec<u8>, parts: Vec<BlobId>, deliver: Vec<u8>) {
    let request = provider::Request {
        service: service.into(),
        input,
        parts,
        deliver,
    };
    ctx.emit(
        provider::PROGRAM,
        &provider::Op::Request(request),
        Wanted::None,
    );
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

pub(crate) fn advance(ctx: &ExecCtx, thread: &mut ThreadRow) -> Result<(), Error> {
    let idle = thread.asking == Asking::Idle;
    let within_turns = thread.turns_since_post < MAX_TURNS_PER_POST;
    let within_failures = thread.failures < MAX_ATTEMPTS;
    let room = thread.sealed < MAX_MESSAGES;
    let hired = AGENTS.has(ctx, &thread.agent);
    let ask =
        !thread.closed && hired && idle && thread.due() && within_turns && within_failures && room;
    if !ask {
        return Ok(());
    }
    if !serves(ctx, MODEL_SERVICE)? {
        let text = format!("no member node serves {MODEL_SERVICE}: the model was not asked");
        append(ctx, thread, Body::Notice { text });
        return Ok(());
    }
    let parts = render(ctx, thread)?;
    let head = abi::encode(&anthropic::head());
    request(ctx, MODEL_SERVICE, head, parts, turned_as(thread.id));
    thread.asking = Asking::Asked {
        upto: thread.frames,
    };
    Ok(())
}

fn answered(bytes: &[u8]) -> Result<anthropic::Reply, String> {
    let response: http::Response = abi::decode(bytes)
        .map_err(|_| "the provider's answer is not an HTTP response".to_owned())?;
    if response.status != 200 {
        return Err(anthropic::failure(response.status, &response.body));
    }
    anthropic::reply(&response.body)
}

pub(crate) fn turned(ctx: &ExecCtx, id: u64, delivery: Delivery) -> Result<(), Error> {
    ctx.env().sent_by(provider::PROGRAM)?;
    let mut thread = thread(ctx, id)?;
    let Asking::Asked { upto } = thread.asking else {
        return Ok(());
    };
    thread.asking = Asking::Idle;
    let answer = match delivery.outcome {
        Outcome::Answered(bytes) => answered(&bytes),
        Outcome::Failed(reason) => Err(format!("the provider could not reach the model: {reason}")),
        Outcome::Expired => Err("no provider answered the model call in time".to_owned()),
    };
    match (thread.closed, answer) {
        (true, _) => {}
        (false, Ok(reply)) => record(ctx, &mut thread, upto, reply)?,
        (false, Err(text)) => {
            thread.failures += 1;
            append(ctx, &mut thread, Body::Notice { text });
        }
    }
    release(ctx, &mut thread)?;
    advance(ctx, &mut thread)?;
    THREADS.put(ctx, &id, &thread);
    Ok(())
}

fn record(
    ctx: &ExecCtx,
    thread: &mut ThreadRow,
    upto: u64,
    reply: anthropic::Reply,
) -> Result<(), Error> {
    thread.seen = upto;
    thread.failures = 0;
    thread.turns_since_post += 1;
    thread.last_stop = Some(reply.stop);
    let calls: Vec<(String, String, String)> = reply
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
    for (call, tool, input) in calls {
        place(ctx, thread, call, tool, input)?;
    }
    if thread.turns_since_post == MAX_TURNS_PER_POST {
        let text = format!(
            "the model took {MAX_TURNS_PER_POST} turns since it was last addressed; address it again to let it go on"
        );
        append(ctx, thread, Body::Notice { text });
    }
    Ok(())
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

fn relay(ctx: &ExecCtx, thread: u64, call: &str) {
    request(
        ctx,
        RELAY,
        call.as_bytes().to_vec(),
        Vec::new(),
        relayed_as(thread, call),
    );
}

fn place(
    ctx: &ExecCtx,
    thread: &mut ThreadRow,
    call: String,
    tool: String,
    input: String,
) -> Result<(), Error> {
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
        return Ok(());
    }
    let row = CallRow {
        thread: thread.id,
        id: call.clone(),
        tool: tool.clone(),
        input,
        made: ctx.env().height,
        state: CallState::Relaying { attempts: 1 },
    };
    CALLS.put(ctx, &(thread.id, call.clone()), &row);
    thread.open_calls += 1;
    append(
        ctx,
        thread,
        Body::Receipt {
            call: call.clone(),
            tool,
        },
    );
    relay(ctx, thread.id, &call);
    Ok(())
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

fn finish(ctx: &ExecCtx, thread: &mut ThreadRow, call: &mut CallRow, text: String, failed: bool) {
    let result = Body::Result {
        call: call.id.clone(),
        tool: call.tool.clone(),
        text: clipped(text),
        failed,
    };
    say(ctx, thread, result);
    call.state = CallState::Done {
        height: ctx.env().height,
        failed,
    };
    thread.open_calls = thread.open_calls.saturating_sub(1);
}

fn refusal(error: &Error) -> String {
    format!("{}: {}", error.code, error.message)
}

fn execute(ctx: &ExecCtx, thread: &mut ThreadRow, call: &mut CallRow) {
    if thread.closed {
        let text = "the thread closed before the call ran".to_owned();
        finish(ctx, thread, call, text, true);
        return;
    }
    let Some(entry) = tools::find(&call.tool) else {
        let text = format!("there is no tool named {}", call.tool);
        finish(ctx, thread, call, text, true);
        return;
    };
    let input: Value = serde_json::from_str(&call.input).unwrap_or(Value::Null);
    let seat = Seat {
        thread: thread.id,
        agent: thread.agent,
        call: &call.id,
    };
    match (entry.run)(ctx, &seat, input) {
        Ok(Run::Done(value)) => finish(ctx, thread, call, value.to_string(), false),
        Ok(Run::Emit { target, payload }) => {
            let message = ctx.emit_raw_as(thread.agent, target, payload, Wanted::Wanted);
            WRITES.put(ctx, &message.seq, &(thread.id, call.id.clone()));
            call.state = CallState::Writing;
        }
        Err(error) => finish(ctx, thread, call, refusal(&error), true),
    }
}

pub(crate) fn relayed(
    ctx: &ExecCtx,
    id: u64,
    call_id: String,
    delivery: Delivery,
) -> Result<(), Error> {
    ctx.env().sent_by(provider::PROGRAM)?;
    let mut thread = thread(ctx, id)?;
    let key = (id, call_id);
    let Some(mut call) = CALLS.get(ctx, &key)? else {
        return Ok(());
    };
    let CallState::Relaying { attempts } = call.state else {
        return Ok(());
    };
    let came_back = matches!(delivery.outcome, Outcome::Answered(_));
    let retry = attempts < MAX_ATTEMPTS;
    match (came_back, retry) {
        (true, _) => execute(ctx, &mut thread, &mut call),
        (false, true) => {
            call.state = CallState::Relaying {
                attempts: attempts + 1,
            };
            relay(ctx, id, &key.1);
        }
        (false, false) => {
            let text = "no provider relayed the call back".to_owned();
            finish(ctx, &mut thread, &mut call, text, true);
        }
    }
    CALLS.put(ctx, &key, &call);
    advance(ctx, &mut thread)?;
    THREADS.put(ctx, &id, &thread);
    Ok(())
}

pub(crate) fn written(ctx: &ExecCtx, seq: u64, outcome: &Written) -> Result<(), Error> {
    let Some((id, call_id)) = WRITES.get(ctx, &seq)? else {
        return Ok(());
    };
    WRITES.remove(ctx, &seq);
    let mut thread = thread(ctx, id)?;
    let key = (id, call_id);
    let Some(mut call) = CALLS.get(ctx, &key)? else {
        return Ok(());
    };
    let (text, failed) = match (outcome, tools::find(&call.tool)) {
        (Written::Applied { output }, Some(entry)) => ((entry.render)(output).to_string(), false),
        (Written::Applied { .. }, None) => ("applied".to_owned(), false),
        (Written::Rejected(error), _) => (refusal(error), true),
    };
    finish(ctx, &mut thread, &mut call, text, failed);
    CALLS.put(ctx, &key, &call);
    advance(ctx, &mut thread)?;
    THREADS.put(ctx, &id, &thread);
    Ok(())
}
