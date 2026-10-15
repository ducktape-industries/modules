//! The writes besides a commit: staging one chunk of a large file, and naming a
//! snapshot with a pin or dropping one.
use ::program::{
    BlobId, Error, ExecCtx, Principal, already_exists, invalid, not_found, unauthorized,
};

use crate::objects::put_chunk;
use crate::state::{PINS, minted};
use crate::{CHUNK_SIZE, PinRow};

pub(crate) fn stage(ctx: &ExecCtx, bytes: &[u8]) -> Result<(), Error> {
    let fits = !bytes.is_empty() && bytes.len() as u64 <= CHUNK_SIZE;
    if !fits {
        return Err(invalid(format!("a chunk is 1 to {CHUNK_SIZE} bytes")));
    }
    let chunk = put_chunk(ctx, bytes)?;
    ctx.set_return_data(abi::encode(&chunk));
    Ok(())
}

pub(crate) fn pin(
    ctx: &ExecCtx,
    owner: Principal,
    name: String,
    snapshot: BlobId,
) -> Result<(), Error> {
    let readable = !name.is_empty() && !name.chars().any(char::is_control);
    if !readable {
        return Err(invalid(
            "a pin's name is not empty and has no control characters",
        ));
    }
    minted(ctx, &snapshot)?;
    if PINS.has(ctx, &name) {
        return Err(already_exists(format!("a pin named {name} exists")));
    }
    let row = PinRow {
        name: name.clone(),
        snapshot,
        owner,
        height: ctx.env().height,
    };
    PINS.put(ctx, &name, &row);
    Ok(())
}

pub(crate) fn unpin(ctx: &ExecCtx, by: &Principal, name: String) -> Result<(), Error> {
    let Some(row) = PINS.get(ctx, &name)? else {
        return Err(not_found(format!("no pin named {name}")));
    };
    let allowed = row.owner == *by || *by == Principal::Root;
    if !allowed {
        return Err(unauthorized(format!("pin {name} is its owner's to remove")));
    }
    PINS.remove(ctx, &name);
    Ok(())
}
