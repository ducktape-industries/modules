//! What an account did lately, as the `activity` program indexes it: the newest frames the
//! account acted in, each titled by its own program (`module.describe`) from its block's payload.
use ::activity::PageRequest;
use ducktape_view_guest::prelude::*;
use serde::{Deserialize, Serialize};

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
    let mut block: Option<Block> = None;
    let mut items = Vec::with_capacity(frames.len());
    for frame in frames {
        let held = block
            .as_ref()
            .is_some_and(|block| block.height == frame.height);
        if !held {
            let at = BlockRef::Height(frame.height);
            block = host.ask::<ChainBlock>(at).await.ok().flatten();
        }
        let payload = block
            .as_ref()
            .and_then(|block| block.txs.get(frame.position as usize))
            .map(|tx| tx.payload.clone());
        items.push(Signed {
            title: title(&host, &frame.target, payload).await,
            height: frame.height,
            time: frame.time,
        });
    }
    Ok(Recent {
        now: tip.first().map_or(0, |block| block.time),
        items,
    })
}

async fn title(host: &Host, target: &str, payload: Option<Vec<u8>>) -> String {
    let Some(payload) = payload else {
        return target.to_owned();
    };
    let size = design::plural(payload.len() as u64, "byte", "bytes");
    match host
        .ask::<ModuleDescribe>((target.to_owned(), payload))
        .await
    {
        Ok(Some(op)) => op.title,
        Ok(None) | Err(_) => format!("{target} · {size}"),
    }
}
