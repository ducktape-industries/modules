//! What harness tells chat as the agent, in its own execution: 👀 on a chat input it takes up,
//! ✅ in place of 👀 on every input a turn answered, and one line in the input's thread saying
//! why when it gives up. A refusal from chat is a Notice in the thread, and the input goes on.
use ::program::{Ctx, Error, ExecCtx, Range, corrupt};
use abi::role::identity::AccountNumber;

use crate::agents::thread;
use crate::state::{FRAMES, THREADS};
use crate::turns::{refusal, say};
use crate::{Body, ChatMessage, Frame, PROGRAM, Stop, ThreadRow};

const TAKEN_UP: &str = "👀";
const ANSWERED: &str = "✅";

async fn write(ctx: &Ctx, agent: AccountNumber, op: &chat::Op) -> Result<(), Error> {
    ctx.call_raw_as(agent, chat::PROGRAM, abi::encode(op))
        .await
        .map(drop)
}

fn react(origin: &ChatMessage, emoji: &str, on: bool) -> chat::Op {
    let (channel_id, seq, emoji) = (origin.channel.clone(), origin.seq, emoji.to_owned());
    match on {
        true => chat::Op::AddReaction {
            channel_id,
            seq,
            emoji,
        },
        false => chat::Op::RemoveReaction {
            channel_id,
            seq,
            emoji,
        },
    }
}

fn refused(ctx: &ExecCtx, id: u64, what: String, error: &Error) -> Result<(), Error> {
    let mut thread = thread(ctx, id)?;
    let text = format!("chat refused {what}: {}", refusal(error));
    say(ctx, &mut thread, Body::Notice { text });
    THREADS.put(ctx, &id, &thread);
    Ok(())
}

pub(crate) async fn taken_up(ctx: &Ctx, id: u64, origin: &ChatMessage) -> Result<(), Error> {
    let agent = thread(ctx, id)?.agent;
    match write(ctx, agent, &react(origin, TAKEN_UP, true)).await {
        Ok(()) => Ok(()),
        Err(error) => {
            let what = format!("{TAKEN_UP} on message {} in {}", origin.seq, origin.channel);
            refused(ctx, id, what, &error)
        }
    }
}

fn inputs(ctx: &ExecCtx, thread: &ThreadRow) -> Result<Vec<ChatMessage>, Error> {
    let from = FRAMES.key(&(thread.id, thread.signaled + 1));
    let to = FRAMES.key(&(thread.id, thread.seen + 1));
    let frames = FRAMES.scan(ctx, Range::new(from, Some(to)))?;
    Ok(frames
        .into_iter()
        .filter_map(|(_, frame)| match frame.body {
            Body::Post { origin, .. } => origin,
            _ => None,
        })
        .collect())
}

fn answered(stop: Option<Stop>) -> bool {
    stop.is_some_and(|stop| !matches!(stop, Stop::ToolUse | Stop::Pause))
}

pub(crate) async fn tell(ctx: &Ctx, id: u64) -> Result<(), Error> {
    let mut thread = thread(ctx, id)?;
    let gave_up = thread.gave_up.take();
    let unsignaled = thread.seen > thread.signaled;
    if gave_up.is_none() && !(unsignaled && answered(thread.last_stop)) {
        return Ok(());
    }
    let inputs = inputs(ctx, &thread)?;
    thread.signaled = thread.seen;
    THREADS.put(ctx, &id, &thread);
    match (gave_up, inputs.last()) {
        (Some(notice), Some(origin)) => why(ctx, &thread, notice, origin).await,
        (Some(_), None) => Ok(()),
        (None, _) => {
            for origin in &inputs {
                swap(ctx, &thread, origin).await?;
            }
            Ok(())
        }
    }
}

async fn swap(ctx: &Ctx, thread: &ThreadRow, origin: &ChatMessage) -> Result<(), Error> {
    for (emoji, on) in [(TAKEN_UP, false), (ANSWERED, true)] {
        if let Err(error) = write(ctx, thread.agent, &react(origin, emoji, on)).await {
            let what = format!("{ANSWERED} on message {} in {}", origin.seq, origin.channel);
            return refused(ctx, thread.id, what, &error);
        }
    }
    Ok(())
}

async fn why(
    ctx: &Ctx,
    thread: &ThreadRow,
    notice: u64,
    origin: &ChatMessage,
) -> Result<(), Error> {
    let key = (thread.id, notice);
    let Some(Frame {
        body: Body::Notice { text },
        ..
    }) = FRAMES.get(ctx, &key)?
    else {
        return Err(corrupt(
            "frame",
            &FRAMES.key(&key),
            "is not the notice the thread gave up with",
        ));
    };
    let line = chat::Op::PostMessage {
        channel_id: origin.channel.clone(),
        message_id: chat::namespace::id(PROGRAM, &format!("{}:{notice}", thread.id)),
        blocks: vec![chat::Block::paragraph(format!(
            "I could not answer: {text}."
        ))],
        thread: Some(origin.reply_to()),
    };
    match write(ctx, thread.agent, &line).await {
        Ok(()) => Ok(()),
        Err(error) => {
            let what = format!("the line saying why in {}", origin.channel);
            refused(ctx, thread.id, what, &error)
        }
    }
}
