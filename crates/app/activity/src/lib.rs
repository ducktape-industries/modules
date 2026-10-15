//! The `activity` program: where each frame a block carried sits on the chain, kept under the key
//! that signed it and the account it acted as, and once more in one chain-wide list, newest
//! first, by the program it went to as well, as the kernel tells its hooks. Views list the
//! chain's latest frames and an account's, and read each frame by its place (`chain.tx`).
mod program;
mod rules;
#[cfg(test)]
mod tests;

pub use program::Activity;
pub use store::{PageRequest, PageResponse};

use ::program::{AccountNumber, ProgramId};
use borsh::{BorshDeserialize, BorshSerialize};

pub const PROGRAM: &str = "activity";

pub type Op = ();

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Frame {
    pub height: u64,
    pub position: u32,
    pub time: u64,
    pub target: ProgramId,
    pub applied: bool,
}

/// A frame and who sent it: a row of the chain-wide list.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Row {
    pub frame: Frame,
    pub signer: Vec<u8>,
    pub account: Option<AccountNumber>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Used {
    pub account: AccountNumber,
    pub program: ProgramId,
    pub frames: u64,
}

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
    /// The chain's frames newest first, every one, or those sent to `program`.
    #[ask(Reply::Recent(PageResponse<Row>))]
    Recent {
        program: Option<ProgramId>,
        page: PageRequest,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Reply {
    Frames(PageResponse<Frame>),
    Used(PageResponse<Used>),
    Recent(PageResponse<Row>),
}

pub fn describe(_op: &Op) -> describe::Description {
    describe::Description {
        title: "Nothing: activity takes no op".into(),
        fields: Vec::new(),
    }
}

describe::export!(Op, describe);
