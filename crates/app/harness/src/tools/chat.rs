//! Tools over `chat`: channels, messages, threads and search read as the agent reads them,
//! and posts and reactions the agent makes, which chat allows where it allows the agent.
use chat::{ChannelInfo, MsgRow, Query, Reply};
use guest::{Error, ExecCtx, Principal};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Run, Seat, Tool, cursor_schema, next, object, page, principal, unexpected};

fn ask(ctx: &ExecCtx, query: Query) -> Result<Reply, Error> {
    ctx.query(chat::MODULE, &query)
}

fn agent(seat: &Seat) -> Principal {
    Principal::Account(seat.agent)
}

fn channel(info: &ChannelInfo) -> Value {
    json!({
        "id": info.channel.id,
        "name": info.channel.name,
        "members_only": info.channel.members_only(),
        "archived": info.channel.archived,
        "newest_seq": info.head_seq,
    })
}

fn message(row: &MsgRow) -> Value {
    let reactions: Vec<Value> = row
        .reactions
        .iter()
        .map(|reaction| json!({ "emoji": reaction.emoji, "count": reaction.count }))
        .collect();
    json!({
        "seq": row.seq,
        "author": principal(&row.author),
        "time": row.time,
        "text": row.text,
        "reply_to": row.thread,
        "replies": row.reply_count,
        "reactions": reactions,
        "edited": row.edited,
        "deleted": row.deleted,
    })
}

fn messages(rows: &chat::PageResponse<MsgRow>) -> Value {
    json!({
        "messages": rows.items.iter().map(message).collect::<Vec<_>>(),
        "next": next(&rows.next),
    })
}

fn channel_schema() -> Value {
    json!({ "type": "string", "description": "a channel id, as chat_channels lists them" })
}

#[derive(Deserialize)]
pub(crate) struct ChannelsArgs {
    cursor: Option<String>,
}

pub(crate) struct Channels;

impl Tool for Channels {
    const NAME: &'static str = "chat_channels";
    const DESCRIPTION: &'static str = "List the network's chat channels: id, name, whether only members post, and the newest message's seq.";
    type Args = ChannelsArgs;

    fn schema() -> Value {
        object(json!({ "cursor": cursor_schema() }), &[])
    }

