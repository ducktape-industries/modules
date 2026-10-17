//! What an account did lately, as the `activity` program indexes it: the newest frames the
//! account acted in, each the program it went to, whether it applied and its place on the chain.
//! A phone's rows are titled by what each transaction did, as the Expo app's are; the desk's
//! list their frames and read no transaction (#378).
use ::activity::{Frame, PageRequest};
use serde::{Deserialize, Serialize};
use view::prelude::*;

pub const SHOWN: usize = 8;
/// A transaction's payload is read up to this many bytes for its title
/// (Expo's `ROW_PAYLOAD`, `query/page.ts:9`); past it, it is titled by
/// its size.
const ROW_PAYLOAD: u32 = 4 << 10;

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Recent {
    pub now: u64,
    pub items: Vec<Signed>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Signed {
    pub target: String,
    /// What the transaction did, as its program describes it, else its
    /// program and size (Expo's `signed`, `members.ts:55-60`); read for a
    /// phone only, `None` on the desk
    pub title: Option<String>,
    pub applied: bool,
    pub height: u64,
    pub position: u32,
    pub time: u64,
}

impl Signed {
    /// What the row says the frame did: its transaction's title, or, on
    /// the desk, whose rows are untitled, the program it went to.
    pub fn title(&self) -> &str {
        self.title.as_deref().unwrap_or(&self.target)
    }
}

impl From<Frame> for Signed {
    fn from(frame: Frame) -> Self {
        Signed {
            target: frame.target,
            title: None,
            applied: frame.applied,
            height: frame.height,
            position: frame.position,
            time: frame.time,
        }
    }
}

/// `account`'s newest frames, each titled (`titled`, a phone) by
/// what its transaction did.
pub async fn recent(host: Host, account: u64, titled: bool) -> Result<Recent, Error> {
    let tip = host
        .ask::<ChainBlocks>(BlockPage {
            before: None,
            limit: 1,
            most: 0,
        })
        .await?;
    let acted = ::activity::ask::Acted {
        account,
        page: PageRequest::first(SHOWN as u64),
    };
    let frames = host.query(acted).await?.items;
    let mut items = Vec::with_capacity(frames.len());
    for frame in frames {
        let title = match titled {
            false => None,
            true => match title(&host, &frame).await? {
                Some(title) => Some(title),
                // Expo lists only the frames whose transaction it found
                None => continue,
            },
        };
        items.push(Signed {
            title,
            ..Signed::from(frame)
        });
    }
    Ok(Recent {
        now: tip.blocks.first().map_or(0, |block| block.time),
        items,
    })
}

/// What `frame`'s transaction did: its payload as its program describes
/// it, else its program and size; `None` where the chain has no
/// transaction at its place. Read one after another, as Expo reads them
/// (`query/activity.ts:26-31`); the describe is the host's, read from the
/// program's own code, and costs no node read.
async fn title(host: &Host, frame: &Frame) -> Result<Option<String>, Error> {
    let at = TxAsk {
        tx: TxRef::At {
            height: frame.height,
            position: frame.position,
        },
        most: ROW_PAYLOAD,
    };
    let Some(Located { tx, .. }) = host.ask::<ChainTx>(at).await? else {
        return Ok(None);
    };
    let described = match tx.payload {
        Some(payload) => {
            host.ask::<ProgramDescribe>((tx.target.clone(), payload))
                .await
        }
        None => Ok(None),
    };
    // a describe that cannot read the bytes leaves the size, as Expo's does
    let described = described.unwrap_or_else(|refusal| {
        // SDK gap: Context::log_refused's line; a read holds a Host, not a
        // Context; the SDK builds it later
        let name = <crate::Members as View>::NAME;
        host.log(format!("{name}: a description refused: {refusal}"));
        None
    });
    let title = described
        .map(|op| op.title)
        .filter(|title| !title.is_empty());
    Ok(Some(title.unwrap_or_else(|| {
        crate::words::sized(&tx.target, tx.size.into())
    })))
}
