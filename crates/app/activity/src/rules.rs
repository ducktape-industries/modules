// The rules: a frame kept under its key and its account, each holding its newest rows, and the
// pages and counts read back.

use guest::{AccountNumber, Error, ExecCtx, ModuleId, QueryCtx, hook};
use store::{KeyCodec, Map, PageRequest, PageResponse};

use crate::{Frame, KEPT, MAX_PAGE, Used};

const SIGNED: Map<(Vec<u8>, u64, u32), Frame> = Map::new("k/");
const SIGNED_HELD: Map<Vec<u8>, u64> = Map::new("kn/");
const ACTED: Map<(AccountNumber, u64, u32), Frame> = Map::new("a/");
const ACTED_HELD: Map<AccountNumber, u64> = Map::new("an/");
const USED: Map<(AccountNumber, ModuleId), u64> = Map::new("u/");

fn newest_first(frame: &Frame) -> (u64, u32) {
    (u64::MAX - frame.height, u32::MAX - frame.position)
}

pub(crate) fn keep(ctx: &ExecCtx, record: hook::Record) -> Result<(), Error> {
    let frame = Frame {
        height: record.height,
        position: record.position,
        time: ctx.env().time,
        target: record.target,
        applied: record.applied,
    };
    let (height, position) = newest_first(&frame);
    let signer = record.signer;
    hold(
        ctx,
        SIGNED_HELD,
        SIGNED,
        &signer,
        (signer.clone(), height, position),
        &frame,
    )?;
    let Some(account) = record.account else {
        return Ok(());
    };
    hold(
        ctx,
        ACTED_HELD,
        ACTED,
        &account,
        (account, height, position),
        &frame,
    )?;
    let used = (account, frame.target);
    let frames = USED.get(ctx, &used)?.unwrap_or(0);
    USED.put(ctx, &used, &(frames + 1));
    Ok(())
}

fn hold<H: KeyCodec, K: KeyCodec>(
    ctx: &ExecCtx,
    held: Map<H, u64>,
    rows: Map<K, Frame>,
    head: &H,
    key: K,
    frame: &Frame,
) -> Result<(), Error> {
    let count = held.get(ctx, head)?.unwrap_or(0);
    let full = count >= KEPT;
    if full {
        let oldest = rows.scan(ctx, rows.prefix_of(head).reverse().limit(1))?;
        for (key, _) in oldest {
            rows.remove(ctx, &key);
        }
    } else {
        held.put(ctx, head, &(count + 1));
    }
    rows.put(ctx, &key, frame);
    Ok(())
}

pub(crate) fn acted(
    ctx: &QueryCtx,
    account: AccountNumber,
    page: &PageRequest,
) -> Result<PageResponse<Frame>, Error> {
    Ok(ACTED
        .range_of(ctx, &account, &page.bounded(MAX_PAGE))?
        .map(|(_, frame)| frame))
}

pub(crate) fn signed(
    ctx: &QueryCtx,
    key: Vec<u8>,
    page: &PageRequest,
) -> Result<PageResponse<Frame>, Error> {
    Ok(SIGNED
        .range_of(ctx, &key, &page.bounded(MAX_PAGE))?
        .map(|(_, frame)| frame))
}

pub(crate) fn used(ctx: &QueryCtx, page: &PageRequest) -> Result<PageResponse<Used>, Error> {
    Ok(USED
        .range(ctx, &page.bounded(MAX_PAGE))?
        .map(|((account, program), frames)| Used {
            account,
            program,
            frames,
        }))
}
