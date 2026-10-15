//! What an account did lately, as the `activity` program indexes it: the newest frames the
//! account acted in, each titled by its own program (`program.describe`) from its payload.
use ::activity::PageRequest;
use serde::{Deserialize, Serialize};
use view::prelude::*;

pub const SHOWN: usize = 8;

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Recent {
    pub now: u64,
    pub items: Vec<Signed>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Signed {
    pub title: String,
    pub height: u64,
    pub time: u64,
}

const DESCRIBED_BYTES: u32 = 4 << 10;

pub async fn recent(host: Host, account: u64) -> Result<Recent, Error> {
    let tip = host
        .ask::<ChainBlocks>(BlockPage {
            before: None,
            limit: 1,
        })
        .await?;
    let acted = ::activity::ask::Acted {
        account,
        page: PageRequest::first(SHOWN as u64),
    };
    let frames = host.query(acted).await?.items;
    let mut items = Vec::with_capacity(frames.len());
    for frame in frames {
        let at = TxRef::At {
            height: frame.height,
            position: frame.position,
        };
        let ask = TxAsk {
            tx: at,
            most: DESCRIBED_BYTES,
        };
        let tx = host.ask::<ChainTx>(ask).await.ok().flatten();
        items.push(Signed {
            title: title(&host, &frame.target, tx.map(|located| located.tx)).await,
            height: frame.height,
            time: frame.time,
        });
    }
    Ok(Recent {
        now: tip.blocks.first().map_or(0, |block| block.time),
        items,
    })
}

async fn title(host: &Host, target: &str, tx: Option<Tx>) -> String {
    let Some(tx) = tx else {
        return target.to_owned();
    };
    let size = design::plural(u64::from(tx.size), "byte", "bytes");
    let Some(payload) = tx.payload else {
        return format!("{target} · {size}");
    };
    match host
        .ask::<ProgramDescribe>((target.to_owned(), payload))
        .await
    {
        Ok(Some(op)) => op.title,
        Ok(None) | Err(_) => format!("{target} · {size}"),
    }
}
