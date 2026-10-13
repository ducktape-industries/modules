//! The `activity` module: where each frame a block carried sits on the chain, kept under the key
//! that signed it and the account it acted as (the newest `KEPT` of each), as the kernel tells
//! its hooks. Views list what an account did and read each frame by its place (`chain.tx`).
mod program;
mod rules;
#[cfg(test)]
mod tests;

pub use program::Activity;
pub use store::{PageRequest, PageResponse};

use borsh::{BorshDeserialize, BorshSerialize};
use guest::{AccountNumber, ModuleId};

pub const MODULE: &str = "activity";

pub type Op = ();

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Frame {
    pub height: u64,
    pub position: u32,
    pub time: u64,
    pub target: ModuleId,
    pub applied: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Used {
    pub account: AccountNumber,
    pub program: ModuleId,
    pub frames: u64,
}

pub const MAX_PAGE: u64 = 256;
pub const KEPT: u64 = 1_000;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, ::program::Ask)]
#[ask(Activity)]
pub enum Query {
    #[ask(Reply::Frames(PageResponse<Frame>))]
    Acted {
        account: AccountNumber,
        page: PageRequest,
    },
    #[ask(Reply::Frames(PageResponse<Frame>))]
    Signed { key: Vec<u8>, page: PageRequest },
    #[ask(Reply::Used(PageResponse<Used>))]
    Used { page: PageRequest },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Reply {
    Frames(PageResponse<Frame>),
    Used(PageResponse<Used>),
}

pub fn describe(_op: &Op) -> describe::Description {
    describe::Description {
        title: "Nothing: activity takes no op".into(),
        fields: Vec::new(),
    }
}

describe::export!(Op, describe);
