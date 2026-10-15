//! The loop consensus runs for a thread: learn the model's context window, summarize the thread
//! when its next answer would not fit, ask the model, run each tool call a block later, ask again.
use ::program::{
    BlobId, Ctx, Error, ExecCtx, HashKind, QueryCtx, Range, all, unexpected_reply, wrong_state,
};
use provider::{RELAY, http};
use serde_json::Value;

use crate::agents::thread;
use crate::anthropic::{self, Answered, Summarized, Summary};
use crate::state::{AGENTS, CALLS, FRAMES, HELD, SEALED, THREADS, WINDOWS, WITHHELD};
use crate::tools::{self, Run, Seat};
use crate::{
    Asking, Body, CallRow, CallState, Context, Frame, MODEL_SERVICE, PART, Role, Said, Sealed,
    ThreadRow,
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

fn settle(ctx: &ExecCtx, thread: &mut ThreadRow) -> Result<(), Error> {
    for index in 0..thread.held {
        let key = (thread.id, index);
        if let Some(body) = HELD.get(ctx, &key)? {
            append(ctx, thread, body);
        }
        HELD.remove(ctx, &key);
    }
    thread.held = 0;
    thread.asking = Asking::Idle;
    Ok(())
}

fn give_up(ctx: &ExecCtx, thread: &mut ThreadRow, text: String) {
    append(ctx, thread, Body::Notice { text });
    thread.seen = thread.seen.max(thread.last_input);
    thread.last_stop = None;
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

async fn call_model(
    ctx: &Ctx,
    head: http::Head,
    parts: Vec<BlobId>,
) -> Result<http::Response, String> {
    let answer = request(ctx, MODEL_SERVICE, abi::encode(&head), parts).await;
    let bytes =
        answer.map_err(|error| format!("the model was not reached: {}", refusal(&error)))?;
    abi::decode(&bytes).map_err(|_| "the provider's answer is not an HTTP response".to_owned())
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

fn summary_part(ctx: &ExecCtx, thread: &ThreadRow) -> Result<Option<BlobId>, Error> {
    let Some(seq) = thread.summary else {
        return Ok(None);
    };
    let Some(Frame {
        body: Body::Compaction { block },
        ..
    }) = FRAMES.get(ctx, &(thread.id, seq))?
    else {
        let key = FRAMES.key(&(thread.id, seq));
        return Err(::program::corrupt(
            "frame",
            &key,
            "is not the thread's summary",
        ));
    };
    Ok(Some(put_part(ctx, anthropic::summary_message(&block))?))
}

fn opens(thread: &ThreadRow) -> bool {
    thread.sealed == 0 && thread.summary.is_none()
}

fn withheld(body: Body) -> Body {
    let left_out = |bytes: usize| {
        format!("left out: these {bytes} bytes alone do not fit the model's context window")
    };
    match body {
        Body::Result {
            call, tool, text, ..
        } => Body::Result {
            call,
            tool,
            text: format!("{}; call again for less of it", left_out(text.len())),
            failed: true,
        },
        Body::Refused {
            call,
            tool,
            problem,
        } => Body::Refused {
            call,
            tool,
            problem: left_out(problem.len()),
        },
        Body::Post { author, text } => Body::Post {
            author,
            text: format!("({})", left_out(text.len())),
        },
        Body::Turn { .. } | Body::Notice { .. } | Body::Compaction { .. } => body,
    }
}

fn shown(ctx: &ExecCtx, thread: u64, frame: Frame) -> Frame {
    match WITHHELD.has(ctx, &(thread, frame.seq)) {
        true => Frame {
            body: withheld(frame.body),
            ..frame
        },
        false => frame,
    }
}

fn tail(ctx: &ExecCtx, thread: &ThreadRow) -> Result<Vec<Frame>, Error> {
    let whole = FRAMES.prefix_of(&thread.id);
    let range = Range::new(FRAMES.key(&(thread.id, thread.tail_from)), whole.end);
    let frames = FRAMES.scan(ctx, range)?.into_iter();
    Ok(frames
        .map(|(_, frame)| shown(ctx, thread.id, frame))
        .collect())
}

fn seal(ctx: &ExecCtx, thread: &mut ThreadRow) -> Result<Option<(Role, Vec<Frame>)>, Error> {
    let mut groups = grouped(tail(ctx, thread)?);
    let Some(last) = groups.pop() else {
        return Ok(None);
    };
    for (role, frames) in groups {
        let end = frames[frames.len() - 1].seq;
        let part = put_part(ctx, anthropic::message(role, &frames, opens(thread), false))?;
        SEALED.put(
            ctx,
            &(thread.id, thread.sealed),
            &Sealed { end, role, part },
        );
        thread.sealed += 1;
    }
    thread.tail_from = last.1[0].seq;
    Ok(Some(last))
}

fn opening(ctx: &ExecCtx, thread: &ThreadRow) -> Result<Vec<BlobId>, Error> {
    let mut parts = vec![thread.header];
    parts.extend(summary_part(ctx, thread)?);
    Ok(parts)
}

fn sealed_parts(ctx: &ExecCtx, thread: u64) -> Result<Vec<Sealed>, Error> {
    let sealed = SEALED.scan(ctx, SEALED.prefix_of(&thread))?;
    Ok(sealed.into_iter().map(|(_, sealed)| sealed).collect())
}

fn turn_parts(ctx: &ExecCtx, thread: &mut ThreadRow) -> Result<Vec<BlobId>, Error> {
    let Some((role, frames)) = seal(ctx, thread)? else {
        return Err(wrong_state("a thread with nothing said asks no model"));
    };
    let mut parts = opening(ctx, thread)?;
    parts.extend(
        sealed_parts(ctx, thread.id)?
            .iter()
            .map(|sealed| sealed.part),
    );
    let last = anthropic::message(role, &frames, opens(thread), true);
    parts.push(put_part(ctx, last)?);
    parts.push(put_part(ctx, anthropic::CLOSE.into())?);
    Ok(parts)
}

fn summarize_parts(
    ctx: &ExecCtx,
    thread: &mut ThreadRow,
) -> Result<Option<(u64, Vec<BlobId>)>, Error> {
    seal(ctx, thread)?;
    let answered = |sealed: &Sealed| {
        let seen = sealed.end <= thread.seen;
        let allowed = thread
            .compact_before
            .is_none_or(|before| sealed.end < before);
        seen && allowed
    };
    let mut covered: Vec<Sealed> = sealed_parts(ctx, thread.id)?
        .into_iter()
        .take_while(answered)
        .collect();
    let cut = covered.iter().rposition(|sealed| sealed.role == Role::User);
    covered.truncate(cut.map_or(0, |index| index + 1));
    let Some(through) = covered.last().map(|sealed| sealed.end) else {
        return Ok(None);
    };
    let mut parts = opening(ctx, thread)?;
    parts.extend(covered.iter().map(|sealed| sealed.part));
    parts.push(put_part(ctx, anthropic::SUMMARIZE.into())?);
    Ok(Some((through, parts)))
}

fn forget_sealed(ctx: &ExecCtx, thread: &mut ThreadRow, through: u64) -> Result<(), Error> {
    for (key, _) in SEALED.scan(ctx, SEALED.prefix_of(&thread.id))? {
        SEALED.remove(ctx, &key);
    }
    thread.sealed = 0;
    thread.tail_from = through + 1;
    thread.compact_before = None;
    Ok(())
}

enum Step {
    Learn {
        model: String,
    },
    Compact {
        through: u64,
        parts: Vec<BlobId>,
    },
    Ask {
        upto: u64,
        summarized: bool,
        parts: Vec<BlobId>,
    },
}

pub(crate) async fn converse(ctx: &Ctx, id: u64) {
    if let Err(error) = drive(ctx, id).await {
        stall(ctx, id, &error);
    }
}

async fn drive(ctx: &Ctx, id: u64) -> Result<(), Error> {
    while let Some(step) = claim(ctx, id)? {
        match step {
            Step::Learn { model } => learn(ctx, id, model).await?,
            Step::Compact { through, parts } => compact(ctx, id, through, parts).await?,
            Step::Ask {
                upto,
                summarized,
                parts,
            } => ask(ctx, id, upto, summarized, parts).await?,
        }
    }
    Ok(())
}

fn may_ask(ctx: &ExecCtx, thread: &ThreadRow) -> bool {
    let idle = thread.asking == Asking::Idle;
    let hired = AGENTS.has(ctx, &thread.agent);
    !thread.closed && hired && idle && thread.due()
}

fn claim(ctx: &ExecCtx, id: u64) -> Result<Option<Step>, Error> {
    let mut thread = thread(ctx, id)?;
    if !may_ask(ctx, &thread) {
        return Ok(None);
    }
    let step = plan(ctx, &mut thread)?;
    THREADS.put(ctx, &id, &thread);
    Ok(step)
}

fn plan(ctx: &ExecCtx, thread: &mut ThreadRow) -> Result<Option<Step>, Error> {
    if !serves(ctx, MODEL_SERVICE)? {
        let text = format!("no member node serves {MODEL_SERVICE}: the model was not asked");
        give_up(ctx, thread, text);
        return Ok(None);
    }
    let Some(window) = WINDOWS.get(ctx, &thread.model)? else {
        thread.asking = Asking::Learning;
        let model = thread.model.clone();
        return Ok(Some(Step::Learn { model }));
    };
    match crowded(thread, window) {
        true => plan_summary(ctx, thread),
        false => plan_turn(ctx, thread),
    }
}

fn plan_turn(ctx: &ExecCtx, thread: &mut ThreadRow) -> Result<Option<Step>, Error> {
    let parts = turn_parts(ctx, thread)?;
    let upto = thread.frames;
    thread.asking = Asking::Asked { upto };
    let summarized = thread.summary.is_some();
    Ok(Some(Step::Ask {
        upto,
        summarized,
        parts,
    }))
}

fn plan_summary(ctx: &ExecCtx, thread: &mut ThreadRow) -> Result<Option<Step>, Error> {
    let Some((through, parts)) = summarize_parts(ctx, thread)? else {
        return match thread.context {
            Context::Measured(_) => plan_turn(ctx, thread),
            Context::Overflowed => plan_without_largest(ctx, thread),
        };
    };
    thread.asking = Asking::Compacting { through };
    Ok(Some(Step::Compact { through, parts }))
}

fn plan_without_largest(ctx: &ExecCtx, thread: &mut ThreadRow) -> Result<Option<Step>, Error> {
    let Some(frame) = largest_unseen(ctx, thread)? else {
        let text = format!(
            "the conversation does not fit {}'s context window, even summarized and with everything new left out",
            thread.model
        );
        give_up(ctx, thread, text);
        thread.context = Context::Measured(0);
        thread.compact_before = None;
        return Ok(None);
    };
    WITHHELD.insert(ctx, &(thread.id, frame.seq));
    let what = match &frame.body {
        Body::Result { call, tool, .. } | Body::Refused { call, tool, .. } => {
            format!("the result of {tool} call {call}")
        }
        Body::Post { .. } | Body::Turn { .. } | Body::Notice { .. } | Body::Compaction { .. } => {
            format!("message {}", frame.seq)
        }
    };
    let text = format!(
        "{what} alone does not fit {}'s context window, so the model is told it was left out",
        thread.model
    );
    append(ctx, thread, Body::Notice { text });
    plan_turn(ctx, thread)
}

fn largest_unseen(ctx: &ExecCtx, thread: &ThreadRow) -> Result<Option<Frame>, Error> {
    let unseen = tail(ctx, thread)?.into_iter().filter(|frame| {
        let new = frame.seq > thread.seen;
        let input = frame.body.role() == Some(Role::User);
        let kept = !WITHHELD.has(ctx, &(thread.id, frame.seq));
        new && input && kept
    });
    let sized = unseen.map(|frame| {
        (
            anthropic::message(Role::User, std::slice::from_ref(&frame), true, false).len(),
            frame,
        )
    });
    Ok(sized.max_by_key(|(size, _)| *size).map(|(_, frame)| frame))
}

fn crowded(thread: &ThreadRow, window: Option<u64>) -> bool {
    match (thread.context, window) {
        (Context::Overflowed, _) => true,
        (Context::Measured(_), None) => false,
        (Context::Measured(tokens), Some(window)) => {
            tokens.saturating_add(u64::from(thread.max_tokens)) > window
        }
    }
}

async fn learn(ctx: &Ctx, id: u64, model: String) -> Result<(), Error> {
    let answer = call_model(ctx, anthropic::model(&model), Vec::new()).await;
    let mut thread = thread(ctx, id)?;
    match answer.and_then(|response| anthropic::window(&response)) {
        Ok(window) => WINDOWS.put(ctx, &model, &window),
        Err(text) => give_up(ctx, &mut thread, text),
    }
    settle(ctx, &mut thread)?;
    THREADS.put(ctx, &id, &thread);
    Ok(())
}

async fn compact(ctx: &Ctx, id: u64, through: u64, parts: Vec<BlobId>) -> Result<(), Error> {
    let answer = call_model(ctx, anthropic::head(true), parts).await;
    let mut thread = thread(ctx, id)?;
    match answer.map(|response| anthropic::summarized(&response)) {
        Ok(Summarized::Summary(summary)) => summarize(ctx, &mut thread, through, summary)?,
        Ok(Summarized::Overflow) => thread.compact_before = Some(through),
        Ok(Summarized::Failed(text)) | Err(text) => give_up(ctx, &mut thread, text),
    }
    settle(ctx, &mut thread)?;
    THREADS.put(ctx, &id, &thread);
    Ok(())
}

fn summarize(
    ctx: &ExecCtx,
    thread: &mut ThreadRow,
    through: u64,
    summary: Summary,
) -> Result<(), Error> {
    append(
        ctx,
        thread,
        Body::Compaction {
            block: summary.block,
        },
    );
    thread.summary = Some(thread.frames);
    thread.context = Context::Measured(summary.tokens);
    forget_sealed(ctx, thread, through)
}

async fn ask(
    ctx: &Ctx,
    id: u64,
    upto: u64,
    summarized: bool,
    parts: Vec<BlobId>,
) -> Result<(), Error> {
    let answer = call_model(ctx, anthropic::head(summarized), parts).await;
    let calls = turned(ctx, id, upto, answer)?;
    let runs = calls.into_iter().map(|call| run_call(ctx, id, call));
    for ran in all(runs).await {
        ran?;
    }
    let mut thread = thread(ctx, id)?;
    settle(ctx, &mut thread)?;
    THREADS.put(ctx, &id, &thread);
    Ok(())
}

fn turned(
    ctx: &ExecCtx,
    id: u64,
    upto: u64,
    answer: Result<http::Response, String>,
) -> Result<Vec<String>, Error> {
    let mut thread = thread(ctx, id)?;
    thread.asking = Asking::Running;
    let answered = answer.map(|response| anthropic::answered(&response));
    let calls = match (thread.closed, answered) {
        (true, _) => Vec::new(),
        (false, Ok(Answered::Reply(reply))) => record(ctx, &mut thread, upto, reply)?,
        (false, Ok(Answered::Overflow)) => {
            thread.context = Context::Overflowed;
            Vec::new()
        }
        (false, Ok(Answered::Failed(text)) | Err(text)) => {
            give_up(ctx, &mut thread, text);
            Vec::new()
        }
    };
    THREADS.put(ctx, &id, &thread);
    Ok(calls)
}

fn stall(ctx: &ExecCtx, id: u64, error: &Error) {
    let Ok(Some(mut thread)) = THREADS.get(ctx, &id) else {
        return;
    };
    give_up(
        ctx,
        &mut thread,
        format!("the agent stopped: {}", refusal(error)),
    );
    if settle(ctx, &mut thread).is_ok() {
        THREADS.put(ctx, &id, &thread);
    }
}

fn record(
    ctx: &ExecCtx,
    thread: &mut ThreadRow,
    upto: u64,
    reply: anthropic::Reply,
) -> Result<Vec<String>, Error> {
    thread.seen = upto;
    thread.last_stop = Some(reply.stop);
    let usage = reply.usage;
    let tokens = usage.input + usage.cache_read + usage.cache_write + usage.output;
    thread.context = Context::Measured(tokens);
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
        usage,
    };
    append(ctx, thread, turn);
    let mut calls = Vec::new();
    for (call, tool, input) in asked {
        if let Some(placed) = place(ctx, thread, call, tool, input)? {
            calls.push(placed);
        }
    }
    Ok(calls)
}

fn call_id_fits(call: &str) -> bool {
    !call.is_empty()
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
        return Err("a call id is letters, digits, _ and -".to_owned());
    }
    if CALLS.has(ctx, &(thread.id, call.to_owned())) {
        return Err(format!("call {call} was made before"));
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
        state: CallState::Relaying,
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
    let relayed = request(ctx, RELAY, call.as_bytes().to_vec(), Vec::new()).await;
    let (text, failed) = match relayed {
        Ok(_) => perform(ctx, id, &call).await?,
        Err(error) => (format!("the relay failed: {}", refusal(&error)), true),
    };
    finish(ctx, id, &call, text, failed)
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

fn finish(ctx: &ExecCtx, id: u64, call: &str, text: String, failed: bool) -> Result<(), Error> {
    let mut thread = thread(ctx, id)?;
    let key = (id, call.to_owned());
    let Some(mut row) = CALLS.get(ctx, &key)? else {
        return Ok(());
    };
    let result = Body::Result {
        call: row.id.clone(),
        tool: row.tool.clone(),
        text,
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
