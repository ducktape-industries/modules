//! The one-time move of a network whose chat kept no thread participants: the vote records
//! each thread's repliers, with the seq of their first reply, from the replies chat holds.
//! Deleted once every network ran it.
use ::program::{Error, ExecCtx};

use crate::state::{MESSAGES, PARTICIPANTS, REPLIES};

pub(crate) fn migrate(ctx: &ExecCtx) -> Result<(), Error> {
    ctx.env().authority()?;
    for (channel, root, seq) in REPLIES.all(ctx)? {
        let Some(reply) = MESSAGES.get(ctx, &(channel.clone(), seq))? else {
            continue;
        };
        let taking_part = (channel, root, reply.author);
        let earlier = PARTICIPANTS
            .get(ctx, &taking_part)?
            .is_some_and(|first| first <= seq);
        if !earlier {
            PARTICIPANTS.put(ctx, &taking_part, &seq);
        }
    }
    Ok(())
}