    fn run(ctx: &ExecCtx, _: &Seat, args: ChannelsArgs) -> Result<Run, Error> {
        match ask(
            ctx,
            Query::Channels {
                page: page(&args.cursor)?,
            },
        )? {
            Reply::Channels(rows) => Ok(Run::Done(json!({
                "channels": rows.items.iter().map(channel).collect::<Vec<_>>(),
                "next": next(&rows.next),
            }))),
            other => Err(unexpected(chat::MODULE, "Channels", &other)),
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct MessagesArgs {
    channel: String,
    cursor: Option<String>,
}

pub(crate) struct Messages;

impl Tool for Messages {
    const NAME: &'static str = "chat_messages";
    const DESCRIPTION: &'static str = "Read a chat channel's messages, newest first, without their thread replies (chat_thread reads those).";
    type Args = MessagesArgs;

    fn schema() -> Value {
        object(
            json!({ "channel": channel_schema(), "cursor": cursor_schema() }),
            &["channel"],
        )
    }

    fn run(ctx: &ExecCtx, seat: &Seat, args: MessagesArgs) -> Result<Run, Error> {
        let asked = Query::Roots {
            channel_id: args.channel,
            viewer: vec![agent(seat)],
            page: page(&args.cursor)?,
        };
        match ask(ctx, asked)? {
            Reply::Roots(rows) => Ok(Run::Done(messages(&rows))),
            other => Err(unexpected(chat::MODULE, "Roots", &other)),
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct ThreadArgs {
    channel: String,
    root: u64,
    cursor: Option<String>,
}

pub(crate) struct Thread;

impl Tool for Thread {
    const NAME: &'static str = "chat_thread";
    const DESCRIPTION: &'static str = "Read a chat message and the replies in its thread.";
    type Args = ThreadArgs;

    fn schema() -> Value {
        object(
            json!({
                "channel": channel_schema(),
                "root": { "type": "integer", "description": "the seq of the message that opened the thread" },
                "cursor": cursor_schema(),
            }),
            &["channel", "root"],
        )
    }

    fn run(ctx: &ExecCtx, seat: &Seat, args: ThreadArgs) -> Result<Run, Error> {
        let asked = Query::Thread {
            channel_id: args.channel,
            root_seq: args.root,
            viewer: vec![agent(seat)],
            page: page(&args.cursor)?,
        };
        match ask(ctx, asked)? {
            Reply::Thread { root, replies } => Ok(Run::Done(json!({
                "root": root.as_ref().map(message),
                "replies": replies.items.iter().map(message).collect::<Vec<_>>(),
                "next": next(&replies.next),
            }))),
            other => Err(unexpected(chat::MODULE, "Thread", &other)),
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct SearchArgs {
    text: String,
    channel: Option<String>,
    cursor: Option<String>,
}

pub(crate) struct Search;

impl Tool for Search {
    const NAME: &'static str = "chat_search";
    const DESCRIPTION: &'static str =
        "Search chat messages for words, across every channel or in one.";
    type Args = SearchArgs;

    fn schema() -> Value {
        object(
            json!({
                "text": { "type": "string", "description": "the words to find" },
                "channel": channel_schema(),
                "cursor": cursor_schema(),
            }),
            &["text"],
        )
    }

    fn run(ctx: &ExecCtx, seat: &Seat, args: SearchArgs) -> Result<Run, Error> {
        let asked = Query::Search {
            text: args.text,
            viewer: vec![agent(seat)],
            channel_id: args.channel,
            page: page(&args.cursor)?,
        };
        match ask(ctx, asked)? {
            Reply::Hits(hits) => Ok(Run::Done(json!({
                "messages": hits.hits.iter().map(message).collect::<Vec<_>>(),
                "capped": hits.capped,
            }))),
            other => Err(unexpected(chat::MODULE, "Search", &other)),
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct PostArgs {
    channel: String,
    text: String,
    reply_to: Option<u64>,
}

pub(crate) struct Post;

impl Tool for Post {
    const NAME: &'static str = "chat_post";
    const DESCRIPTION: &'static str = "Post a message to a chat channel, or a reply in a message's thread. Markdown works; `<@N>` mentions account N. The message is yours, and chat allows it where it allows your account.";
    const WRITES: bool = true;
    type Args = PostArgs;

    fn schema() -> Value {
        object(
            json!({
                "channel": channel_schema(),
                "text": { "type": "string" },
                "reply_to": { "type": "integer", "description": "the seq of the message whose thread this replies in" },
            }),
            &["channel", "text"],
        )
    }

    fn run(_: &ExecCtx, seat: &Seat, args: PostArgs) -> Result<Run, Error> {
        let op = chat::Op::PostMessage {
            channel_id: args.channel,
            message_id: format!("harness-{}-{}", seat.thread, seat.call),
            blocks: chat::parse_message(&args.text),
            thread: args.reply_to,
        };
        Ok(Run::Emit {
            target: chat::MODULE.into(),
            payload: abi::encode(&op),
        })
    }
}

#[derive(Deserialize)]
pub(crate) struct ReactArgs {
    channel: String,
    seq: u64,
    emoji: String,
}

pub(crate) struct React;

impl Tool for React {
    const NAME: &'static str = "chat_react";
    const DESCRIPTION: &'static str = "React to a chat message with an emoji, as yourself.";
    const WRITES: bool = true;
    type Args = ReactArgs;

    fn schema() -> Value {
        object(
            json!({
                "channel": channel_schema(),
                "seq": { "type": "integer" },
                "emoji": { "type": "string" },
            }),
            &["channel", "seq", "emoji"],
        )
    }

    fn run(_: &ExecCtx, _: &Seat, args: ReactArgs) -> Result<Run, Error> {
        let op = chat::Op::AddReaction {
            channel_id: args.channel,
            seq: args.seq,
            emoji: args.emoji,
        };
        Ok(Run::Emit {
            target: chat::MODULE.into(),
            payload: abi::encode(&op),
        })
    }
}
