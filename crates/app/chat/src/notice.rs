//! What a posted message tells its readers' devices: chat's notices, which `push` carries.
//!
//! What becomes a notice, for whom, its words, its tag, its link and when it is retracted are
//! decided by each program in its own code; this is chat's, and [`of`] is its one rule. chat's
//! program sends what [`of`] returns as a new message is posted (never on an edit, a delete or a
//! reaction); a reader's notifier on the desktop calls [`of`] over the same row and keeps the
//! notices whose `to` names its reader, so both paths say the same thing.
//!
//! A message is news to the accounts it mentions and, in a direct room, to the counterpart: never
//! to its author, and nothing for a deleted row (the accounts are [`relations`], the one walk
//! attribution is told). The counterpart's notice is titled with the author's name and tagged
//! `@<author>`; a mention reads "<author> mentioned you", tagged `#<channel name>`, and a
//! counterpart who is also mentioned gets the direct one alone. The body is the message as names
//! read it ([`message_body`]), the link `[<channel>, <seq>]`.
use std::collections::BTreeSet;

use ::program::{Error, ExecCtx, QueryCtx, Reply, unexpected_reply};
use abi::role::identity as role;
use attribution::Reason;
use push::Notice;

use crate::attributed::{DIRECT, mentioned, relations};
use crate::view::{Names, message_body};
use crate::{ChannelRow, MsgRow};

/// The notices `row`, posted in the channel named `channel`, makes, worded with `names` (the
/// author's and every mentioned account's profile).
pub fn of(row: &MsgRow, channel: &str, names: &Names) -> Vec<Notice> {
    let direct_reason = Reason::Defined(DIRECT.into());
    let (direct, mentions): (Vec<_>, Vec<_>) = relations(row)
        .into_iter()
        .partition(|relation| relation.reason == direct_reason);
    let direct: Vec<u64> = direct.into_iter().map(|r| r.recipient).collect();
    let mentioned: Vec<u64> = mentions
        .into_iter()
        .map(|r| r.recipient)
        .filter(|number| !direct.contains(number))
        .collect();
    let author = names.author(&row.author);
    let body = message_body(&row.blocks, names);
    let link = vec![row.channel_id.clone(), row.seq.to_string()];
    let mut notices = Vec::new();
    if !direct.is_empty() {
        let tag = format!("@{author}");
        notices.push(Notice::new(direct, &author, &body, tag, link.clone()));
    }
    if !mentioned.is_empty() {
        let title = format!("{author} mentioned you");
        let tag = format!("#{channel}");
        notices.push(Notice::new(mentioned, &title, &body, tag, link));
    }
    notices
}

/// Hands `push` the notices a new message makes. Sent with [`Reply::Wanted`], whose refusal chat's
/// `reply` absorbs: a refusal there, or a network founded without `push`, never fails the post.
pub(crate) fn send(ctx: &ExecCtx, channel: &ChannelRow, row: &MsgRow) -> Result<(), Error> {
    if relations(row).is_empty() {
        return Ok(());
    }
    let notices = of(row, &channel.name, &names(ctx, row)?);
    ctx.emit(push::PROGRAM, &push::Op::Send { notices }, Reply::Wanted);
    Ok(())
}

/// The profiles a notice names: the author's and every mentioned account's.
fn names(ctx: &QueryCtx, row: &MsgRow) -> Result<Names, Error> {
    let identity = ctx.env().role(abi::role::IDENTITY)?.clone();
    let accounts: BTreeSet<u64> = row
        .author
        .account()
        .into_iter()
        .chain(mentioned(&row.blocks))
        .collect();
    let mut profiles = Vec::new();
    for number in accounts {
        match ctx.query::<role::Query, role::Reply>(&identity, &role::Query::Profile(number))? {
            role::Reply::Profile(profile) => profiles.extend(profile),
            other => return Err(unexpected_reply(&identity, "Profile", &other)),
        }
    }
    Ok(Names::from_roster(profiles))
}
