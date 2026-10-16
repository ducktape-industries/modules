//! A turn's reply as the model streams it: each event the egress role hands harness
//! (`Op::Streamed`) is checked against the thread's model call and folded, in its wire, into
//! the thread's [`Partial`], which the turn is built from once the call resolves.
use ::program::{Error, ExecCtx, unauthorized, wrong_state};

use crate::state::{PARTIALS, THREADS};
use crate::turns::bound;
use crate::{Asking, Event, Flow, Partial, wire};

pub(crate) fn streamed(ctx: &ExecCtx, thread: u64, event: Event) -> Result<(), Error> {
    let from_egress = ctx.env().sending_program()? == bound(ctx)?;
    if !from_egress {
        return Err(unauthorized(
            "only the program bound to the egress role streams a model's reply",
        ));
    }
    let asked = THREADS
        .get(ctx, &thread)?
        .is_some_and(|row| matches!(row.asking, Asking::Asked { .. }));
    let Some(partial) = PARTIALS.get(ctx, &thread)?.filter(|_| asked) else {
        return Err(wrong_state(format!(
            "thread {thread} is waiting on no model reply"
        )));
    };
    PARTIALS.put(ctx, &thread, &fold(partial, &event));
    Ok(())
}

enum Arrival {
    Repeat,
    Restart,
    Next,
    Gap,
}

fn arrival(next: u64, seq: u64) -> Arrival {
    let restarted = seq == 0 && next > 0;
    if restarted {
        return Arrival::Restart;
    }
    match seq.cmp(&next) {
        std::cmp::Ordering::Less => Arrival::Repeat,
        std::cmp::Ordering::Equal => Arrival::Next,
        std::cmp::Ordering::Greater => Arrival::Gap,
    }
}

pub(crate) fn fold(mut partial: Partial, event: &Event) -> Partial {
    match arrival(partial.next_seq, event.seq) {
        Arrival::Repeat => partial,
        Arrival::Restart => {
            let mut restarted = Partial::new(partial.wire);
            grow(&mut restarted, event);
            restarted
        }
        Arrival::Next => {
            grow(&mut partial, event);
            partial
        }
        Arrival::Gap => {
            gapped(&mut partial, event.seq);
            partial
        }
    }
}

fn gapped(partial: &mut Partial, seq: u64) {
    let reason = format!(
        "event {seq} arrived where event {} was due",
        partial.next_seq
    );
    match partial.flow {
        Flow::Open => partial.flow = Flow::Broken { reason },
        Flow::Ended | Flow::Overflowed | Flow::Broken { .. } | Flow::Failed { .. } => {}
    }
}

fn grow(partial: &mut Partial, event: &Event) {
    partial.next_seq = event.seq + 1;
    match partial.flow {
        Flow::Open => wire::fold(partial, &event.data),
        Flow::Ended | Flow::Overflowed | Flow::Broken { .. } | Flow::Failed { .. } => {}
    }
}

#[cfg(test)]
mod tests;
