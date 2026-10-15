//! What an account did lately, as the `activity` program indexes it: the newest frames the
//! account acted in, each the program it went to, whether it applied and its place on the chain.
use ::activity::{Frame, PageRequest};
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
    pub target: String,
    pub applied: bool,
    pub height: u64,
    pub position: u32,
    pub time: u64,
}

impl From<Frame> for Signed {
    fn from(frame: Frame) -> Self {
        Signed {
            target: frame.target,
            applied: frame.applied,
            height: frame.height,
            position: frame.position,
            time: frame.time,
        }
    }
}

pub async fn recent(host: Host, account: u64) -> Result<Recent, Error> {
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
    Ok(Recent {
        now: tip.blocks.first().map_or(0, |block| block.time),
        items: frames.into_iter().map(Signed::from).collect(),
    })
}
